//! The catalog loader and schema helper, against eui's catalog, the basic
//! catalog, the conformance catalogs and ports of upstream's unit tests.

use std::fs;
use std::path::{Path, PathBuf};

use a2ui_express::Error;
use a2ui_express::schema::{
    AllowedCallers, Catalog, CatalogError, CatalogSet, CatalogVersion, LoadOptions, Segment,
    SlotKind, SymbolKind,
};
use serde_json::{Value, json};

fn vendor() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor")
}

fn read(path: &str) -> Value {
    let path = vendor().join(path);
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn load(path: &str) -> Catalog {
    Catalog::from_json(read(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn eui() -> Catalog {
    load("eui/catalog/core.schema.json")
}

fn basic() -> Catalog {
    load("a2ui/catalogs/basic/v1/catalog.json")
}

fn conformance(name: &str) -> Catalog {
    load(&format!("a2ui/conformance/test_data/catalogs/{name}"))
}

fn item(key: &str) -> [Segment<'_>; 2] {
    [Segment::Item, Segment::Property(key)]
}

fn strings(values: &[String]) -> Vec<&str> {
    values.iter().map(String::as_str).collect()
}

// ---- eui ------------------------------------------------------------------

#[test]
fn eui_loads_all_104_components() {
    let catalog = eui();
    assert_eq!(catalog.version(), CatalogVersion::V1_0);
    assert_eq!(catalog.id(), "https://ui.exponential.at/catalogs/core/v1");
    assert_eq!(catalog.components().len(), 104);
    let declared: Vec<String> = catalog.document()["components"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    let loaded: Vec<String> = catalog
        .components()
        .iter()
        .map(|c| c.name().to_owned())
        .collect();
    assert_eq!(loaded, declared);
    assert_eq!(
        catalog.functions().len(),
        catalog.document()["functions"].as_object().unwrap().len()
    );
}

#[test]
fn eui_input_value_is_bindable() {
    let catalog = eui();
    assert_eq!(catalog.is_static("Input", "value"), Some(false));
    assert_eq!(catalog.admits_path_at("Input", "value", &[]), Some(true));
    assert_eq!(catalog.is_static("Input", "label"), Some(true));
    assert!(catalog.is_required("Input", "label"));
    assert!(!catalog.is_required("Input", "value"));
}

#[test]
fn eui_menu_item_checked_is_bindable_and_static_item_keys_are_not() {
    let catalog = eui();
    assert_eq!(
        catalog.admits_path_at("Menu", "items", &item("checked")),
        Some(true)
    );
    for key in [
        "label",
        "value",
        "kind",
        "shortcut",
        "destructive",
        "disabled",
    ] {
        assert_eq!(
            catalog.admits_path_at("Menu", "items", &item(key)),
            Some(false),
            "Menu.items[].{key}"
        );
    }
    // The submenu's entries are the same menu item, one level deeper.
    let nested = [
        Segment::Item,
        Segment::Property("items"),
        Segment::Item,
        Segment::Property("checked"),
    ];
    assert_eq!(catalog.admits_path_at("Menu", "items", &nested), Some(true));
    // A key the item schema doesn't declare: unknown, so allowed.
    assert_eq!(catalog.admits_path_at("Menu", "items", &item("nope")), None);
    assert!(catalog.is_bindable_at("Menu", "items", &item("nope")));
    // The array itself is a literal list.
    assert_eq!(catalog.is_static("Menu", "items"), Some(true));
}

#[test]
fn eui_positional_order_follows_the_schema() {
    let catalog = eui();
    assert_eq!(
        strings(catalog.component_properties("Menu")),
        [
            "label", "icon", "items", "side", "align", "anchor", "openOn", "children", "on",
            "style", "visible"
        ]
    );
    // Input inherits `@checks` from the common Checkable: appended last.
    let input = catalog.component_properties("Input");
    assert_eq!(input.first().map(String::as_str), Some("label"));
    assert_eq!(input.last().map(String::as_str), Some("checks"));
    assert_eq!(catalog.check_rule_property("Input"), Some("checks"));
    assert_eq!(
        catalog.check_rule_wire_key("Input").as_deref(),
        Some("@checks")
    );
    assert!(!catalog.is_checkable("Menu"));
    assert_eq!(
        catalog.property_enum("Menu", "side").unwrap(),
        &[json!("top"), json!("right"), json!("bottom"), json!("left")]
    );
}

#[test]
fn eui_child_and_child_list_slots() {
    let catalog = eui();
    assert_eq!(
        catalog.property_slot_kind("Menu", "anchor"),
        Some(SlotKind::Child)
    );
    // eui's catalog-local `#/$defs/ChildList` reads as the common ChildList.
    assert_eq!(
        catalog.property_slot_kind("Menu", "children"),
        Some(SlotKind::ChildList)
    );
    assert!(catalog.is_component_reference(catalog.property_schema("Menu", "children").unwrap()));
    assert_eq!(catalog.property_slot_kind("Menu", "label"), None);
}

#[test]
fn eui_every_property_has_a_schema_and_classifies() {
    let catalog = eui();
    for component in catalog.components() {
        for prop in component.properties() {
            let schema = if Some(prop.as_str()) == component.check_rule_property() {
                component.check_rule_property_schema()
            } else {
                component.property_schema(prop)
            };
            let schema = schema.unwrap_or_else(|| panic!("{}.{prop}", component.name()));
            // Every query terminates on eui's recursive `$defs`.
            catalog.admits_path(schema);
            catalog.accepts_number(schema);
            catalog.is_action_slot(schema);
            catalog.slot_kind(schema);
            catalog.expects_option_objects(schema);
        }
    }
}

// ---- basic catalog ----------------------------------------------------------

#[test]
fn basic_choice_picker_option_label_is_bindable() {
    let catalog = basic();
    assert_eq!(
        catalog.admits_path_at("ChoicePicker", "options", &item("label")),
        Some(true)
    );
    assert_eq!(
        catalog.admits_path_at("ChoicePicker", "options", &item("value")),
        Some(false)
    );
    assert!(catalog.expects_option_objects_property("ChoicePicker", "options"));
    assert!(!catalog.expects_option_objects_property("ChoicePicker", "value"));
}

#[test]
fn basic_positional_properties() {
    let catalog = basic();
    assert_eq!(
        strings(catalog.component_properties("Text")),
        ["text", "variant", "weight"]
    );
    assert_eq!(
        strings(catalog.component_properties("Button")),
        ["child", "variant", "action", "weight", "checks"]
    );
    assert_eq!(catalog.check_rule_property("Button"), Some("checks"));
    assert!(!catalog.is_checkable("Text"));
    assert!(
        !catalog
            .component_properties("Image")
            .iter()
            .any(|p| p == "accessibility")
    );
}

#[test]
fn basic_only_button_action_is_an_action_slot() {
    let catalog = basic();
    for component in catalog.components() {
        for prop in component.properties() {
            let expected = component.name() == "Button" && prop == "action";
            assert_eq!(
                catalog.is_action_property(component.name(), prop),
                expected,
                "{}.{prop}",
                component.name()
            );
        }
    }
}

#[test]
fn basic_resolves_tab_items() {
    let catalog = basic();
    let tabs = catalog.property_schema("Tabs", "tabs").unwrap();
    let items = catalog.resolve_item_schema(tabs).unwrap();
    let title = catalog.resolve_property_schema(items, "title").unwrap();
    assert!(catalog.admits_path(title));
    let child = catalog.resolve_property_schema(items, "child").unwrap();
    assert!(!catalog.admits_path(child));
    assert!(
        catalog
            .resolve_property_schema(&json!({}), "nope")
            .is_none()
    );
    assert!(catalog.resolve_item_schema(&Value::Null).is_none());
}

#[test]
fn basic_descriptions_and_functions() {
    let catalog = basic();
    assert!(catalog.component("Column").unwrap().description().is_some());
    assert!(
        catalog
            .function("required")
            .unwrap()
            .description()
            .is_some()
    );
    let regex = catalog.function("regex").unwrap();
    assert_eq!(regex.argument_schema("pattern").unwrap()["type"], "string");
    assert!(regex.is_required("pattern"));
    assert!(regex.return_type().is_some());
    // The protocol's system function.
    let index = catalog.function("@index").unwrap();
    assert!(index.is_system());
    assert_eq!(strings(index.arguments()), ["offset"]);
    assert_eq!(index.return_type(), Some("number"));
    assert!(!catalog.has_function("@index"));
}

// ---- conformance catalogs ---------------------------------------------------

#[test]
fn every_conformance_catalog_loads_or_fails_as_a_catalog_error() {
    let dir = vendor().join("a2ui/conformance/test_data/catalogs");
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let result = Catalog::from_json_str(&fs::read_to_string(&path).unwrap());
        match name.as_str() {
            "catalog_malformed.json" => {
                assert!(
                    matches!(result, Err(CatalogError::InvalidJson(_))),
                    "{name}"
                )
            }
            // No catalogId: the `$id` names it, as upstream reads it.
            "catalog_without_id.json" => {
                let c = result.unwrap();
                assert!(c.id().ends_with("catalog_without_id.json"));
                assert_eq!(c.version(), CatalogVersion::DEFAULT);
            }
            // An extension key that is not a UAX #31 identifier.
            "component_metadata_invalid_key_catalog_v1_0.json" => {
                let error = result.unwrap_err();
                assert!(
                    matches!(error, CatalogError::InvalidMetadata { .. }),
                    "{error}"
                );
                assert_eq!(
                    error.to_string(),
                    "Invalid UAX #31 extension identifier 'com.example.policy' in component 'Card' metadata.extensions."
                );
            }
            n if n.contains("v08") || n.contains("v0_8") => {
                assert!(
                    matches!(result, Err(CatalogError::UnsupportedVersion(_)) | Ok(_)),
                    "{name}"
                )
            }
            _ => {
                result.unwrap_or_else(|e| panic!("{name}: {e}"));
            }
        }
    }
}

#[test]
fn forms_catalog_own_check_rule_property() {
    let catalog = conformance("forms_catalog_v1_0.json");
    assert_eq!(
        strings(catalog.component_properties("TextField")),
        ["label", "value", "placeholder", "checks"]
    );
    assert_eq!(catalog.check_rule_property("TextField"), Some("checks"));
}

#[test]
fn allowed_callers_and_return_type() {
    let catalog = conformance("allowed_callers_catalog_v1_0.json");
    let f = catalog.function("readSessionToken").unwrap();
    assert_eq!(f.return_type(), Some("string"));
    assert_eq!(f.allowed_callers(), AllowedCallers::AgentOnly);
    assert!(f.is_agent_callable());
}

#[test]
fn agent_callable_functions_across_catalogs() {
    let set = CatalogSet::new(vec![
        conformance("simplified_catalog_v1_0.json"),
        conformance("agent_functions_catalog_v1_0.json"),
    ])
    .unwrap();
    assert_eq!(set.agent_callable_functions(), ["notify", "record", "peek"]);
}

// ---- several catalogs -------------------------------------------------------

fn tiny(id: &str, version: &str, component: &str) -> Catalog {
    Catalog::from_json(json!({
        "catalogId": id,
        "protocolVersion": version,
        "components": {
            component: {"properties": {"component": {"const": component}}},
            "Shared": {"properties": {"component": {"const": "Shared"}}}
        }
    }))
    .unwrap()
}

#[test]
fn catalog_set_checks_versions_and_ids() {
    assert_eq!(
        CatalogSet::new(vec![]).unwrap_err(),
        CatalogError::NoCatalogs
    );
    assert_eq!(
        CatalogSet::new(vec![tiny("a", "1.0", "A"), tiny("a", "1.0", "B")]).unwrap_err(),
        CatalogError::DuplicateId("a".into())
    );
    assert!(matches!(
        CatalogSet::new(vec![tiny("a", "1.0", "A"), tiny("b", "0.9", "B")]),
        Err(CatalogError::MixedVersions { .. })
    ));
    assert!(matches!(
        CatalogSet::new(vec![tiny("a", "0.9", "A"), tiny("b", "0.9.1", "B")]),
        Err(CatalogError::SeveralBelowV1(_))
    ));
    let one = CatalogSet::new(vec![tiny("a", "0.9", "A")]).unwrap();
    assert_eq!(one.surface_catalog_id(), Some("a"));
    assert_eq!(
        one.resolve_version(Some(CatalogVersion::V0_9_1)).unwrap(),
        CatalogVersion::V0_9_1
    );
    assert!(one.resolve_version(Some(CatalogVersion::V1_0)).is_err());
}

#[test]
fn catalog_set_resolves_names_with_catalog_id_override() {
    let set = CatalogSet::new(vec![tiny("a", "1.0", "A"), tiny("b", "1.0", "B")]).unwrap();
    assert_eq!(set.surface_catalog_id(), None);

    let r = set.resolve(SymbolKind::Component, "B", None).unwrap();
    assert_eq!((r.catalog.id(), r.catalog_id), ("b", Some("b")));

    let err = set
        .resolve(SymbolKind::Component, "Shared", None)
        .unwrap_err();
    assert_eq!(
        err,
        Error::Validation(
            "Component 'Shared' is defined in several catalogs: 'a', 'b'. Name the catalog to use with a catalogId argument.".into()
        )
    );
    let r = set
        .resolve(SymbolKind::Component, "Shared", Some(&json!("b")))
        .unwrap();
    assert_eq!(r.catalog.id(), "b");

    let err = set
        .resolve(SymbolKind::Component, "A", Some(&json!("b")))
        .unwrap_err();
    assert_eq!(
        err,
        Error::Validation("Component 'A' is not in catalog 'b'. It is defined in: 'a'.".into())
    );
    let err = set
        .resolve(SymbolKind::Component, "A", Some(&json!("zzz")))
        .unwrap_err();
    assert_eq!(
        err,
        Error::Validation(
            "Component 'A' targets unknown catalog 'zzz'. Active catalogs: 'a', 'b'.".into()
        )
    );
    assert_eq!(
        set.resolve(SymbolKind::Function, "nope", None).unwrap_err(),
        Error::Validation("Function 'nope' is not defined in the catalog.".into())
    );
    assert!(
        set.resolve(SymbolKind::Component, "A", Some(&json!("")))
            .is_err()
    );

    let v09 = CatalogSet::new(vec![tiny("a", "0.9", "A")]).unwrap();
    assert!(
        v09.resolve(SymbolKind::Component, "A", Some(&json!("a")))
            .is_err()
    );
    let r = v09.resolve(SymbolKind::Component, "A", None).unwrap();
    assert_eq!(r.catalog_id, None);
}

// ---- ported upstream unit tests --------------------------------------------

#[test]
fn catalog_local_defs_are_not_common_types() {
    let doc = json!({
        "catalogId": "test_negative",
        "protocolVersion": "1.0",
        "components": {
            "CustomComp": {
                "type": "object",
                "properties": {
                    "component": {"const": "CustomComp"},
                    "actionSlot": {"$ref": "#/$defs/MyAction"},
                    "dynamicFoo": {"$ref": "#/$defs/DynamicFoo"},
                    "withAtPath": {"$ref": "#/$defs/WithAtPath"},
                    "withBarePath": {"$ref": "#/$defs/WithBarePath"},
                    "withOptionalAtPath": {"$ref": "#/$defs/WithOptionalAtPath"},
                    "withLabelAndPath": {"$ref": "#/$defs/WithLabelAndPath"}
                }
            }
        },
        "functions": {},
        "$defs": {
            "MyAction": {"type": "object", "properties": {"event": {"type": "string"}}},
            "DynamicFoo": {"type": "object", "properties": {"value": {"type": "string"}}},
            "WithAtPath": {"type": "object", "properties": {"@path": {"type": "string"}}, "required": ["@path"]},
            "WithBarePath": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]},
            "WithOptionalAtPath": {"type": "object", "properties": {"@path": {"type": "string"}}},
            "WithLabelAndPath": {
                "type": "object",
                "properties": {"label": {"type": "string"}, "path": {"type": "string"}},
                "required": ["label", "path"]
            }
        }
    });
    let c = Catalog::from_json(doc.clone()).unwrap();
    assert!(!c.is_action_property("CustomComp", "actionSlot"));
    assert_eq!(c.is_static("CustomComp", "actionSlot"), Some(true));
    assert!(!c.is_action_property("CustomComp", "dynamicFoo"));
    assert_eq!(c.is_static("CustomComp", "dynamicFoo"), Some(true));
    assert_eq!(c.is_static("CustomComp", "withAtPath"), Some(false));
    assert_eq!(c.is_static("CustomComp", "withBarePath"), Some(true));
    assert_eq!(c.is_static("CustomComp", "withOptionalAtPath"), Some(true));
    assert_eq!(c.is_static("CustomComp", "withLabelAndPath"), Some(true));

    let mut v09 = doc;
    v09["protocolVersion"] = json!("v0.9");
    let c = Catalog::from_json(v09).unwrap();
    assert_eq!(c.is_static("CustomComp", "withBarePath"), Some(false));
    assert_eq!(c.is_static("CustomComp", "withAtPath"), Some(true));
}

#[test]
fn dereference_keeps_common_refs_and_survives_cycles() {
    let c = Catalog::from_json(json!({
        "catalogId": "test_local_defs",
        "protocolVersion": "1.0",
        "components": {
            "ActionAndCycle": {
                "type": "object",
                "properties": {
                    "component": {"const": "ActionAndCycle"},
                    "onTap": {"oneOf": [{"$ref": "#/$defs/TapAction"}, {"type": "null"}]},
                    "targetId": {"$ref": "#/$defs/ComponentId"},
                    "node": {"$ref": "#/$defs/CycleNode"},
                    "flatAction": {"oneOf": [
                        {"type": "object", "properties": {"event": {"type": "object", "properties": {"name": {"type": "string"}}, "required": ["name"]}}, "required": ["event"]},
                        {"type": "object", "properties": {"functionCall": {"type": "object", "properties": {"@call": {"type": "string"}}, "required": ["@call"]}}, "required": ["functionCall"]}
                    ]}
                }
            }
        },
        "$defs": {
            "ComponentId": {"type": "string"},
            "TapAction": {"$ref": "common_types.json#/$defs/Action"},
            "CycleNode": {"oneOf": [{"$ref": "#/$defs/CycleNode"}, {"type": "string"}]}
        }
    }))
    .unwrap();
    assert_eq!(
        c.property_schema("ActionAndCycle", "targetId").unwrap()["$ref"],
        "#/$defs/ComponentId"
    );
    assert!(c.is_action_property("ActionAndCycle", "onTap"));
    assert!(c.is_action_property("ActionAndCycle", "flatAction"));
    assert_eq!(c.property_enum("ActionAndCycle", "node"), None);
    let node = c.property_schema("ActionAndCycle", "node").unwrap();
    assert!(!c.admits_path(node));
    assert!(!c.is_action_slot(node));
    assert_eq!(c.slot_kind(node), None);
    assert_eq!(c.admits_path_at("ActionAndCycle", "node", &item("x")), None);
}

#[test]
fn own_definition_wins_and_keeps_first_position() {
    let c = Catalog::from_json(json!({
        "catalogId": "test_own_property_override",
        "protocolVersion": "1.0",
        "components": {
            "Badge": {
                "type": "object",
                "allOf": [
                    {"$ref": "#/$defs/BadgeMixin"},
                    {"type": "object", "properties": {
                        "size": {"type": "string", "enum": ["inline-s", "inline-m"]},
                        "tone": {"type": "string", "enum": ["inline-calm", "inline-loud"]}
                    }}
                ],
                "properties": {
                    "component": {"const": "Badge"},
                    "label": {"type": "string"},
                    "tone": {"type": "string", "enum": ["own-calm", "own-loud"]},
                    "size": {"type": "string", "enum": ["own-s", "own-m"]}
                }
            }
        },
        "functions": {},
        "$defs": {
            "BadgeMixin": {"type": "object", "properties": {
                "size": {"type": "string", "enum": ["mixin-s", "mixin-m"]},
                "label": {"type": "number"},
                "extra": {"type": "string"}
            }}
        }
    }))
    .unwrap();
    assert_eq!(
        c.property_enum("Badge", "tone").unwrap(),
        &[json!("own-calm"), json!("own-loud")]
    );
    assert_eq!(
        c.property_enum("Badge", "size").unwrap(),
        &[json!("own-s"), json!("own-m")]
    );
    assert_eq!(
        c.property_schema("Badge", "label").unwrap()["type"],
        "string"
    );
    assert_eq!(
        strings(c.component_properties("Badge")),
        ["label", "tone", "size", "extra"]
    );
}

#[test]
fn nested_local_mixin_v0_9() {
    let common = "https://a2ui.org/specification/v0_9/common_types.json#/$defs";
    let c = Catalog::from_json(json!({
        "catalogId": "test_nested_mixin",
        "components": {
            "MaterialText": {"type": "object", "allOf": [
                {"$ref": "#/$defs/MaterialComponentCommon"},
                {"type": "object", "properties": {
                    "component": {"const": "MaterialText"},
                    "text": {"type": "string"},
                    "usageHint": {"type": "string", "enum": ["h1", "body"]}
                }, "required": ["component", "text"]}
            ]},
            "MaterialHeading": {"type": "object", "allOf": [
                {"$ref": "#/$defs/MaterialComponentCommon"},
                {"type": "object", "properties": {
                    "component": {"const": "MaterialHeading"},
                    "text": {"type": "string"},
                    "weight": {"type": "string", "enum": ["own-light", "own-bold"]}
                }}
            ]},
            "MaterialButton": {"type": "object", "allOf": [
                {"$ref": format!("{common}/Checkable")},
                {"$ref": "#/$defs/MaterialComponentCommon"},
                {"type": "object", "properties": {
                    "component": {"const": "MaterialButton"},
                    "child": {"type": "string"},
                    "action": {"$ref": format!("{common}/Action")}
                }}
            ]}
        },
        "functions": {},
        "$defs": {
            "MaterialComponentCommon": {"allOf": [
                {"$ref": format!("{common}/ComponentCommon")},
                {"type": "object", "properties": {"weight": {"type": "number"}, "style": {"type": "object"}}, "required": ["style"]}
            ]}
        }
    }))
    .unwrap();
    assert_eq!(c.version(), CatalogVersion::V0_9);
    assert_eq!(
        strings(c.component_properties("MaterialText")),
        ["text", "usageHint", "weight", "style"]
    );
    assert_eq!(
        strings(c.component_required("MaterialText")),
        ["component", "text", "style"]
    );
    assert_eq!(
        c.property_schema("MaterialText", "weight").unwrap(),
        &json!({"type": "number"})
    );
    assert_eq!(
        strings(c.component_properties("MaterialHeading")),
        ["text", "weight", "style"]
    );
    assert_eq!(
        c.property_enum("MaterialHeading", "weight").unwrap(),
        &[json!("own-light"), json!("own-bold")]
    );
    assert_eq!(
        strings(c.component_properties("MaterialButton")),
        ["child", "action", "weight", "style", "checks"]
    );
    assert_eq!(
        c.check_rule_wire_key("MaterialButton").as_deref(),
        Some("checks")
    );
    assert!(c.is_action_property("MaterialButton", "action"));
}

#[test]
fn envelope_properties_and_custom_names() {
    let c = Catalog::from_json(json!({
        "catalogId": "test_envelope",
        "protocolVersion": "1.0",
        "components": {
            "AtInline": {"type": "object", "properties": {
                "component": {"const": "AtInline"},
                "label": {"type": "string"},
                "@catalogId": {"type": "string"},
                "@accessibility": {"$ref": "common_types.json#/$defs/AccessibilityAttributes"},
                "@metadata": {"type": "object"},
                "@checks": {"type": "array", "items": {"$ref": "common_types.json#/$defs/CheckRule"}}
            }, "required": ["component"]},
            "BareChecks": {"type": "object", "properties": {
                "component": {"const": "BareChecks"},
                "checks": {"type": "array", "items": {"$ref": "common_types.json#/$defs/CheckRule"}},
                "label": {"type": "string"}
            }, "required": ["component"]},
            "Custom": {"type": "object", "properties": {
                "component": {"const": "Custom"},
                "metadata": {"type": "string"},
                "accessibility": {"type": "boolean"},
                "checks": {"type": "integer"},
                "catalogId": {"type": "string"}
            }, "required": ["component"]},
            "ScoreInput": {"type": "object", "allOf": [
                {"$ref": "common_types.json#/$defs/Checkable"},
                {"type": "object", "properties": {
                    "component": {"const": "ScoreInput"},
                    "checks": {"type": "integer"},
                    "label": {"type": "string"}
                }, "required": ["component", "checks"]}
            ]}
        }
    }))
    .unwrap();
    assert_eq!(
        strings(c.component_properties("AtInline")),
        ["label", "checks"]
    );
    assert!(c.is_checkable("AtInline"));
    assert!(
        c.component("AtInline")
            .unwrap()
            .custom_envelope_properties()
            .is_empty()
    );
    assert!(
        c.component("AtInline")
            .unwrap()
            .check_rule_property_schema()
            .is_some()
    );

    assert_eq!(
        strings(c.component_properties("BareChecks")),
        ["label", "checks"]
    );
    assert!(c.is_checkable("BareChecks"));

    assert_eq!(
        strings(c.component_properties("Custom")),
        ["metadata", "accessibility", "checks", "catalogId"]
    );
    assert!(!c.is_checkable("Custom"));
    assert_eq!(
        strings(c.component("Custom").unwrap().custom_envelope_properties()),
        ["catalogId", "metadata", "accessibility", "checks"]
    );

    assert!(c.is_checkable("ScoreInput"));
    assert_eq!(
        strings(
            c.component("ScoreInput")
                .unwrap()
                .custom_envelope_properties()
        ),
        ["checks"]
    );
    assert_eq!(
        strings(c.component_properties("ScoreInput")),
        ["checks", "label"]
    );
}

#[test]
fn option_objects() {
    let c = basic();
    assert!(c.expects_option_objects(&json!({
        "type": "array",
        "items": {"type": "object", "properties": {"label": {"type": "string"}, "value": {"type": "string"}}}
    })));
    assert!(!c.expects_option_objects(&json!({
        "type": "array",
        "items": {"type": "object", "properties": {"label": {"type": "string"}}}
    })));
    assert!(!c.expects_option_objects(&Value::Null));
    assert!(!c.expects_option_objects(&json!("string")));
}

// ---- loading ----------------------------------------------------------------

#[test]
fn load_options_override_id_and_version() {
    let doc = json!({"catalogId": "x", "protocolVersion": "1.0", "components": {}});
    let c = Catalog::load(
        doc.clone(),
        LoadOptions {
            catalog_id: Some("y".into()),
            protocol_version: Some(CatalogVersion::V0_9_1),
            ..LoadOptions::default()
        },
    )
    .unwrap();
    assert_eq!(
        (c.id(), c.version(), c.path_key()),
        ("y", CatalogVersion::V0_9_1, "path")
    );
    assert_eq!(
        c.common_types()["$id"],
        "https://a2ui.org/specification/v0_9/common_types.json"
    );
    let c = Catalog::from_json(doc).unwrap();
    assert_eq!((c.path_key(), c.call_key()), ("@path", "@call"));
}

#[test]
fn v1_rejects_external_refs_and_bad_activation_callers() {
    let err = Catalog::from_json(json!({
        "catalogId": "x", "protocolVersion": "1.0",
        "components": {"A": {"properties": {"b": {"$ref": "other.json#/$defs/B"}}}}
    }))
    .unwrap_err();
    assert_eq!(
        err,
        CatalogError::ExternalRef {
            reference: "other.json#/$defs/B".into(),
            pointer: "/components/A/properties/b/$ref".into()
        }
    );
    let err = Catalog::from_json(json!({
        "catalogId": "x", "protocolVersion": "1.0",
        "functions": {"f": {"requiresUserActivation": true, "allowedCallers": "agentOnly"}}
    }))
    .unwrap_err();
    assert!(matches!(err, CatalogError::UserActivationCallers { .. }));
    assert!(matches!(
        Catalog::from_json(json!({"catalogId": "x", "protocolVersion": "0.8"})),
        Err(CatalogError::UnsupportedVersion(_))
    ));
}

#[test]
fn a_catalog_needs_an_id() {
    assert_eq!(
        Catalog::from_json(json!({"components": {}})).unwrap_err(),
        CatalogError::MissingId
    );
}

#[test]
fn any_component_limits_the_components() {
    let c = Catalog::from_json(json!({
        "catalogId": "x", "protocolVersion": "1.0",
        "components": {"A": {"properties": {}}, "B": {"properties": {}}},
        "$defs": {"anyComponent": {"oneOf": [{"$ref": "#/components/B"}]}}
    }))
    .unwrap();
    assert!(!c.has_component("A"));
    assert!(c.has_component("B"));
}

/// Every `@path` inside a fixture prop lands where the catalog allows one.
fn check_fixture_bindings(catalog: &Catalog, node: &Value, found: &mut usize) {
    match node {
        Value::Array(items) => items
            .iter()
            .for_each(|n| check_fixture_bindings(catalog, n, found)),
        Value::Object(map) => {
            if let (Some(Value::String(component)), Some(Value::Object(props))) =
                (map.get("component"), map.get("props"))
            {
                for (prop, value) in props {
                    let mut path = Vec::new();
                    walk_value(catalog, component, prop, value, &mut path, found);
                }
            }
            map.values()
                .for_each(|v| check_fixture_bindings(catalog, v, found));
        }
        _ => {}
    }
}

fn walk_value(
    catalog: &Catalog,
    component: &str,
    prop: &str,
    value: &Value,
    path: &mut Vec<String>,
    found: &mut usize,
) {
    match value {
        Value::Object(map) if map.contains_key("@path") && map.len() == 1 => {
            let owned = path.clone();
            let segs: Vec<Segment<'_>> = owned
                .iter()
                .map(|s| {
                    if s == "[]" {
                        Segment::Item
                    } else {
                        Segment::Property(s)
                    }
                })
                .collect();
            *found += 1;
            assert_ne!(
                catalog.admits_path_at(component, prop, &segs),
                Some(false),
                "{component}.{prop} at {owned:?}"
            );
        }
        Value::Object(map) if !map.contains_key("@call") => {
            for (k, v) in map {
                path.push(k.clone());
                walk_value(catalog, component, prop, v, path, found);
                path.pop();
            }
        }
        Value::Array(items) => {
            for v in items {
                path.push("[]".into());
                walk_value(catalog, component, prop, v, path, found);
                path.pop();
            }
        }
        _ => {}
    }
}

#[test]
fn eui_fixture_bindings_are_all_allowed() {
    let catalog = eui();
    let mut found = 0;
    for fixture in ["kitchen-sink", "dashboard", "demo-surface"] {
        let doc = read(&format!("eui/fixtures/{fixture}.json"));
        check_fixture_bindings(&catalog, &doc, &mut found);
    }
    assert!(found > 0, "the fixtures bind something");
}

fn v1_catalog(components: Value, functions: Value) -> Result<Catalog, CatalogError> {
    Catalog::from_json(json!({
        "catalogId": "ids",
        "protocolVersion": "1.0",
        "components": components,
        "functions": functions,
    }))
}

/// Upstream's loader: from v1.0, component, property, function and argument
/// names are UAX #31 identifiers (#AXC-14).
#[test]
fn v1_names_are_uax31_identifiers() {
    let text = json!({"properties": {"component": {"const": "Text"}, "text": {"type": "string"}}});
    let message = |r: Result<Catalog, CatalogError>| {
        let error = r.unwrap_err();
        assert!(
            matches!(error, CatalogError::InvalidIdentifier { .. }),
            "{error}"
        );
        error.to_string()
    };

    assert_eq!(
        message(v1_catalog(json!({"My-Text": text}), json!({}))),
        "Invalid UAX #31 component identifier: 'My-Text'"
    );
    assert_eq!(
        message(v1_catalog(
            json!({"Text": {"properties": {"aria-label": {"type": "string"}}}}),
            json!({})
        )),
        "Invalid UAX #31 property identifier: 'aria-label' in component 'Text'"
    );
    let function = |name: &str, arg: &str| {
        json!({name: {"properties": {
            "@call": {"const": name},
            "args": {"type": "object", "properties": {arg: {"type": "string"}}}
        }}})
    };
    assert_eq!(
        message(v1_catalog(json!({}), function("open.url", "url"))),
        "Invalid UAX #31 function identifier: 'open.url'"
    );
    assert_eq!(
        message(v1_catalog(json!({}), function("openUrl", "the url"))),
        "Invalid UAX #31 argument identifier: 'the url' in function 'openUrl'"
    );

    // Unicode identifiers and the `@` spelling of the standard mixins pass.
    let ok = v1_catalog(
        json!({"Überschrift": {"properties": {
            "component": {"const": "Überschrift"},
            "größe": {"type": "number"},
            "@checks": {"type": "array"},
            "@catalogId": {"type": "string"}
        }}}),
        function("öffne", "ziel_1"),
    )
    .unwrap();
    assert!(ok.has_component("Überschrift") && ok.has_function("öffne"));
    // Any other `@` name is not a mixin.
    assert_eq!(
        message(v1_catalog(
            json!({"Text": {"properties": {"@style": {"type": "string"}}}}),
            json!({})
        )),
        "Invalid UAX #31 property identifier: '@style' in component 'Text'"
    );

    // Below v1.0 the names are not checked, as upstream does not.
    let v0_9 = Catalog::from_json(json!({
        "catalogId": "ids",
        "protocolVersion": "0.9",
        "components": {"My-Text": {"properties": {"aria-label": {"type": "string"}}}},
    }))
    .unwrap();
    assert!(v0_9.has_component("My-Text"));
}

/// Upstream's `validate_metadata`: a v1.0 `metadata` (of the catalog, a
/// component or a function) is an object whose only key is `extensions`,
/// keyed by identifiers (#AXC-14).
#[test]
fn v1_metadata_is_validated() {
    let with = |metadata: Value| {
        v1_catalog(
            json!({"Card": {"metadata": metadata, "properties": {"component": {"const": "Card"}}}}),
            json!({}),
        )
    };
    let message = |metadata: Value| {
        let error = with(metadata).unwrap_err();
        assert!(
            matches!(&error, CatalogError::InvalidMetadata { owner, .. } if owner == "component 'Card'"),
            "{error}"
        );
        error.to_string()
    };
    assert!(with(json!({"extensions": {"a2ui_policy": {"x": 1}}})).is_ok());
    assert!(with(json!({})).is_ok());
    assert!(with(Value::Null).is_ok());
    assert_eq!(
        message(json!([])),
        "Expected component 'Card' metadata to be an object, got array."
    );
    assert_eq!(
        message(json!({"vendor": {}})),
        "Unexpected property 'vendor' in component 'Card' metadata; only 'extensions' is allowed."
    );
    assert_eq!(
        message(json!({"extensions": "x"})),
        "Expected component 'Card' metadata.extensions to be an object, got string."
    );
    assert_eq!(
        message(json!({"extensions": {"com.example": 1}})),
        "Invalid UAX #31 extension identifier 'com.example' in component 'Card' metadata.extensions."
    );
    assert!(conformance("component_metadata_catalog_v1_0.json").has_component("Card"));
    assert!(conformance("function_metadata_catalog_v1_0.json").has_function("launch"));

    let function = v1_catalog(
        json!({}),
        json!({"open": {"metadata": {"extensions": {"a-b": 1}}, "properties": {"@call": {"const": "open"}}}}),
    );
    assert_eq!(
        function.unwrap_err().to_string(),
        "Invalid UAX #31 extension identifier 'a-b' in function 'open' metadata.extensions."
    );
    let catalog = Catalog::from_json(json!({
        "catalogId": "ids", "protocolVersion": "1.0", "metadata": {"owner": "x"},
    }));
    assert_eq!(
        catalog.unwrap_err().to_string(),
        "Unexpected property 'owner' in catalog metadata; only 'extensions' is allowed."
    );
    // Below v1.0 there is no metadata envelope to check.
    assert!(Catalog::from_json(json!({"catalogId": "ids", "metadata": {"owner": "x"}})).is_ok());
}
