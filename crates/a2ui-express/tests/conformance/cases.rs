//! Loading the vendored suites and resolving what their cases refer to.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::lowering::lower_catalog_document;
use crate::sdk::{Catalog, Transformer};

/// The catalog a case uses when it names none.
pub const DEFAULT_CATALOG: &str = "test_data/catalogs/simplified_catalog_v1_0.json";

/// The Express suites, in the order the summary prints them.
pub const SUITES: &[&str] = &[
    "compiler",
    "decompiler",
    "response_parser",
    "response_streaming",
    "prompt_generator",
    "scaling",
];

/// `vendor/a2ui/conformance/`, which the paths inside the cases are relative to.
pub fn conformance_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/a2ui/conformance")
}

#[derive(Debug, Clone)]
pub struct Case {
    pub suite: &'static str,
    pub name: String,
    pub action: String,
    pub args: Value,
    /// The whole case as JSON.
    pub raw: Value,
}

impl Case {
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.raw.get(key)
    }
}

pub fn load_suite(suite: &'static str) -> Result<Vec<Case>, String> {
    let path = conformance_root().join(format!("agent/express/{suite}.yaml"));
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    // YAML maps onto JSON directly; the suites use no tags or non-string keys.
    let doc: Value =
        serde_yaml_ng::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let Value::Array(items) = doc else {
        return Err(format!("{}: expected a list of cases", path.display()));
    };
    items
        .into_iter()
        .map(|raw| {
            let field = |k: &str| raw.get(k).and_then(Value::as_str).map(str::to_owned);
            let name = field("name").ok_or_else(|| format!("{suite}: case without a name"))?;
            let action = field("action").ok_or_else(|| format!("{suite}/{name}: no action"))?;
            let args = raw.get("args").cloned().unwrap_or_else(|| json!({}));
            Ok(Case {
                suite,
                name,
                action,
                args,
                raw,
            })
        })
        .collect()
}

pub fn read_json(relative: &str) -> Result<Value, String> {
    let path = conformance_root().join(relative);
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn basename(path: &str) -> String {
    let file = path.rsplit('/').next().unwrap_or(path);
    file.strip_suffix(".json").unwrap_or(file).to_owned()
}

/// The catalog document an entry names (a path, an inline document, or a
/// `{catalog, transformers}` registration of either), with the id it defaults
/// to: the file's basename, or `inline`.
pub fn raw_catalog_doc(entry: &Value) -> Result<(Value, String), String> {
    let inner = match entry {
        Value::Object(map) if map.contains_key("catalog") => &map["catalog"],
        other => other,
    };
    match inner {
        Value::String(path) => Ok((read_json(path)?, basename(path))),
        Value::Object(_) => Ok((inner.clone(), "inline".to_owned())),
        other => Err(format!("invalid catalog reference: {other}")),
    }
}

/// The `catalogId` of the first catalog a case registers.
pub fn first_catalog_id(args: &Value) -> Option<String> {
    let default = json!(DEFAULT_CATALOG);
    let entry = args
        .get("catalog")
        .or_else(|| args.get("catalogs").and_then(|c| c.get(0)))
        .unwrap_or(&default);
    let (doc, default_id) = raw_catalog_doc(entry).ok()?;
    Some(
        doc.get("catalogId")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or(default_id),
    )
}

/// Resolves one catalog entry, the way upstream's Python harness does
/// (`catalog_config_from_document`): a document without `protocolVersion` is
/// v1.0 and one without `catalogId` takes the default id. A lowered run lowers
/// the document first, which sets the same default id.
pub fn resolve_catalog(entry: &Value, lower: bool) -> Result<Catalog, String> {
    let mut transformers = Vec::new();
    if let Some(specs) = entry.get("transformers").and_then(Value::as_array) {
        for spec in specs {
            let names = |key: &str| -> Option<Vec<String>> {
                spec.get(key)?.as_array().map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
            };
            if let Some(names) = names("component_pruning") {
                transformers.push(Transformer::ComponentPruning(names));
            } else if let Some(names) = names("function_pruning") {
                transformers.push(Transformer::FunctionPruning(names));
            } else {
                return Err(format!("unknown transformer: {spec}"));
            }
        }
    }
    let (doc, default_id) = raw_catalog_doc(entry)?;
    let mut document = if lower {
        lower_catalog_document(&doc, Some(&default_id))
    } else {
        doc
    };
    if let Some(map) = document.as_object_mut() {
        if !map.contains_key("protocolVersion") {
            map.insert("protocolVersion".into(), json!("1.0"));
        }
        if !map.contains_key("catalogId") {
            map.insert("catalogId".into(), json!(default_id));
        }
    }
    Ok(Catalog {
        document,
        transformers,
    })
}

/// The catalogs a case registers: `args.catalogs`, `args.catalog`, or the
/// default catalog.
pub fn resolve_catalogs(args: &Value, lower: bool) -> Result<Vec<Catalog>, String> {
    match (args.get("catalogs"), args.get("catalog")) {
        (Some(Value::Array(entries)), _) => {
            entries.iter().map(|e| resolve_catalog(e, lower)).collect()
        }
        (_, Some(entry)) => Ok(vec![resolve_catalog(entry, lower)?]),
        _ => Ok(vec![resolve_catalog(&json!(DEFAULT_CATALOG), lower)?]),
    }
}

/// Fails on a key the harness does not know, so a re-synced suite that adds
/// an expectation cannot pass by having it ignored (upstream's
/// `assert_known_keys`).
pub fn assert_known_keys(value: &Value, allowed: &[&str], where_: &str) -> Result<(), String> {
    let Some(map) = value.as_object() else {
        return Ok(());
    };
    let mut unknown: Vec<&str> = map
        .keys()
        .map(String::as_str)
        .filter(|k| !allowed.contains(k))
        .collect();
    if unknown.is_empty() {
        return Ok(());
    }
    unknown.sort_unstable();
    Err(format!(
        "unknown conformance keys at {where_}: {}",
        unknown.join(", ")
    ))
}

const METADATA: &[&str] = &["name", "description", "features", "action"];

/// The keys a case of each action may carry, beyond [`METADATA`].
pub fn case_keys(action: &str) -> Option<&'static [&'static str]> {
    Some(match action {
        "compile" => &["args", "input", "expect", "expect_error", "expect_present"],
        "decompile" => &[
            "args",
            "messages",
            "expect_contains",
            "expect_absent",
            "expect_error",
            "expect_recompiled",
            "expect_round_trip",
        ],
        "unwrap" | "has_format_content" => &["args", "input", "expect"],
        "wrap" => &[
            "args",
            "parts",
            "expect_output",
            "expect_contains",
            "expect_round_trip",
        ],
        "parse_response" => &["args", "input", "expect", "expect_error"],
        "parse_chunk" => &[
            "args",
            "steps",
            "expect_close",
            "expect_error",
            "expect_matches_single_shot",
        ],
        "generate_prompt_snippet" => &[
            "args",
            "expect_file",
            "expect_callable_functions",
            "expect_contains",
            "expect_absent",
            "expect_deterministic",
            "expect_error",
        ],
        "measure_growth" => &["args", "operation", "payload", "sizes", "max_ratio"],
        _ => return None,
    })
}

pub const ARGS_KEYS: &[&str] = &[
    "format",
    "catalog",
    "catalogs",
    "complete",
    "wrapped",
    "examples",
    "allowed_messages",
    "progressive_keys",
    "after_rejected_block",
];
pub const STEP_KEYS: &[&str] = &[
    "input",
    "expect",
    "expect_error",
    "expect_partial_results",
    "expect_in_block",
];
pub const EXPECT_ERROR_KEYS: &[&str] = &["category", "message", "message_is_regex", "details"];

/// Checks a case's shape before it runs.
pub fn check_case_keys(case: &Case) -> Result<(), String> {
    let allowed = case_keys(&case.action)
        .ok_or_else(|| format!("{}: unknown action {:?}", case.name, case.action))?;
    let all: Vec<&str> = METADATA.iter().chain(allowed).copied().collect();
    assert_known_keys(&case.raw, &all, &format!("{} case", case.action))?;
    assert_known_keys(&case.args, ARGS_KEYS, &format!("{} args", case.action))?;
    if let Some(e) = case.get("expect_error") {
        assert_known_keys(e, EXPECT_ERROR_KEYS, "expect_error")?;
    }
    for step in case
        .get("steps")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        assert_known_keys(step, STEP_KEYS, "parse_chunk step")?;
        if let Some(e) = step.get("expect_error") {
            assert_known_keys(e, EXPECT_ERROR_KEYS, "expect_error")?;
        }
    }
    match case.args.get("format").and_then(Value::as_str) {
        Some("express") => Ok(()),
        other => Err(format!("{}: unexpected format {other:?}", case.name)),
    }
}
