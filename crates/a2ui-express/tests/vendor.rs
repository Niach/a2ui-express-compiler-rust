//! Checks that the vendored upstream suite and eui fixtures are present and
//! readable. The conformance runner itself comes later; this pins the inputs.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value as Json;
use serde_yaml_ng::Value as Yaml;

fn vendor() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor")
}

fn read_json(path: &Path) -> Json {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn read_cases(path: &Path) -> Vec<Yaml> {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    match serde_yaml_ng::from_str(&text) {
        Ok(Yaml::Sequence(cases)) => cases,
        Ok(other) => panic!(
            "{}: expected a list of cases, got {other:?}",
            path.display()
        ),
        Err(e) => panic!("{}: {e}", path.display()),
    }
}

/// Every string in a case that looks like a `test_data/...` path.
fn collect_test_data_refs(value: &Yaml, out: &mut Vec<String>) {
    match value {
        Yaml::String(s) if s.starts_with("test_data/") => out.push(s.clone()),
        Yaml::Sequence(items) => items.iter().for_each(|v| collect_test_data_refs(v, out)),
        Yaml::Mapping(map) => map.values().for_each(|v| collect_test_data_refs(v, out)),
        Yaml::Tagged(tagged) => collect_test_data_refs(&tagged.value, out),
        _ => {}
    }
}

#[test]
fn upstream_pin_is_recorded() {
    let upstream = fs::read_to_string(vendor().join("a2ui/UPSTREAM")).unwrap();
    let commit = upstream
        .lines()
        .find_map(|l| l.strip_prefix("commit: "))
        .expect("vendor/a2ui/UPSTREAM has a commit line");
    assert_eq!(commit.len(), 40, "full commit sha: {commit}");
}

#[test]
fn express_suites_load_and_their_test_data_exists() {
    let conformance = vendor().join("a2ui/conformance");
    let suites = [
        "compiler",
        "decompiler",
        "prompt_generator",
        "response_parser",
        "response_streaming",
        "scaling",
    ];
    let mut total = 0;
    for suite in suites {
        let path = conformance.join(format!("agent/express/{suite}.yaml"));
        let cases = read_cases(&path);
        assert!(!cases.is_empty(), "{suite}.yaml has no cases");
        for case in &cases {
            let name = case.get("name").and_then(Yaml::as_str);
            assert!(name.is_some(), "{suite}.yaml: case without a name");
            let mut refs = Vec::new();
            collect_test_data_refs(case, &mut refs);
            for r in refs {
                let file = conformance.join(&r);
                assert!(file.is_file(), "{suite}/{}: missing {r}", name.unwrap());
            }
        }
        total += cases.len();
    }
    assert!(
        total >= 287,
        "expected at least 287 Express cases, found {total}"
    );
}

#[test]
fn test_catalogs_are_json() {
    let dir = vendor().join("a2ui/conformance/test_data/catalogs");
    let mut count = 0;
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        // `catalog_malformed.json` is deliberately not valid JSON.
        if path.file_name().unwrap() == "catalog_malformed.json" {
            continue;
        }
        read_json(&path);
        count += 1;
    }
    assert!(count > 0);
}

#[test]
fn schemas_catalog_and_grammar_are_present() {
    let a2ui = vendor().join("a2ui");
    for schema in [
        "specification/v0_9/json/common_types.json",
        "specification/v0_9/json/server_to_client.json",
        "specification/v1_0/json/common_types.json",
        "specification/v1_0/json/agent_to_renderer.json",
        "catalogs/basic/v1/catalog.json",
    ] {
        read_json(&a2ui.join(schema));
    }
    let grammar =
        fs::read_to_string(a2ui.join("specification/inference_formats/express/Express.g4"))
            .unwrap();
    assert!(grammar.contains("grammar Express"));
}

#[test]
fn eui_catalog_and_fixtures_load() {
    let eui = vendor().join("eui");
    let catalog = read_json(&eui.join("catalog/core.schema.json"));
    assert!(catalog.is_object());
    for fixture in [
        "dashboard",
        "dashboard.data",
        "demo-surface",
        "kitchen-sink",
    ] {
        read_json(&eui.join(format!("fixtures/{fixture}.json")));
    }
    let upstream = fs::read_to_string(eui.join("UPSTREAM")).unwrap();
    assert!(upstream.contains("version: "));
}
