//! The compiler's typed errors and version targets, beyond what the
//! conformance suite pins.

use a2ui_express::compile::{Category, CompileErrorKind, Compiler};
use a2ui_express::schema::{Catalog, CatalogVersion};
use serde_json::{Value, json};

fn catalog(version: &str) -> Catalog {
    Catalog::from_json(json!({
        "catalogId": "demo",
        "protocolVersion": version,
        "components": {
            "Text": {"properties": {
                "component": {"const": "Text"},
                "text": {"$ref": "common_types.json#/$defs/DynamicString"},
                "variant": {"type": "string", "enum": ["body", "caption"]}
            }, "required": ["component", "text"]},
            "Column": {"properties": {
                "component": {"const": "Column"},
                "children": {"$ref": "common_types.json#/$defs/ChildList"},
                "gap": {"type": "number"}
            }}
        },
        "functions": {
            "upper": {"returnType": "string", "properties": {
                "@call": {"const": "upper"},
                "args": {"type": "object", "properties": {"value": {"type": "string"}}}
            }}
        }
    }))
    .unwrap()
}

fn compile(version: &str, source: &str) -> Result<Vec<Value>, a2ui_express::compile::CompileError> {
    Compiler::new(vec![catalog(version)], None)
        .unwrap()
        .compile(source, "main")
}

fn kind(source: &str) -> CompileErrorKind {
    compile("1.0", source).unwrap_err().kind
}

#[test]
fn errors_are_typed() {
    assert_eq!(
        kind("root = Nope()"),
        CompileErrorKind::UnknownComponent {
            component: "Nope".into()
        }
    );
    assert_eq!(
        kind("root = Text(nope(1))"),
        CompileErrorKind::UnknownFunction {
            function: "nope".into()
        }
    );
    assert_eq!(
        kind("root = Text(variant=\"body\")"),
        CompileErrorKind::MissingRequiredProperty {
            component: "Text".into(),
            property: "text".into()
        }
    );
    assert_eq!(
        kind("root = Text(\"a\", size=3)"),
        CompileErrorKind::UnknownProperty {
            component: "Text".into(),
            property: "size".into()
        }
    );
    assert_eq!(
        kind("root = Text(\"a\", text=\"b\")"),
        CompileErrorKind::DuplicateProperty {
            component: "Text".into(),
            property: "text".into()
        }
    );
    assert_eq!(
        kind("root = Text(upper(\"a\", value=\"b\"))"),
        CompileErrorKind::DuplicateParam {
            function: "upper".into(),
            param: "value".into()
        }
    );
    assert_eq!(
        kind("root = Text(\"a\", $/v)"),
        CompileErrorKind::ForbiddenDatabinding {
            component: "Text".into(),
            property: "variant".into()
        }
    );
    assert_eq!(
        kind("root = Column([Text(\"a\")])\nroot_children_0 = Text(\"b\")"),
        CompileErrorKind::IdCollision {
            id: "root_children_0".into()
        }
    );
    let e = compile("1.0", "").unwrap_err();
    assert_eq!(
        e.kind,
        CompileErrorKind::UndefinedRoot {
            target: "root".into()
        }
    );
    assert_eq!(e.category(), Category::Parse);
    assert_eq!(e.message, "Root target 'root' is not defined.");
}

#[test]
fn v0_9_splits_create_surface() {
    let messages = compile(
        "0.9",
        "root = Column([t], gap=4)\nt = Text($/name)\n$/name = \"Ada\"",
    )
    .unwrap();
    assert_eq!(
        messages,
        vec![
            json!({"version": "v0.9", "createSurface": {"surfaceId": "main", "catalogId": "demo"}}),
            json!({"version": "v0.9", "updateComponents": {"surfaceId": "main", "components": [
                {"id": "root", "component": "Column", "children": ["t"], "gap": 4},
                {"id": "t", "component": "Text", "text": {"path": "/name"}}
            ]}}),
            json!({"version": "v0.9", "updateDataModel": {"surfaceId": "main", "path": "/", "value": {"name": "Ada"}}}),
        ]
    );
}

#[test]
fn v1_only_features_are_rejected_on_v0_9() {
    for source in [
        "root = Text(\"a\")\nupper(\"x\")",
        "root = Column(on=Event(\"e\", userMessage=\"hi\"))",
        "surface(\"s\", extensions={a: 1})\nroot = Text(\"a\")",
    ] {
        let e = compile("0.9", source).unwrap_err();
        assert_eq!(
            e.category(),
            Category::Validation,
            "{source}: {}",
            e.message
        );
    }
}

#[test]
fn a_requested_patch_version_is_emitted() {
    let compiler = Compiler::new(vec![catalog("0.9")], Some(CatalogVersion::V0_9_1)).unwrap();
    let messages = compiler.compile("t = Text(\"a\")", "main").unwrap();
    assert_eq!(messages[0]["version"], "v0.9.1");
    assert!(Compiler::new(vec![catalog("0.9")], Some(CatalogVersion::V1_0)).is_err());
}

#[test]
fn update_scope_and_delete() {
    let messages = compile(
        "1.0",
        "surface(\"s\", update=true)\nroot = Text(\"a\")\n$/x = 1\ndeleteSurface(\"old\")",
    )
    .unwrap();
    assert_eq!(
        messages,
        vec![
            json!({"version": "1.0", "updateComponents": {"surfaceId": "s", "components": [
                {"id": "root", "component": "Text", "text": "a"}
            ]}}),
            json!({"version": "1.0", "updateDataModel": {"surfaceId": "s", "path": "/x", "value": 1}}),
            json!({"version": "1.0", "deleteSurface": {"surfaceId": "old"}}),
        ]
    );
}
