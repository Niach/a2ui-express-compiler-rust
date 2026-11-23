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

fn eui() -> Compiler {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../vendor/eui/catalog/core.schema.json"
    );
    let catalog = Catalog::from_json_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    Compiler::new(vec![catalog], None).unwrap()
}

/// A check on a component that binds no value would read its own message
/// (`required(value: "Email is required")`), so it is an error; a bound
/// field, or a check that names its value, compiles.
#[test]
fn a_check_with_nothing_to_read_is_an_error() {
    let err = eui()
        .compile(
            r#"root = Input(label="Email", name="email", checks=[?required("Email is required")])"#,
            "main",
        )
        .unwrap_err();
    assert_eq!(err.category(), Category::Validation, "{err:?}");
    assert!(err.to_string().contains("no value to check"), "{err}");

    let out = eui()
        .compile(
            r#"root = Input(label="Email", name="email", value=$/email, checks=[?required("Email is required"), ?email("Invalid")])"#,
            "main",
        )
        .unwrap();
    let checks = &out[0]["createSurface"]["components"][0]["@checks"];
    assert_eq!(
        checks[0]["condition"]["args"]["value"],
        json!({"@path": "/email"})
    );
    assert_eq!(checks[0]["message"], "Email is required");

    let out = eui()
        .compile(
            r#"root = Checkbox(label="Terms", name="terms", checks=[?required($/terms, "Accept the terms")])"#,
            "main",
        )
        .unwrap();
    let checks = &out[0]["createSurface"]["components"][0]["@checks"];
    assert_eq!(
        checks[0]["condition"]["args"]["value"],
        json!({"@path": "/terms"})
    );
    assert_eq!(checks[0]["message"], "Accept the terms");
}

/// A catalog with one function per caller rule, for the post-compile checks
/// (#AXC-16).
fn callers_catalog() -> Catalog {
    let function = |callers: &str, activation: bool| {
        json!({
            "allowedCallers": callers,
            "requiresUserActivation": activation,
            "properties": {"@call": {}, "args": {"type": "object", "properties": {
                "value": {"type": "string"}
            }}}
        })
    };
    let mut doc = json!({
        "catalogId": "demo",
        "protocolVersion": "1.0",
        "components": {"Text": {"properties": {
            "component": {"const": "Text"},
            "text": {"$ref": "common_types.json#/$defs/DynamicString"}
        }, "required": ["component", "text"]}},
        "functions": {
            "toast": function("agentOnly", false),
            "notify": function("rendererOrAgent", false),
            "format": function("rendererOnly", false),
            "launch": function("rendererOnly", true)
        }
    });
    for (name, f) in doc["functions"].as_object_mut().unwrap() {
        f["properties"]["@call"] = json!({"const": name});
    }
    Catalog::from_json(doc).unwrap()
}

/// The public compile API (not only the conformance seam) refuses a
/// standalone call the agent may not make, in every entry point.
#[test]
fn standalone_calls_name_functions_the_agent_may_call() {
    use a2ui_express::response::{ExpressParser, Part};

    let compiler = Compiler::new(vec![callers_catalog()], None).unwrap();
    for ok in [r#"toast("hi")"#, r#"notify("hi")"#] {
        let messages = compiler.compile(ok, "main").unwrap();
        assert!(messages[0].get("callRendererFunction").is_some(), "{ok}");
    }
    for (source, reason) in [
        (r#"format("hi")"#, "only the renderer may call"),
        (r#"launch("hi")"#, "only the renderer may call"),
    ] {
        let error = compiler.compile(source, "main").unwrap_err();
        assert_eq!(error.category(), Category::Validation, "{source}");
        assert!(
            error.message.contains(reason),
            "{source}: {}",
            error.message
        );

        // The response parser and the stream run the same check.
        let parser = ExpressParser::new(compiler.clone(), "main");
        let wrapped = format!("<a2ui>\n{source}\n</a2ui>");
        let parts = parser.parse_response(&wrapped, true);
        assert!(
            parts.is_err()
                || parts
                    .as_ref()
                    .unwrap()
                    .iter()
                    .any(|p| matches!(p, Part::Error(_))),
            "{source}: {parts:?}"
        );
        let mut stream = parser.open_stream();
        let streamed = stream.push(&wrapped).and_then(|mut parts| {
            parts.extend(stream.finish()?);
            Ok(parts)
        });
        let emitted_call = streamed.as_ref().is_ok_and(|parts| {
            parts.iter().any(|p| match p {
                Part::A2ui(messages) => messages
                    .iter()
                    .any(|m| m.get("callRendererFunction").is_some()),
                _ => false,
            })
        });
        assert!(!emitted_call, "{source}: {streamed:?}");
    }
}

/// `compile::validate` is public, for messages that did not come from the
/// compiler.
#[test]
fn validate_checks_messages_from_elsewhere() {
    use a2ui_express::compile::validate;

    let compiler = Compiler::new(vec![callers_catalog()], None).unwrap();
    let catalogs = compiler.catalogs();
    let create = |components: Value| {
        vec![json!({"version": "1.0", "createSurface": {
            "surfaceId": "main", "catalogId": "demo", "components": components
        }})]
    };
    let message = |messages: &[Value]| validate(catalogs, messages).unwrap_err().message;

    assert!(
        validate(
            catalogs,
            &create(json!([{"id": "root", "component": "Text", "text": "x"}]))
        )
        .is_ok()
    );
    assert_eq!(
        message(&create(json!([
            {"id": "root", "component": "Text", "text": "x"},
            {"id": "root", "component": "Text", "text": "y"}
        ]))),
        "Duplicate component id 'root' in one message."
    );
    assert_eq!(
        message(&create(json!([{"id": "root", "component": "Text",
            "text": {"@call": "missing", "args": {}}}]))),
        "Unrecognized function 'missing'"
    );
    // A renderer-only function inside a component is fine: the renderer runs it.
    assert!(
        validate(
            catalogs,
            &create(json!([{"id": "root", "component": "Text",
        "text": {"@call": "format", "args": {"value": "x"}}}]))
        )
        .is_ok()
    );
    assert_eq!(
        message(&create(
            json!([{"id": "root", "component": "Text", "text": {"@path": "/a~"}}])
        )),
        "Invalid path syntax: '/a~'"
    );
    let mut nested = json!({"@path": "/v"});
    for _ in 0..6 {
        nested = json!({"@call": "notify", "args": {"value": nested}});
    }
    assert_eq!(
        message(&create(
            json!([{"id": "root", "component": "Text", "text": nested}])
        )),
        "Recursion limit exceeded: functionCall depth > 5"
    );
    assert_eq!(
        message(&[json!({"version": "1.0", "callRendererFunction": {
            "functionCallId": "c1",
            "callFunction": {"@call": "toast", "catalogId": "other", "args": {}}
        }})]),
        "Function call 'c1' names catalog 'other', which isn't one of the active catalogs."
    );
}
