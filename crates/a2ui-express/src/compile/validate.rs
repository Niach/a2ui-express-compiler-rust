//! Checks on compiled messages, run before they leave the compiler.
//!
//! Upstream validates what it compiled against the catalogs and the
//! message schema (`validate_payload`). The compiler already checks every
//! value it writes against the catalog's schema helper (components,
//! properties, required properties, enums, bindings, function arguments),
//! so what is left is what a renderer would otherwise have to guard
//! against, ported from upstream's Python SDK:
//!
//! - a message states `"version": "1.0"`;
//! - a `callRendererFunction` names a function the agent may call: one an
//!   active catalog declares with `allowedCallers` `agentOnly` or
//!   `rendererOrAgent`, and that needs no user activation
//!   (`check_renderer_may_call`), and so may the calls nested in its arguments;
//! - every `@call` in a component names a declared function;
//! - no component carries an envelope key in its bare pre-1.0 spelling;
//! - a message lists each component id once;
//! - no value nests deeper than 50 levels, and every data path is a JSON Pointer (`validate_recursion_and_paths`,
//!   which a renderer runs on every message it receives).
//!
//! Full JSON Schema validation of each message is not ported: it needs a
//! JSON Schema engine, and everything it would catch in compiler output is
//! already refused while compiling.

use std::collections::HashSet;

use serde_json::Value;

use super::CompileError;
use crate::schema::{Catalog, CatalogSet, check_message_version};

/// How deep a message may nest objects and lists.
pub const MAX_DEPTH: usize = 50;

/// Checks compiled messages for `catalogs`; see the module docs.
///
/// [`Compiler::compile`](super::Compiler::compile), its streaming
/// counterpart and the response parser run this on everything they emit.
/// Messages from elsewhere (an agent's own JSON) can be checked with it too.
pub fn validate(catalogs: &CatalogSet, messages: &[Value]) -> Result<(), CompileError> {
    for message in messages {
        if let Some(map) = message.as_object() {
            check_message_version(map).map_err(super::error::from_schema_error)?;
        }
        check_depth_and_paths(message, 0)?;
        if let Some(body) = message.get("callRendererFunction") {
            check_agent_may_call(catalogs, body)?;
        }
        for key in ["createSurface", "updateComponents"] {
            let components = message
                .get(key)
                .and_then(|b| b.get("components"))
                .and_then(Value::as_array);
            let mut ids = HashSet::new();
            for component in components.into_iter().flatten() {
                let Some(map) = component.as_object() else {
                    continue;
                };
                if let Some(id) = map.get("id").and_then(Value::as_str) {
                    if !ids.insert(id) {
                        return Err(CompileError::validation(format!(
                            "Duplicate component id '{id}' in one message."
                        )));
                    }
                }
                catalogs
                    .refuse_legacy_envelope(
                        map,
                        message[key].get("catalogId").and_then(Value::as_str),
                    )
                    .map_err(super::error::from_schema_error)?;
                for (k, v) in map {
                    if k != "id" && k != "component" {
                        check_calls(catalogs, v)?;
                    }
                }
            }
        }
    }
    Ok(())
}

/// A `callRendererFunction` names a function the agent may call.
fn check_agent_may_call(catalogs: &CatalogSet, body: &Value) -> Result<(), CompileError> {
    let invalid = |m: String| Err(CompileError::validation(m));
    let subject = format!(
        "Function call '{}'",
        body.get("functionCallId")
            .and_then(Value::as_str)
            .unwrap_or("")
    );
    let call = &body["callFunction"];
    let name = call.get("@call").and_then(Value::as_str).unwrap_or("");
    let catalog_id = call.get("catalogId").and_then(Value::as_str).unwrap_or("");
    let Some(catalog) = catalogs.get(catalog_id) else {
        return invalid(format!(
            "{subject} names catalog '{catalog_id}', which isn't one of the active catalogs."
        ));
    };
    let Some(function) = catalog.function(name).filter(|f| !f.is_system()) else {
        return invalid(format!(
            "{subject} names '{name}', which catalog '{catalog_id}' does not declare."
        ));
    };
    let callers = function.allowed_callers();
    if !callers.admits_agent() {
        return invalid(format!(
            "{subject} names '{name}', which only the renderer may call (allowedCallers is {}).",
            callers.as_str()
        ));
    }
    if function.requires_user_activation() {
        return invalid(format!(
            "{subject} names '{name}', which requires a user activation, and a call from the agent has none."
        ));
    }
    match call.get("args") {
        Some(args) => check_nested_calls(catalogs, catalog, args, &subject),
        None => Ok(()),
    }
}

/// The calls nested in a `callRendererFunction`'s arguments run under the
/// agent's caller boundary too, outside any list template (`@index` has no
/// item there). One that names no catalog runs in the outer call's.
fn check_nested_calls(
    catalogs: &CatalogSet,
    home: &Catalog,
    value: &Value,
    subject: &str,
) -> Result<(), CompileError> {
    let map = match value {
        Value::Array(items) => {
            return items
                .iter()
                .try_for_each(|v| check_nested_calls(catalogs, home, v, subject));
        }
        Value::Object(map) => map,
        _ => return Ok(()),
    };
    if let Some(Value::String(name)) = map.get("@call") {
        let at = format!("{subject} holds a call to '{name}'");
        let invalid = |m: String| Err(CompileError::validation(m));
        if name.starts_with('@') {
            return invalid(format!(
                "{at}, but the arguments are resolved outside any list template."
            ));
        }
        let catalog = match map.get("catalogId").and_then(Value::as_str) {
            Some(id) => match catalogs.get(id) {
                Some(catalog) => catalog,
                None => {
                    return invalid(format!(
                        "{at} that names catalog '{id}', which isn't one of the active catalogs."
                    ));
                }
            },
            None => home,
        };
        let Some(function) = catalog.function(name) else {
            return invalid(format!(
                "{at}, which catalog '{}' does not declare.",
                catalog.id()
            ));
        };
        let callers = function.allowed_callers();
        if !callers.admits_agent() {
            return invalid(format!(
                "{at} that the agent can't make: Function '{name}' cannot be called by agent (allowedCallers is {}).",
                callers.as_str()
            ));
        }
        if function.requires_user_activation() {
            return invalid(format!(
                "{at} that the agent can't make: Function '{name}' requires a user activation, and a call from the agent has none."
            ));
        }
    }
    map.values()
        .try_for_each(|v| check_nested_calls(catalogs, home, v, subject))
}

/// Every `{"@call": name, …}` in a value names a declared function.
fn check_calls(catalogs: &CatalogSet, value: &Value) -> Result<(), CompileError> {
    match value {
        Value::Array(items) => items.iter().try_for_each(|v| check_calls(catalogs, v)),
        Value::Object(map) => {
            if map.contains_key("@path") {
                return Ok(());
            }
            if let Some(Value::String(name)) = map.get("@call") {
                let declared = catalogs
                    .catalogs()
                    .iter()
                    .any(|c| c.function(name).is_some());
                if !declared {
                    return Err(CompileError::validation(format!(
                        "Unrecognized function '{name}'"
                    )));
                }
            }
            map.values().try_for_each(|v| check_calls(catalogs, v))
        }
        _ => Ok(()),
    }
}

/// Upstream's `validate_recursion_and_paths`, with its wording. Function calls
/// have no depth limit of their own.
fn check_depth_and_paths(item: &Value, depth: usize) -> Result<(), CompileError> {
    if depth > MAX_DEPTH {
        return Err(CompileError::validation(format!(
            "Global recursion limit exceeded: Depth > {MAX_DEPTH}"
        )));
    }
    match item {
        Value::Array(items) => items
            .iter()
            .try_for_each(|x| check_depth_and_paths(x, depth + 1)),
        Value::Object(map) => {
            for key in ["path", "@path"] {
                if let Some(Value::String(path)) = map.get(key) {
                    if !is_relaxed_pointer(path) {
                        return Err(CompileError::validation(format!(
                            "Invalid path syntax: '{path}'"
                        )));
                    }
                }
            }
            map.values()
                .try_for_each(|v| check_depth_and_paths(v, depth + 1))
        }
        _ => Ok(()),
    }
}

/// Upstream's `RELAXED_PATH_PATTERN`: an absolute or relative JSON Pointer,
/// in which `~` only escapes (`~0`, `~1`).
fn is_relaxed_pointer(path: &str) -> bool {
    let mut chars = path.chars();
    while let Some(c) = chars.next() {
        if c == '~' && !matches!(chars.next(), Some('0' | '1')) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pointers_follow_the_relaxed_pattern() {
        for ok in ["", "/", "/a/b", "a/b", "/a~0b/~1", "a//b"] {
            assert!(is_relaxed_pointer(ok), "{ok}");
        }
        for bad in ["~", "/a~", "/a~2", "a~b"] {
            assert!(!is_relaxed_pointer(bad), "{bad}");
        }
    }

    #[test]
    fn depth_limit_and_paths() {
        let mut deep = json!(1);
        for _ in 0..=MAX_DEPTH {
            deep = json!([deep]);
        }
        let error = check_depth_and_paths(&deep, 0).unwrap_err();
        assert_eq!(error.message, "Global recursion limit exceeded: Depth > 50");

        let error = check_depth_and_paths(&json!({"@path": "/a~"}), 0).unwrap_err();
        assert_eq!(error.message, "Invalid path syntax: '/a~'");
    }
}
