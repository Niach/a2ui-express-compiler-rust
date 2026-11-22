//! Running the v1.0 cases again against v0.9 (`v0.9-lowered`).
//!
//! A port of upstream's `python/a2ui_agent/tests/conformance/lowering.py` (and
//! its TypeScript twin `lowering.ts`): a case's v1.0 catalogs and messages are
//! lowered to v0.9 before the call, and the v0.9 messages that come back are
//! lifted to the v1.0 shape the case expects.

use serde_json::{Map, Value, json};

use crate::cases::{Case, first_catalog_id, raw_catalog_doc};

pub const ONE_CATALOG_PER_SURFACE: &str =
    "v0.9 names one catalog per surface, so the case runs as written in v1.0 only.";
const RESERVED_PLAIN_KEYS: &str = "v0.9 reserves the plain path and call keys for data bindings and function calls, so a map literal that uses them runs as written in v1.0 only.";
const NO_USER_MESSAGE: &str = "The v0.9 Action has no userMessage field, so an event carrying one runs as written in v1.0 only.";
const NO_SYSTEM_FUNCTIONS: &str =
    "v0.9 defines no system functions, so a call to @index runs as written in v1.0 only.";
const EXTENSIONS: &str = "Surface and component extensions require A2UI v1.0 or later.";
const NO_CALL_RENDERER_FUNCTION: &str = "Protocol v0.9 has no callRendererFunction message.";

/// Cases that only make sense in v1.0, with the reason (upstream's
/// `V1_ONLY_CASES`, Express entries).
pub const V1_ONLY_CASES: &[(&str, &str)] = &[
    (
        "test_compile_express_index_system_function",
        NO_SYSTEM_FUNCTIONS,
    ),
    (
        "test_compile_express_index_system_function_positional_offset",
        NO_SYSTEM_FUNCTIONS,
    ),
    (
        "test_compile_express_index_system_function_named_offset",
        NO_SYSTEM_FUNCTIONS,
    ),
    (
        "test_decompile_express_index_system_function",
        NO_SYSTEM_FUNCTIONS,
    ),
    (
        "test_express_snippet_lists_system_functions_from_v1_0",
        NO_SYSTEM_FUNCTIONS,
    ),
    (
        "test_compile_express_system_function_with_catalog_id_is_a_validation_error",
        NO_SYSTEM_FUNCTIONS,
    ),
    (
        "test_compile_express_unknown_system_function_is_a_validation_error",
        NO_SYSTEM_FUNCTIONS,
    ),
    (
        "test_compile_express_unreferenced_variable_holding_a_system_function_call_with_catalog_id_is_a_validation_error",
        NO_SYSTEM_FUNCTIONS,
    ),
    (
        "test_compile_express_unreferenced_variable_holding_an_unknown_system_function_is_a_validation_error",
        NO_SYSTEM_FUNCTIONS,
    ),
    (
        "test_compile_express_root_holding_an_unknown_system_function_is_a_validation_error",
        NO_SYSTEM_FUNCTIONS,
    ),
    (
        "test_compile_express_variable_holding_a_system_function_call_is_inlined",
        NO_SYSTEM_FUNCTIONS,
    ),
    (
        "test_compile_express_variable_holding_a_system_function_call_with_catalog_id_is_a_validation_error",
        NO_SYSTEM_FUNCTIONS,
    ),
    (
        "test_decompile_express_system_function_with_catalog_id_is_a_validation_error",
        NO_SYSTEM_FUNCTIONS,
    ),
    (
        "test_decompile_express_unknown_system_function_is_a_validation_error",
        NO_SYSTEM_FUNCTIONS,
    ),
    ("test_compile_express_event_user_message", NO_USER_MESSAGE),
    (
        "test_compile_express_event_user_message_binding_with_context",
        NO_USER_MESSAGE,
    ),
    (
        "test_decompile_express_event_action_with_user_message",
        NO_USER_MESSAGE,
    ),
    (
        "test_compile_express_event_user_message_function_call",
        NO_USER_MESSAGE,
    ),
    (
        "test_decompile_express_event_action_with_function_call_user_message",
        NO_USER_MESSAGE,
    ),
    (
        "test_express_snippet_teaches_event_user_message_from_v1_0",
        NO_USER_MESSAGE,
    ),
    (
        "test_compile_express_surface_targeting_names_a_catalog",
        ONE_CATALOG_PER_SURFACE,
    ),
    (
        "test_decompile_express_two_surfaces_in_two_catalogs",
        ONE_CATALOG_PER_SURFACE,
    ),
    (
        "test_decompile_express_event_action_with_nested_context",
        "A v0.9 DynamicValue holds no map, so a map in an event's context runs as written in v1.0 only.",
    ),
    (
        "test_decompile_express_non_identifier_id_before_v1_0_derives_a_variable",
        "Already written against a v0.9 catalog to test pre-v1.0 component id derivation, so it runs as written without lowering.",
    ),
    (
        "test_decompile_express_non_identifier_id_from_v1_0_is_a_validation_error",
        "Before v1.0, component ids that are not UAX 31 identifiers are derived into variables rather than rejected.",
    ),
    (
        "test_compile_express_non_identifier_id_from_v1_0_is_a_validation_error",
        "Before v1.0, component ids need not be UAX 31 identifiers.",
    ),
    (
        "test_compile_express_map_literal_keeps_keys_named_like_express_constructs",
        RESERVED_PLAIN_KEYS,
    ),
    (
        "test_compile_express_map_literal_keeps_quoted_keys_named_like_constructs",
        RESERVED_PLAIN_KEYS,
    ),
    (
        "test_compile_express_map_literals_in_function_arguments_keep_their_keys",
        RESERVED_PLAIN_KEYS,
    ),
    (
        "test_decompile_express_map_with_keys_named_like_express_constructs",
        RESERVED_PLAIN_KEYS,
    ),
    (
        "test_decompile_express_map_with_plain_call_and_args_keys",
        RESERVED_PLAIN_KEYS,
    ),
    (
        "test_decompile_express_map_with_a_binding_key_beside_other_keys",
        "Lowered to v0.9, the @path key becomes the plain path key, which v0.9 reserves for data bindings, so the case runs as written in v1.0 only.",
    ),
    (
        "test_compile_express_map_with_call_like_keys_still_rejects_bindings",
        "Lowered to v0.9, this SDK does not reject a binding nested in a free-form object property.",
    ),
    (
        "test_compile_express_surface_extensions_variable_reference",
        "Surface extensions are only supported in A2UI v1.0 or later.",
    ),
    (
        "test_compile_express_empty_extensions_preserved",
        EXTENSIONS,
    ),
    (
        "test_decompile_express_component_metadata_writes_extensions",
        EXTENSIONS,
    ),
    (
        "test_decompile_express_legacy_bare_metadata_on_v1_0",
        EXTENSIONS,
    ),
    (
        "test_decompile_express_legacy_bare_checks_on_v1_0",
        "Tests v1.0 reading the legacy bare checks key, which is the v0.9 key, so it runs as written in v1.0 only.",
    ),
    (
        "test_compile_express_standalone_call_renderer_or_agent_function",
        NO_CALL_RENDERER_FUNCTION,
    ),
    (
        "test_compile_express_standalone_call_agent_only_function",
        NO_CALL_RENDERER_FUNCTION,
    ),
    (
        "test_compile_express_standalone_call_renderer_only_function_is_validation_error",
        NO_CALL_RENDERER_FUNCTION,
    ),
    (
        "test_compile_express_standalone_call_user_activation_function_is_validation_error",
        NO_CALL_RENDERER_FUNCTION,
    ),
    (
        "test_express_snippet_names_only_agent_callable_functions_for_standalone_calls",
        "Protocol v0.9 function schema does not support allowedCallers.",
    ),
];

/// Swaps between the v1.0 and v0.9 version strings.
pub fn swap_version(version: &Value) -> Value {
    match version.as_str() {
        Some("1.0" | "v1.0") => json!("v0.9"),
        Some("v0.9") => json!("1.0"),
        _ => version.clone(),
    }
}

fn is_at_least_1_0(version: &Value) -> bool {
    version
        .as_str()
        .is_some_and(|v| v.strip_prefix('v').unwrap_or(v).starts_with("1."))
}

fn replace_child_refs(node: &Value) -> Value {
    const CHILD: &str = "#/$defs/Child";
    match node {
        Value::Array(items) => Value::Array(items.iter().map(replace_child_refs).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| match v.as_str() {
                    Some(s) if k == "$ref" && s.ends_with(CHILD) => (
                        k.clone(),
                        json!(format!(
                            "{}#/$defs/ComponentId",
                            &s[..s.len() - CHILD.len()]
                        )),
                    ),
                    _ => (k.clone(), replace_child_refs(v)),
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Lowers a v1.0 catalog document to v0.9.
pub fn lower_catalog_document(document: &Value, default_catalog_id: Option<&str>) -> Value {
    let set_default_id = |doc: &mut Value| {
        if let (Some(id), Some(map)) = (default_catalog_id, doc.as_object_mut()) {
            if !id.is_empty() && !map.contains_key("catalogId") {
                map.insert("catalogId".into(), json!(id));
            }
        }
    };
    if let Some(proto) = document.get("protocolVersion") {
        if !is_at_least_1_0(proto) {
            let mut doc = document.clone();
            set_default_id(&mut doc);
            return doc;
        }
    }

    let mut doc = replace_child_refs(document);
    let Some(map) = doc.as_object_mut() else {
        return doc;
    };
    map.insert("protocolVersion".into(), json!("0.9"));
    set_default_id(&mut doc);
    let map = doc.as_object_mut().unwrap();

    if let Some(Value::Object(functions)) = map.get_mut("functions") {
        for function in functions.values_mut() {
            let Value::Object(f) = function else { continue };
            f.shift_remove("allowedCallers");
            f.shift_remove("requiresUserActivation");
            if f.get("returnType") == Some(&json!("validationResult")) {
                f.insert("returnType".into(), json!("boolean"));
            }
            if let Some(Value::Object(props)) = f.get_mut("properties") {
                if let Some(call) = props.shift_remove("@call") {
                    props.insert("call".into(), call);
                }
                if props.get("returnType").and_then(|r| r.get("const"))
                    == Some(&json!("validationResult"))
                {
                    props.insert("returnType".into(), json!({"const": "boolean"}));
                }
            }
            if let Some(Value::Array(required)) = f.get_mut("required") {
                for r in required.iter_mut() {
                    if r == "@call" {
                        *r = json!("call");
                    }
                }
            }
        }
    }

    if let Some(disc) = map
        .get_mut("$defs")
        .and_then(|d| d.get_mut("anyFunction"))
        .and_then(|f| f.get_mut("discriminator"))
        .and_then(Value::as_object_mut)
    {
        if disc.get("propertyName") == Some(&json!("@call")) {
            disc.insert("propertyName".into(), json!("call"));
        }
    }
    doc
}

fn rename_keys_in(node: &Value, lower: bool) -> Value {
    match node {
        Value::Array(items) => {
            Value::Array(items.iter().map(|v| rename_keys_in(v, lower)).collect())
        }
        Value::Object(map) => {
            let is_template = map.contains_key("componentId");
            let is_component = map.contains_key("id") && map.contains_key("component");
            let rename = |k: &str| -> String {
                if is_template {
                    return k.to_owned();
                }
                let renamed = match (lower, k) {
                    (true, "@path") => "path",
                    (true, "@call") => "call",
                    (false, "path") => "@path",
                    (false, "call") => "@call",
                    (true, "@checks") if is_component => "checks",
                    (true, "@accessibility") if is_component => "accessibility",
                    (false, "checks") if is_component => "@checks",
                    (false, "accessibility") if is_component => "@accessibility",
                    _ => k,
                };
                renamed.to_owned()
            };
            Value::Object(
                map.iter()
                    .map(|(k, v)| (rename(k), rename_keys_in(v, lower)))
                    .collect(),
            )
        }
        other => other.clone(),
    }
}

fn rename_reserved_keys(message: &Value, lower: bool) -> Map<String, Value> {
    let Some(map) = message.as_object() else {
        return Map::new();
    };
    map.iter()
        .map(|(k, v)| match v {
            Value::Object(body) => (
                k.clone(),
                Value::Object(
                    body.iter()
                        .map(|(fk, fv)| {
                            if fk == "components" || fk == "callFunction" {
                                (fk.clone(), rename_keys_in(fv, lower))
                            } else {
                                (fk.clone(), fv.clone())
                            }
                        })
                        .collect(),
                ),
            ),
            other => (k.clone(), other.clone()),
        })
        .collect()
}

/// Lowers v1.0 messages to v0.9, splitting a `createSurface` that carries
/// components or a data model into the v0.9 messages for them.
pub fn lower_messages(messages: &[Value], default_catalog_id: Option<&str>) -> Vec<Value> {
    let mut lowered = Vec::new();
    for message in messages {
        let mut msg = rename_reserved_keys(message, true);
        let version = swap_version(msg.get("version").unwrap_or(&Value::Null));
        msg.insert("version".into(), version.clone());
        if let Some(Value::Object(create)) = msg.get("createSurface") {
            let mut create = create.clone();
            let components = create.shift_remove("components");
            let data_model = create.shift_remove("dataModel");
            if let Some(id) = default_catalog_id {
                if !create.contains_key("catalogId") {
                    create.insert("catalogId".into(), json!(id));
                }
            }
            let surface_id = create.get("surfaceId").cloned().unwrap_or(Value::Null);
            lowered.push(json!({"version": version, "createSurface": create}));
            if let Some(components) = components.filter(|c| !c.is_null()) {
                lowered.push(json!({
                    "version": version,
                    "updateComponents": {"surfaceId": surface_id, "components": components},
                }));
            }
            if let Some(value) = data_model.filter(|d| !d.is_null()) {
                lowered.push(json!({
                    "version": version,
                    "updateDataModel": {"surfaceId": surface_id, "path": "/", "value": value},
                }));
            }
        } else {
            lowered.push(Value::Object(msg));
        }
    }
    lowered
}

/// Lifts v0.9 messages back to the v1.0 shape. With `join`, an
/// `updateComponents` or root `updateDataModel` right after the
/// `createSurface` of its surface is folded back into it.
pub fn lift_messages(messages: &[Value], join: bool) -> Vec<Value> {
    let mut lifted: Vec<Value> = Vec::new();
    for raw in messages {
        let msg = rename_reserved_keys(raw, false);
        if join {
            if let Some(last_create) = lifted
                .last_mut()
                .and_then(|m| m.get_mut("createSurface"))
                .and_then(Value::as_object_mut)
            {
                let last_surface = last_create.get("surfaceId").cloned();
                if let Some(Value::Object(upd)) = msg.get("updateComponents") {
                    if upd.get("surfaceId").cloned() == last_surface
                        && !last_create.contains_key("components")
                    {
                        let components = upd.get("components").cloned().unwrap_or(Value::Null);
                        last_create.insert("components".into(), components);
                        continue;
                    }
                }
                if let Some(Value::Object(upd)) = msg.get("updateDataModel") {
                    if upd.get("surfaceId").cloned() == last_surface
                        && upd.get("path") == Some(&json!("/"))
                        && !last_create.contains_key("dataModel")
                    {
                        let value = upd.get("value").cloned().unwrap_or(Value::Null);
                        last_create.insert("dataModel".into(), value);
                        continue;
                    }
                }
            }
        }
        let version = swap_version(msg.get("version").unwrap_or(&Value::Null));
        if let Some(Value::Object(create)) = msg.get("createSurface") {
            let create: Map<String, Value> = create
                .iter()
                .filter(|(k, v)| !(*k == "sendDataModel" && **v == json!(false)))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            lifted.push(json!({"version": version, "createSurface": create}));
        } else {
            let mut msg = msg;
            msg.insert("version".into(), version);
            lifted.push(Value::Object(msg));
        }
    }
    lifted
}

/// The protocol version the first catalog a case registers declares.
fn case_catalog_version(args: &Value) -> Option<String> {
    let entry = args
        .get("catalog")
        .or_else(|| args.get("catalogs").and_then(|c| c.get(0)))?;
    let (doc, _) = raw_catalog_doc(entry).ok()?;
    doc.get("protocolVersion")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// Why a case cannot run lowered to v0.9, or `None` when it can
/// (upstream's `v09_skip_reason`, the branches that reach Express suites).
pub fn v09_skip_reason(case: &Case) -> Option<String> {
    if let Some((_, reason)) = V1_ONLY_CASES.iter().find(|(name, _)| *name == case.name) {
        return Some((*reason).to_owned());
    }
    if let Some(version) = case_catalog_version(&case.args) {
        if matches!(version.as_str(), "0.8" | "v0.8" | "0.9" | "v0.9") {
            return Some(format!("Case already targets protocol version {version}."));
        }
    }
    let format = case.args.get("format").and_then(Value::as_str);
    if let Some(fmt @ ("elemental" | "atom")) = format {
        return Some(format!(
            "Experimental format {fmt} does not target lowered v0.9 suites."
        ));
    }
    let catalogs = case.args.get("catalogs").and_then(Value::as_array);
    if catalogs.is_some_and(|c| c.len() > 1) && format == Some("express") {
        return Some(ONE_CATALOG_PER_SURFACE.to_owned());
    }
    if format == Some("direct_json") && case.action == "generate_prompt_snippet" {
        return Some("The Direct JSON snippet embeds the schemas of the catalogs' version, so the case runs as written in v1.0 only.".to_owned());
    }
    if case.raw.to_string().contains("callRendererFunction") {
        return Some(NO_CALL_RENDERER_FUNCTION.to_owned());
    }
    None
}

/// The catalog id lowered messages name when a `createSurface` names none.
pub fn default_catalog_id(args: &Value) -> Option<String> {
    first_catalog_id(args)
}
