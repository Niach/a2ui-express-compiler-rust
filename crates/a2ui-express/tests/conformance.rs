//! The upstream Express conformance suite, run against this crate.
//!
//! Loads `vendor/a2ui/conformance/agent/express/*.yaml`, runs every case
//! through the calls in `conformance/sdk.rs`, and prints the pass count per
//! suite. As upstream's Python and TypeScript harnesses do, each case runs as
//! written (`v1.0`) and again lowered to v0.9 (`v0.9-lowered`), unless it
//! cannot be lowered.
//!
//! `conformance/known_failures.txt` (at the repository root) lists the cases
//! expected to fail, as `<case>` (every version) or `<case>::<version>`. The
//! run fails when a case not listed fails, when a listed case passes, or when
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
#[path = "conformance/lowering.rs"]
mod lowering;
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
use run::{Version, run_case};

fn known_failures_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/known_failures.txt")
}

const KNOWN_FAILURES_HEADER: &str = "\
# Upstream Express conformance cases expected to fail.
#
# One entry per line: `<case>` fails in every version it runs in, and
# `<case>::v1.0` or `<case>::v0.9-lowered` in that version only. The
# conformance test fails when a listed case passes, so remove its entry when
# you fix it. Regenerate with:
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
    Skip(String),
}

struct Record<'a> {
    case: &'a Case,
    version: Version,
    outcome: Outcome,
}

fn run_guarded(case: &Case, version: Version) -> Outcome {
    match catch_unwind(AssertUnwindSafe(|| run_case(case, version))) {
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

fn versions_for(case: &Case) -> &'static [Version] {
    // Upstream times the scaling cases as written only.
    if case.action == "measure_growth" {
        &Version::ALL[..1]
    } else {
        &Version::ALL
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
            for &version in versions_for(case) {
                let outcome = match version {
                    Version::V0_9Lowered => match lowering::v09_skip_reason(case) {
                        Some(reason) => Outcome::Skip(reason),
                        None => run_guarded(case, version),
                    },
                    Version::V1_0 => run_guarded(case, version),
                };
                records.push(Record {
                    case,
                    version,
                    outcome,
                });
            }
        }
    }

    // Check the outcomes against the known failures.
    let mut used: BTreeSet<&str> = BTreeSet::new();
    let mut unexpected_failures = Vec::new();
    let mut unexpected_passes = Vec::new();
    for record in &records {
        let qualified = format!("{}::{}", record.case.name, record.version.key());
        let entry: Option<&str> = known
            .get(&qualified)
            .or_else(|| known.get(&record.case.name))
            .map(String::as_str);
        if let Some(entry) = entry {
            used.insert(entry);
        }
        match (&record.outcome, entry) {
            (Outcome::Fail(reason), None) => unexpected_failures.push((qualified, reason)),
            (Outcome::Pass, Some(entry)) => unexpected_passes.push((qualified, entry)),
            (Outcome::Fail(reason), Some(_)) if verbose => {
                println!(
                    "known failure {}/{qualified}\n{}",
                    record.case.suite,
                    indent(reason)
                );
            }
            (Outcome::Skip(reason), _) if verbose => {
                println!("skipped {}/{qualified}: {reason}", record.case.suite);
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
    for (name, entry) in &unexpected_passes {
        let _ = writeln!(
            report,
            "\nPASS {name}, listed as `{entry}` in known_failures.txt: remove the entry{}",
            if entry.contains("::") {
                ""
            } else {
                " (or narrow it to the version that still fails)"
            }
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

#[derive(Default, Clone, Copy)]
struct Tally {
    pass: usize,
    run: usize,
    skip: usize,
}

impl Tally {
    fn add(&mut self, outcome: &Outcome) {
        match outcome {
            Outcome::Pass => {
                self.pass += 1;
                self.run += 1;
            }
            Outcome::Fail(_) => self.run += 1,
            Outcome::Skip(_) => self.skip += 1,
        }
    }

    fn show(self) -> String {
        if self.run == 0 && self.skip == 0 {
            return "-".to_owned();
        }
        let mut s = format!("{}/{}", self.pass, self.run);
        if self.skip > 0 {
            let _ = write!(s, " ({} skipped)", self.skip);
        }
        s
    }
}

fn print_summary(suites: &[(&str, Vec<Case>)], records: &[Record]) {
    let mut by_suite: BTreeMap<(&str, &str), Tally> = BTreeMap::new();
    let mut totals: BTreeMap<&str, Tally> = BTreeMap::new();
    for record in records {
        let key = record.version.key();
        by_suite
            .entry((record.case.suite, key))
            .or_default()
            .add(&record.outcome);
        totals.entry(key).or_default().add(&record.outcome);
    }
    let (v10, v09) = (Version::V1_0.key(), Version::V0_9Lowered.key());
    println!("\nExpress conformance (passed/run)");
    println!("{:<20} {:<14} {}", "suite", v10, v09);
    for (suite, _) in suites {
        let get = |v| by_suite.get(&(*suite, v)).copied().unwrap_or_default();
        println!("{:<20} {:<14} {}", suite, get(v10).show(), get(v09).show());
    }
    let get = |v| totals.get(v).copied().unwrap_or_default();
    println!(
        "{:<20} {:<14} {}",
        "total",
        get(v10).show(),
        get(v09).show()
    );
}

fn write_known_failures(records: &[Record]) -> Result<PathBuf, String> {
    // Per case, in suite order: the versions it fails in and the ones it ran in.
    let mut order: Vec<&str> = Vec::new();
    let mut failing: BTreeMap<&str, (Vec<&str>, usize)> = BTreeMap::new();
    for record in records {
        let name = record.case.name.as_str();
        if !failing.contains_key(name) {
            order.push(name);
        }
        let entry = failing.entry(name).or_default();
        match record.outcome {
            Outcome::Fail(_) => {
                entry.0.push(record.version.key());
                entry.1 += 1;
            }
            Outcome::Pass => entry.1 += 1,
            Outcome::Skip(_) => {}
        }
    }
    let mut text = KNOWN_FAILURES_HEADER.to_owned();
    let mut suite = "";
    for record in records {
        let name = record.case.name.as_str();
        if order.first() != Some(&name) {
            continue;
        }
        order.remove(0);
        let (versions, ran) = &failing[name];
        if versions.is_empty() {
            continue;
        }
        if record.case.suite != suite {
            suite = record.case.suite;
            let _ = write!(text, "\n# {suite}\n");
        }
        if versions.len() == *ran {
            let _ = writeln!(text, "{name}");
        } else {
            for v in versions {
                let _ = writeln!(text, "{name}::{v}");
            }
        }
    }
    let path = known_failures_path();
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}
