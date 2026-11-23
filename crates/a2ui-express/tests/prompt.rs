//! The prompt generator against upstream's golden rules text and eui's catalog.

use std::path::{Path, PathBuf};

use a2ui_express::prompt::{PromptOptions, PromptStyle, agent_callable_functions, generate_prompt};
use a2ui_express::schema::{Catalog, CatalogSet};
use serde_json::{Value, json};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn load(path: &str) -> Catalog {
    let text = std::fs::read_to_string(root().join(path)).unwrap();
    let mut doc: Value = serde_json::from_str(&text).unwrap();
    if doc.get("catalogId").is_none() {
        doc["catalogId"] = "inline".into();
    }
    Catalog::from_json(doc).unwrap()
}

fn prompt(catalogs: Vec<Catalog>, options: &PromptOptions) -> String {
    generate_prompt(&CatalogSet::new(catalogs).unwrap(), options).unwrap()
}

const BASIC_V1: &str = "../../vendor/a2ui/catalogs/basic/v1/catalog.json";
const SIMPLIFIED: &str =
    "../../vendor/a2ui/conformance/test_data/catalogs/simplified_catalog_v1_0.json";
const CUSTOM: &str = "../../vendor/a2ui/conformance/test_data/catalogs/custom_catalog_v1_0.json";

/// Upstream's `express_base_rules.txt` (conformance/test_data/skills at the
/// pinned commit) is the rules section for the v1.0 basic catalog, byte for
/// byte.
#[test]
fn base_rules_match_upstream_golden() {
    let golden =
        std::fs::read_to_string(root().join("tests/prompt/express_base_rules.txt")).unwrap();
    let text = prompt(vec![load(BASIC_V1)], &PromptOptions::default());
    let rules = &text[..text.find("\n\n## Positional Component Signatures").unwrap()];
    assert_eq!(rules, golden.trim_end_matches('\n'));
}

#[test]
fn root_example_falls_back_to_the_first_component() {
    let mut doc: Value =
        serde_json::from_str(&std::fs::read_to_string(root().join(SIMPLIFIED)).unwrap()).unwrap();
    doc["components"]
        .as_object_mut()
        .unwrap()
        .retain(|k, _| k == "Text");
    let text = prompt(
        vec![Catalog::from_json(doc).unwrap()],
        &PromptOptions::default(),
    );
    assert!(text.contains("    root = Text(...)"));
    assert!(!text.contains("Card("));
}

#[test]
fn deletion_only_drops_surface_targeting_and_renumbers() {
    let options = PromptOptions {
        allowed_messages: Some(vec!["deleteSurface".into()]),
        ..Default::default()
    };
    let text = prompt(vec![load(SIMPLIFIED)], &options);
    assert!(text.contains("\n\n11. To delete a user interface surface"));
    assert!(!text.contains("surface(\"dashboard-surface-1\")"));
    assert!(!text.contains("## System Functions"));
    assert!(!text.contains("Data model population"));
}

#[test]
fn several_catalogs_get_the_multi_catalog_rules_and_headers() {
    let text = prompt(
        vec![load(SIMPLIFIED), load(CUSTOM)],
        &PromptOptions::default(),
    );
    assert!(text.contains("## Multiple Catalogs"));
    assert!(text.contains("catalogId=\"conformance/custom\")"));
    assert!(
        text.contains("# Catalog: conformance/simplified\n\n## Positional Component Signatures")
    );
    assert!(text.contains("# Catalog: conformance/custom\n\n"));
    // Rules 16 and 17 keep their numbers after the 15 base rules.
    assert!(text.contains("\n\n16. A surface has no catalog of its own"));
}

#[test]
fn examples_are_decompiled_and_wrapped_per_turn() {
    let turn = vec![
        json!({"version": "1.0", "createSurface": {"surfaceId": "s1"}}),
        json!({"version": "1.0", "updateComponents": {"surfaceId": "s1", "components": [
            {"id": "root", "component": "Text", "text": "Hi"}
        ]}}),
    ];
    let options = PromptOptions {
        examples: vec![turn.clone(), turn],
        ..Default::default()
    };
    let text = prompt(vec![load(SIMPLIFIED)], &options);
    let block = "<a2ui>\nsurface(\"s1\")\nroot = Text(\"Hi\")\n</a2ui>";
    assert!(text.ends_with(&format!("### Examples:\n\n{block}\n\n{block}")));
}

#[test]
fn an_example_naming_an_unknown_component_is_an_error() {
    let options = PromptOptions {
        examples: vec![vec![json!({"version": "1.0", "updateComponents": {
            "surfaceId": "s1", "components": [{"id": "root", "component": "Nope"}]
        }})]],
        ..Default::default()
    };
    let set = CatalogSet::new(vec![load(SIMPLIFIED)]).unwrap();
    assert!(generate_prompt(&set, &options).is_err());
}

#[test]
fn agent_callable_functions_are_empty_without_callers() {
    let set = CatalogSet::new(vec![load(SIMPLIFIED)]).unwrap();
    assert!(agent_callable_functions(&set).is_empty());
}

/// eui's catalog generates, and every component gets a signature. The size
/// is printed (`--nocapture`) for comparison with eui's `prompt-budget.json`.
#[test]
fn eui_catalog_generates() {
    let catalog = load("../../vendor/eui/catalog/core.schema.json");
    let names: Vec<String> = catalog
        .components()
        .iter()
        .map(|c| c.name().to_owned())
        .collect();
    let text = prompt(vec![catalog], &PromptOptions::default());
    for name in &names {
        assert!(
            text.contains(&format!("\n• {name}(")),
            "no signature for {name}"
        );
    }
    println!(
        "eui prompt: {} chars, ~{} tokens (chars / 4)",
        text.len(),
        text.len() / 4
    );
}

fn compact() -> PromptOptions {
    PromptOptions {
        style: PromptStyle::Compact,
        ..Default::default()
    }
}

/// eui's budget (`prompt-budget.json`): 10,000 tokens at chars / 4, counted
/// in characters as eui counts them. The compact prompt fits; every
/// component still gets a signature; the shared style map and icon enum
/// are listed once and referred to by name.
#[test]
fn eui_compact_prompt_fits_the_budget() {
    let catalog = load("../../vendor/eui/catalog/core.schema.json");
    let names: Vec<String> = catalog
        .components()
        .iter()
        .map(|c| c.name().to_owned())
        .collect();
    let text = prompt(vec![catalog], &compact());
    let chars = text.chars().count();
    println!(
        "eui compact prompt: {chars} chars, ~{} tokens (chars / 4)",
        chars.div_ceil(4)
    );
    assert!(chars.div_ceil(4) <= 10_000, "{chars} chars");
    for name in &names {
        assert!(
            text.contains(&format!("\n• {name}(")),
            "no signature for {name}"
        );
    }
    assert_eq!(text.matches("a-large-small").count(), 1);
    assert_eq!(text.matches("\n- Style: {display: ").count(), 1);
    assert!(text.contains("\n- Icon: one of a-large-small|"));
    assert!(text.contains("Every component also takes style? (Style), visible?~"));
    assert!(text.contains("\n- icon: Icon\n"));
    assert!(text.contains("aspectRatio?:n"));
    assert!(!text.contains("visible? (static)"));
    // The rules section is upstream's.
    let upstream = prompt(
        vec![load("../../vendor/eui/catalog/core.schema.json")],
        &PromptOptions::default(),
    );
    let rules = |t: &str| t[..t.find("\n\n## ").unwrap()].to_owned();
    assert_eq!(rules(&text), rules(&upstream));
}

#[test]
fn compact_names_shared_shapes_and_keeps_the_rest_inline() {
    let catalog = Catalog::from_json(json!({
        "catalogId": "demo",
        "protocolVersion": "1.0",
        "$defs": {
            "Pad": {"type": "object", "properties": {
                "top": {"type": "number"}, "bottom": {"type": "number"},
                "left": {"type": "number"}, "right": {"type": "number"},
                "unit": {"enum": ["px", "rem"]}
            }}
        },
        "components": {
            "Badge": {"description": "Small label. Second sentence.", "properties": {
                "component": {"const": "Badge"},
                "text": {"$ref": "common_types.json#/$defs/DynamicString", "description": "Shown."},
                "tone": {"enum": ["neutral", "success", "warning", "danger", "info"]},
                "pad": {"$ref": "#/$defs/Pad"},
                "kind": {"enum": ["a", "b"]}
            }, "required": ["component", "text"]},
            "Tag": {"properties": {
                "component": {"const": "Tag"},
                "tone": {"enum": ["neutral", "success", "warning", "danger", "info"]},
                "pad": {"$ref": "#/$defs/Pad"},
                "count": {"type": "number"}
            }, "required": ["component"]}
        }
    }))
    .unwrap();
    let text = prompt(vec![catalog], &compact());
    // Shared and long: named, listed once.
    assert!(
        text.contains("\n- Pad: {top, bottom, left, right, unit: px|rem}\n"),
        "{text}"
    );
    assert!(text.contains("\n• Badge(text~, tone?, pad?, kind?) — Small label.\n  - tone: neutral|success|warning|danger|info\n  - pad: Pad\n  - kind: a|b\n"), "{text}");
    // Short enums stay inline; numbers are marked.
    assert!(text.contains("\n• Tag(tone?, pad?, count?:n)\n"), "{text}");
    assert!(!text.contains("Second sentence"));
}
