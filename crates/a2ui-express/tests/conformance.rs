//! The upstream Express conformance suite, run against this crate.
//!
//! Loads `vendor/a2ui/conformance/agent/express/*.yaml`, runs every case
//! through the calls in `conformance/sdk.rs`, and prints the pass count per
//! suite. `conformance/out_of_scope.txt` lists the pre-1.0 cases left out.
//!
//! `conformance/known_failures.txt` (at the repository root) lists the cases
//! expected to fail. The run fails when a case not listed fails, when a listed case passes, or when
//! the list names a case that does not exist, so the list only ever shrinks.
//!
//! ```sh
//! cargo test -p a2ui-express --test conformance                 # summary
//! cargo test -p a2ui-express --test conformance -- decompile    # cases whose name contains "decompile"
//! CONFORMANCE_VERBOSE=1 cargo test -p a2ui-express --test conformance   # every failure's reason
//! CONFORMANCE_BLESS=1 cargo test -p a2ui-express --test conformance     # rewrite known_failures.txt
//! ```

#[path = "conformance/cases.rs"]
mod cases;
#[path = "conformance/check.rs"]
mod check;
#[path = "conformance/run.rs"]
mod run;
#[path = "conformance/sdk.rs"]
mod sdk;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use cases::{Case, SUITES, check_case_keys, load_suite};
use run::run_case;

fn known_failures_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/known_failures.txt")
}

const KNOWN_FAILURES_HEADER: &str = "\
# Upstream Express conformance cases expected to fail.
#
# One case per line. The conformance test fails when a listed case passes, so
# remove its entry when you fix it. Regenerate with:
#
#   CONFORMANCE_BLESS=1 cargo test -p a2ui-express --test conformance
";

fn read_known_failures() -> Result<BTreeSet<String>, String> {
    let path = known_failures_path();
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    Ok(text
        .lines()
        .map(|l| l.split('#').next().unwrap_or_default().trim())
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect())
}

enum Outcome {
    Pass,
    Fail(String),
}

struct Record<'a> {
    case: &'a Case,
    outcome: Outcome,
}

fn run_guarded(case: &Case) -> Outcome {
    match catch_unwind(AssertUnwindSafe(|| run_case(case))) {
        Ok(Ok(())) => Outcome::Pass,
        Ok(Err(reason)) => Outcome::Fail(reason),
        Err(panic) => {
            let message = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                .unwrap_or_else(|| "panic".to_owned());
            Outcome::Fail(format!("panicked: {message}"))
        }
    }
}

fn indent(text: &str) -> String {
    text.lines().map(|l| format!("      {l}\n")).collect()
}

fn main() -> ExitCode {
    let filter: Option<String> = std::env::args().skip(1).find(|a| !a.starts_with('-'));
    let verbose = std::env::var_os("CONFORMANCE_VERBOSE").is_some();
    let bless = std::env::var_os("CONFORMANCE_BLESS").is_some();

    let mut problems: Vec<String> = Vec::new();

    let mut suites: Vec<(&str, Vec<Case>)> = Vec::new();
    for suite in SUITES {
        match load_suite(suite) {
            Ok(cases) => suites.push((suite, cases)),
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        }
    }
    // A key the harness does not understand fails the run outright: the
    // suite changed under it.
    for (_, cases) in &suites {
        for case in cases {
            if let Err(e) = check_case_keys(case) {
                problems.push(format!("{}/{}: {e}", case.suite, case.name));
            }
        }
    }
    if !problems.is_empty() {
        for p in &problems {
            eprintln!("error: {p}");
        }
        return ExitCode::FAILURE;
    }

    let known = match read_known_failures() {
        Ok(known) => known,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let mut records: Vec<Record> = Vec::new();
    for (_, cases) in &suites {
        for case in cases {
            if filter
                .as_ref()
                .is_some_and(|f| !case.name.contains(f.as_str()))
            {
                continue;
            }
            let outcome = run_guarded(case);
            records.push(Record { case, outcome });
        }
    }

    // Check the outcomes against the known failures.
    let mut used: BTreeSet<&str> = BTreeSet::new();
    let mut unexpected_failures = Vec::new();
    let mut unexpected_passes = Vec::new();
    for record in &records {
        let name = record.case.name.as_str();
        let entry: Option<&str> = known.get(name).map(String::as_str);
        if let Some(entry) = entry {
            used.insert(entry);
        }
        match (&record.outcome, entry) {
            (Outcome::Fail(reason), None) => unexpected_failures.push((name, reason)),
            (Outcome::Pass, Some(_)) => unexpected_passes.push(name),
            (Outcome::Fail(reason), Some(_)) if verbose => {
                println!(
                    "known failure {}/{name}\n{}",
                    record.case.suite,
                    indent(reason)
                );
            }
            _ => {}
        }
    }
    let stale: Vec<&String> = if filter.is_none() {
        known
            .iter()
            .filter(|k| !used.contains(k.as_str()))
            .collect()
    } else {
        Vec::new()
    };

    print_summary(&suites, &records);

    if bless && filter.is_none() {
        return match write_known_failures(&records) {
            Ok(path) => {
                println!("\nwrote {}", path.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        };
    }

    let mut report = String::new();
    for (name, reason) in &unexpected_failures {
        let _ = write!(
            report,
            "\nFAIL {name} (not in known_failures.txt)\n{}",
            indent(reason)
        );
    }
    for name in &unexpected_passes {
        let _ = writeln!(
            report,
            "\nPASS {name}, listed in known_failures.txt: remove the entry"
        );
    }
    for entry in &stale {
        let _ = writeln!(
            report,
            "\nSTALE `{entry}` in known_failures.txt names no case that runs: remove it"
        );
    }
    if report.is_empty() {
        ExitCode::SUCCESS
    } else {
        eprintln!("{report}");
        eprintln!(
            "conformance: {} unexpected failures, {} unexpected passes, {} stale entries",
            unexpected_failures.len(),
            unexpected_passes.len(),
            stale.len()
        );
        ExitCode::FAILURE
    }
}

fn print_summary(suites: &[(&str, Vec<Case>)], records: &[Record]) {
    let mut by_suite: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for record in records {
        let tally = by_suite.entry(record.case.suite).or_default();
        tally.0 += usize::from(matches!(record.outcome, Outcome::Pass));
        tally.1 += 1;
    }
    println!("\nExpress conformance (passed/run)");
    for (suite, _) in suites {
        let (pass, run) = by_suite.get(suite).copied().unwrap_or_default();
        println!("{suite:<20} {pass}/{run}");
    }
    let pass: usize = by_suite.values().map(|t| t.0).sum();
    println!("{:<20} {pass}/{}", "total", records.len());
}

fn write_known_failures(records: &[Record]) -> Result<PathBuf, String> {
    let mut text = KNOWN_FAILURES_HEADER.to_owned();
    let mut suite = "";
    for record in records {
        if !matches!(record.outcome, Outcome::Fail(_)) {
            continue;
        }
        if record.case.suite != suite {
            suite = record.case.suite;
            let _ = write!(text, "\n# {suite}\n");
        }
        let _ = writeln!(text, "{}", record.case.name);
    }
    let path = known_failures_path();
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}
