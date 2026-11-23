//! Checks on compiled messages, run before they leave the compiler.
//!
//! Upstream validates what it compiled against the catalogs and the
//! message schema (`validate_payload`). The compiler already checks every
//! value it writes against the catalog's schema helper (components,
//! properties, required properties, enums, bindings, function arguments),
//! so what is left is what a renderer would otherwise have to guard
//! against, ported from upstream's Python SDK:
//!
//! - a `callRendererFunction` names a function the agent may call: one an
//!   active catalog declares with `allowedCallers` `agentOnly` or
//!   `rendererOrAgent`, and that needs no user activation
//!   (`check_renderer_may_call`);
//! - from v1.0, every `@call` in a component names a declared function;
//! - a message lists each component id once;
//! - no value nests deeper than 50 levels, nor function calls deeper than 5,
//!   and every data path is a JSON Pointer (`validate_recursion_and_paths`,
//!   which a renderer runs on every message it receives).
//!
//! Full JSON Schema validation of each message is not ported: it needs a
//! JSON Schema engine, and everything it would catch in compiler output is
//! already refused while compiling.

use std::collections::HashSet;

use serde_json::Value;

use super::CompileError;
use crate::schema::CatalogSet;

/// How deep a message may nest objects and lists.
pub const MAX_DEPTH: usize = 50;
/// How deep function calls may nest in a value.
pub const MAX_CALL_DEPTH: usize = 5;

/// Checks compiled messages for `catalogs`; see the module docs.
///
/// [`Compiler::compile`](super::Compiler::compile), its streaming
/// counterpart and the response parser run this on everything they emit.
/// Messages from elsewhere (an agent's own JSON) can be checked with it too.
pub fn validate(catalogs: &CatalogSet, messages: &[Value]) -> Result<(), CompileError> {
    let v1 = catalogs.version().at_least_v1();
    for message in messages {
        check_depth_and_paths(message, 0, 0)?;
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
                if v1 {
                    for (k, v) in map {
                        if k != "id" && k != "component" {
                            check_calls(catalogs, v)?;
                        }
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
    Ok(())
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

/// Upstream's `validate_recursion_and_paths`, with its wording.
fn check_depth_and_paths(item: &Value, depth: usize, calls: usize) -> Result<(), CompileError> {
    if depth > MAX_DEPTH {
        return Err(CompileError::validation(format!(
            "Global recursion limit exceeded: Depth > {MAX_DEPTH}"
        )));
    }
    let too_deep = || {
        CompileError::validation(format!(
            "Recursion limit exceeded: functionCall depth > {MAX_CALL_DEPTH}"
        ))
    };
    match item {
        Value::Array(items) => items
            .iter()
            .try_for_each(|x| check_depth_and_paths(x, depth + 1, calls)),
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
            if let Some(call @ Value::Object(_)) = map.get("functionCall") {
                if calls >= MAX_CALL_DEPTH {
                    return Err(too_deep());
                }
                return check_depth_and_paths(call, depth + 1, calls + 1);
            }
            if (map.contains_key("call") || map.contains_key("@call")) && map.contains_key("args") {
                if calls >= MAX_CALL_DEPTH {
                    return Err(too_deep());
                }
                return map.iter().try_for_each(|(k, v)| {
                    let calls = if k == "args" { calls + 1 } else { calls };
                    check_depth_and_paths(v, depth + 1, calls)
                });
            }
            map.values()
                .try_for_each(|v| check_depth_and_paths(v, depth + 1, calls))
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
    fn depth_and_call_limits() {
        let mut deep = json!(1);
        for _ in 0..=MAX_DEPTH {
            deep = json!([deep]);
        }
        let error = check_depth_and_paths(&deep, 0, 0).unwrap_err();
        assert_eq!(error.message, "Global recursion limit exceeded: Depth > 50");

        let nest = |n: usize| {
            let mut v = json!({"@path": "/x"});
            for _ in 0..n {
                v = json!({"@call": "f", "args": {"value": v}});
            }
            v
        };
        assert!(check_depth_and_paths(&nest(MAX_CALL_DEPTH), 0, 0).is_ok());
        let error = check_depth_and_paths(&nest(MAX_CALL_DEPTH + 1), 0, 0).unwrap_err();
        assert_eq!(
            error.message,
            "Recursion limit exceeded: functionCall depth > 5"
        );
        let error = check_depth_and_paths(&json!({"@path": "/a~"}), 0, 0).unwrap_err();
        assert_eq!(error.message, "Invalid path syntax: '/a~'");
    }
}
