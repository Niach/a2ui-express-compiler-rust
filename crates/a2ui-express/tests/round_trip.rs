//! The decompiler's round-trip property: `compile(decompile(x)) == x`.
//!
//! `x` is every message list the upstream suites hold (each `expect` of
//! `compiler.yaml`, each `messages` of `decompiler.yaml`) and the A2UI v1.0
//! messages of eui's `a2ui10-messages.json`. Each is decompiled in four
//! ways, `Order::Upstream` and `Order::TopDown`, positional and with
//! `keyword_args`, and compiled back.
//!
//! - `Upstream` must reproduce `x` exactly.
//! - `TopDown` must reproduce it up to the order of each component list (an
//!   A2UI component list carries no order, and the compiler writes
//!   components in source order). It must also do so when `x`'s component
//!   lists are reversed first: the order it writes depends on the tree, not
//!   on the order the components arrive in.
//!
//! Some suite inputs cannot round-trip, because they are not what the
//! compiler would write (a legacy spelling, split messages the decompiler
//! joins, an id it has to rename). `round_trip/not_round_trippable.txt`
//! lists them with the reason; the test fails when a listed case
//! round-trips after all, or names a case that does not exist.

// The conformance harness modules are shared; this test uses part of them.
#![allow(dead_code)]

#[path = "conformance/cases.rs"]
mod cases;
#[path = "conformance/check.rs"]
mod check;
#[path = "conformance/lowering.rs"]
mod lowering;
#[path = "conformance/sdk.rs"]
mod sdk;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use a2ui_express::compile::Compiler;
use a2ui_express::decompile::{
    DecompileOptions, Order, assign_variable_names, decompile, is_uax31_identifier,
};
use a2ui_express::schema::Catalog;
use a2ui_express::syntax::lexer::is_identifier;
use serde_json::Value;

use cases::{load_suite, resolve_catalogs};
use check::json_eq;

const SURFACE_ID: &str = "default_surface";

const MODES: [(Order, bool); 4] = [
    (Order::Upstream, false),
    (Order::Upstream, true),
    (Order::TopDown, false),
    (Order::TopDown, true),
];

fn manifest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// The component lists of `messages`, rewritten by `f`.
fn map_component_lists(messages: &[Value], f: impl Fn(&mut Vec<Value>)) -> Vec<Value> {
    let mut messages = messages.to_vec();
    for message in &mut messages {
        for key in ["createSurface", "updateComponents"] {
            if let Some(Value::Array(list)) =
                message.get_mut(key).and_then(|b| b.get_mut("components"))
            {
                f(list);
            }
        }
    }
    messages
}

fn sorted_by_id(messages: &[Value]) -> Vec<Value> {
    map_component_lists(messages, |list| {
        list.sort_by_key(|c| c.get("id").and_then(Value::as_str).unwrap_or("").to_owned());
    })
}

fn reversed(messages: &[Value]) -> Vec<Value> {
    map_component_lists(messages, |list| list.reverse())
}

/// `compile(decompile(input)) == expected` in one mode, or why not.
fn round_trip(
    compiler: &Compiler,
    surface_id: &str,
    input: &[Value],
    expected: &[Value],
    (order, keyword_args): (Order, bool),
) -> Result<(), String> {
    let mode = format!("{order:?}, keyword_args={keyword_args}");
    let options = DecompileOptions {
        keyword_args,
        order,
    };
    let express = decompile(compiler.catalogs(), input, &options)
        .map_err(|e| format!("[{mode}] decompile: {e}"))?;
    let compiled = compiler
        .compile(&express, surface_id)
        .map_err(|e| format!("[{mode}] recompile: {}\n{express}", e.message))?;
    let same = match order {
        Order::Upstream => json_eq(
            &Value::Array(compiled.clone()),
            &Value::Array(expected.to_vec()),
        ),
        Order::TopDown => json_eq(
            &Value::Array(sorted_by_id(&compiled)),
            &Value::Array(sorted_by_id(expected)),
        ),
    };
    if same {
        Ok(())
    } else {
        Err(format!(
            "[{mode}] differs\n{express}\n got {}\nwant {}",
            Value::Array(compiled),
            Value::Array(expected.to_vec())
        ))
    }
}

/// Every mode, plus `TopDown` on the reversed component lists.
fn round_trip_all(compiler: &Compiler, surface_id: &str, messages: &[Value]) -> Result<(), String> {
    for mode in MODES {
        round_trip(compiler, surface_id, messages, messages, mode)?;
    }
    for keyword_args in [false, true] {
        round_trip(
            compiler,
            surface_id,
            &reversed(messages),
            messages,
            (Order::TopDown, keyword_args),
        )
        .map_err(|e| format!("reversed: {e}"))?;
    }
    Ok(())
}

/// `<case>` → reason, from `round_trip/not_round_trippable.txt`.
fn not_round_trippable() -> BTreeMap<String, String> {
    let path = manifest().join("tests/round_trip/not_round_trippable.txt");
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            let (case, reason) = l
                .split_once(" -- ")
                .unwrap_or_else(|| panic!("`<case> -- <reason>` expected, got {l:?}"));
            assert!(!reason.trim().is_empty(), "{case}: no reason given");
            (case.trim().to_owned(), reason.trim().to_owned())
        })
        .collect()
}

#[test]
fn conformance_messages_round_trip() {
    let listed = not_round_trippable();
    let mut seen = BTreeSet::new();
    let mut failures = Vec::new();
    let mut unexpected_passes = Vec::new();
    let mut checked = 0;

    for (suite, key) in [("compiler", "expect"), ("decompiler", "messages")] {
        for case in load_suite(suite).unwrap() {
            if case.get("expect_error").is_some() {
                continue;
            }
            let Some(messages) = case.get(key).and_then(Value::as_array) else {
                continue;
            };
            let catalogs = resolve_catalogs(&case.args, false).unwrap();
            let compiler =
                sdk::compiler(&catalogs).unwrap_or_else(|e| panic!("{}: {}", case.name, e.message));
            let result = round_trip_all(&compiler, SURFACE_ID, messages);
            seen.insert(case.name.clone());
            match (result, listed.get(&case.name)) {
                (Ok(()), None) => checked += 1,
                (Ok(()), Some(_)) => unexpected_passes.push(case.name.clone()),
                (Err(_), Some(_)) => {}
                (Err(e), None) => failures.push(format!("{}: {e}", case.name)),
            }
        }
    }

    let unknown: Vec<&String> = listed
        .keys()
        .filter(|c| !c.starts_with("eui/") && !seen.contains(*c))
        .collect();
    assert!(
        unknown.is_empty(),
        "not_round_trippable.txt names cases that do not exist: {unknown:?}"
    );
    assert!(
        unexpected_passes.is_empty(),
        "these round-trip now; remove them from not_round_trippable.txt: {unexpected_passes:?}"
    );
    assert!(
        failures.is_empty(),
        "{} case(s) do not round-trip:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    // The suites hold 125 such cases at the pinned commit; a drop means the
    // loader above stopped finding them.
    assert!(checked >= 100, "only {checked} cases were checked");
    println!(
        "{checked} cases round-trip in every mode; {} listed as not round-trippable",
        listed.keys().filter(|c| !c.starts_with("eui/")).count()
    );
}

// ---- eui's A2UI v1.0 messages ---------------------------------------------

fn eui_fixture() -> Value {
    let path = manifest().join("../../vendor/eui/fixtures/a2ui10-messages.json");
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap()
}

/// Ids that are v1.0 identifiers and that Express does not lex as one
/// `IDENTIFIER`, with the variable name the decompiler writes instead.
///
/// Unicode 15.1 added ZWJ and ZWNJ (U+200D, U+200C) to `XID_Continue`, so
/// `a\u{200d}` is a v1.0 component id in eui, in upstream's TS SDK (the
/// engine's `\p{XID_Continue}`) and in Python's `isidentifier`. Upstream's
/// ANTLR lexer has older tables compiled in and does not lex it, so the
/// decompiler writes it like a reserved word: under a derived name, with
/// `id="a\u{200d}"` (#AXC-28).
const NOT_LEXED: &[(&str, &str)] = &[("a\u{200d}", "a_")];

fn eui_compiler() -> Compiler {
    let path = manifest().join("../../vendor/eui/catalog/core.schema.json");
    let catalog = Catalog::from_json_str(&fs::read_to_string(path).unwrap()).unwrap();
    Compiler::new(vec![catalog], None).unwrap()
}

/// eui's `isIdentifier` agrees with the decompiler's on every id of the
/// fixture, and for an id that is not an identifier the variable name the
/// decompiler derives is the id eui's 0.9 → 1.0 upgrade renames it to
/// (`mangle`). An id that is an identifier keeps its name in both.
///
/// The exception is an id Express does not lex ([`NOT_LEXED`]): eui keeps
/// it, the decompiler has to rename it.
#[test]
fn eui_id_mangling_matches() {
    let fixture = eui_fixture();
    let ids = fixture["ids"].as_array().unwrap();
    assert!(ids.len() >= 20);
    let mut mangled = 0;
    for entry in ids {
        let id = entry["id"].as_str().unwrap();
        let expected = &entry["expected"];
        let identifier = expected["identifier"].as_bool().unwrap();
        let mangle = expected["mangle"].as_str().unwrap();
        assert_eq!(is_uax31_identifier(id), identifier, "identifier {id:?}");
        if identifier {
            assert_eq!(mangle, id, "eui keeps identifier {id:?}");
        } else {
            mangled += 1;
        }
        // Express renames a reserved word once more: `_` is its skip
        // placeholder, so an id eui mangles to `_` is written `__`.
        let want = if let Some((_, name)) = NOT_LEXED.iter().find(|(i, _)| *i == id) {
            assert!(identifier && !is_identifier(id), "Express lexes {id:?} now");
            (*name).to_owned()
        } else if is_identifier(mangle) {
            mangle.to_owned()
        } else {
            format!("_{mangle}")
        };
        assert_eq!(
            assign_variable_names([id])[id],
            want,
            "variable name of {id:?}"
        );
    }
    assert!(mangled >= 8, "only {mangled} non-identifier ids");
}

/// Every id eui reads as an identifier is a v1.0 component id and
/// extension key here too: a surface that uses it compiles back to itself in
/// every mode, whether Express lexes the id (a bare variable name) or not
/// (`id="…"`, a quoted key).
#[test]
fn eui_identifier_ids_round_trip() {
    let fixture = eui_fixture();
    let compiler = eui_compiler();
    let core = compiler.catalogs().catalogs()[0].id().to_owned();
    let mut checked = 0;
    for entry in fixture["ids"].as_array().unwrap() {
        if !entry["expected"]["identifier"].as_bool().unwrap() {
            continue;
        }
        let id = entry["id"].as_str().unwrap();
        let message = serde_json::json!({
            "version": "1.0",
            "createSurface": {
                "surfaceId": "ids",
                "catalogId": core,
                "components": [
                    {"id": "root", "component": "Stack", "children": [id]},
                    {"id": id, "component": "Text", "text": "Hi"},
                ],
                "metadata": {"extensions": {id: 1}},
            }
        });
        round_trip_all(&compiler, "ids", std::slice::from_ref(&message))
            .unwrap_or_else(|e| panic!("id {id:?}: {e}"));
        checked += 1;
    }
    assert!(checked >= 12, "only {checked} identifier ids");
}

/// The v1.0 messages eui's router accepts, against eui's core catalog.
///
/// A step counts when its message is a v1.0 `createSurface`,
/// `updateComponents`, `updateDataModel` or `deleteSurface` that the router
/// answers without an error and that is on a surface of the core catalog
/// (the fixture also exercises inline test catalogs). Steps that cannot
/// round-trip are listed in `not_round_trippable.txt` as
/// `eui/<flow> / step <n>`.
#[test]
fn eui_v1_messages_round_trip() {
    let fixture = eui_fixture();
    let compiler = eui_compiler();
    let core = compiler.catalogs().catalogs()[0].id().to_owned();
    let listed = not_round_trippable();
    let listed: BTreeMap<&str, &String> = listed
        .iter()
        .filter_map(|(k, v)| Some((k.strip_prefix("eui/")?, v)))
        .collect();
    let mut seen = BTreeSet::new();
    let mut checked = 0;
    let mut failures = Vec::new();
    let mut unexpected_passes = Vec::new();

    for flow in fixture["flows"].as_array().unwrap() {
        let flow_name = flow["name"].as_str().unwrap_or("");
        // The catalog each surface of the flow was created with.
        let mut surface_catalog: BTreeMap<String, String> = BTreeMap::new();
        for (index, step) in flow["steps"].as_array().unwrap().iter().enumerate() {
            let message = &step["message"];
            let Some((key, body)) = [
                "createSurface",
                "updateComponents",
                "updateDataModel",
                "deleteSurface",
            ]
            .iter()
            .find_map(|k| message.get(*k).map(|b| (*k, b))) else {
                continue;
            };
            let refused = step["expected"]
                .as_array()
                .is_some_and(|ops| ops.iter().any(|op| op["message"].get("error").is_some()));
            let single = message.as_object().is_some_and(|m| m.len() == 2);
            if message["version"] != "1.0" || refused || !single {
                continue;
            }
            let Some(surface_id) = body["surfaceId"].as_str() else {
                continue;
            };
            if key == "createSurface" {
                let id = body["catalogId"].as_str().unwrap_or("").to_owned();
                surface_catalog.insert(surface_id.to_owned(), id);
            }
            if surface_catalog.get(surface_id) != Some(&core) {
                continue;
            }
            let label = format!("{flow_name} / step {index}");
            let result = round_trip_all(&compiler, surface_id, std::slice::from_ref(message));
            match (result, listed.contains_key(label.as_str())) {
                (Ok(()), false) => checked += 1,
                (Ok(()), true) => unexpected_passes.push(label.clone()),
                (Err(_), true) => {}
                (Err(e), false) => failures.push(format!("{label} ({key}): {e}")),
            }
            seen.insert(label);
        }
    }

    let unknown: Vec<&&str> = listed.keys().filter(|l| !seen.contains(**l)).collect();
    assert!(
        unknown.is_empty(),
        "not_round_trippable.txt names eui steps that do not exist: {unknown:?}"
    );
    assert!(
        unexpected_passes.is_empty(),
        "these round-trip now; remove them from not_round_trippable.txt: {unexpected_passes:?}"
    );
    assert!(
        failures.is_empty(),
        "{} eui message(s) do not round-trip:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    assert!(checked >= 5, "only {checked} eui messages were checked");
    println!(
        "{checked} eui messages round-trip in every mode; {} listed as not round-trippable",
        listed.len()
    );
}
