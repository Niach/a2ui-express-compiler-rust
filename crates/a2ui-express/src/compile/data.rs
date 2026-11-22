//! Data paths: folding `$/a/b = value` assignments into one data model.

use serde_json::{Map, Value};

use super::error::CompileError;

const MAX_ARRAY_INDEX: usize = 10000;

/// The path without its `$` (and the `/` after it).
pub(crate) fn strip_path_prefix(path: &str) -> &str {
    path.strip_prefix("$/")
        .or_else(|| path.strip_prefix('$'))
        .unwrap_or(path)
}

/// The JSON Pointer of a data path: `$/a/b` is `/a/b`, `$` is `/`.
pub(crate) fn to_json_pointer(path: &str) -> String {
    format!("/{}", strip_path_prefix(path))
}

fn is_array_index(segment: &str) -> bool {
    segment == "0"
        || (!segment.is_empty()
            && !segment.starts_with('0')
            && segment.bytes().all(|b| b.is_ascii_digit()))
}

fn parse_array_index(segment: &str, path: &str) -> Result<usize, CompileError> {
    if !is_array_index(segment) {
        return Err(CompileError::validation(format!(
            "Cannot use non-numeric segment '{segment}' on an array in path '{path}'."
        )));
    }
    match segment.parse::<usize>() {
        Ok(i) if i <= MAX_ARRAY_INDEX => Ok(i),
        _ => Err(CompileError::validation(format!(
            "Cannot set path '{path}': array index '{segment}' exceeds maximum supported index ({MAX_ARRAY_INDEX})."
        ))),
    }
}

fn primitive_error(path: &str, segment: &str) -> CompileError {
    CompileError::validation(format!(
        "Cannot set path '{path}': segment '{segment}' is a primitive value."
    ))
}

/// The container `segment` names in `current`, created (a list when the next
/// segment is an index, else a map) when absent or null.
fn step<'v>(
    current: &'v mut Value,
    segment: &str,
    next: &str,
    path: &str,
) -> Result<&'v mut Value, CompileError> {
    let fresh = || {
        if is_array_index(next) {
            Value::Array(Vec::new())
        } else {
            Value::Object(Map::new())
        }
    };
    let slot = match current {
        Value::Array(items) => {
            let idx = parse_array_index(segment, path)?;
            if items.len() <= idx {
                items.resize(idx + 1, Value::Null);
            }
            &mut items[idx]
        }
        Value::Object(map) => map.entry(segment.to_owned()).or_insert(Value::Null),
        _ => unreachable!("only containers are stepped into"),
    };
    match slot {
        Value::Null => {
            *slot = fresh();
            Ok(slot)
        }
        Value::Array(_) | Value::Object(_) => Ok(slot),
        _ => Err(primitive_error(path, segment)),
    }
}

/// Sets `value` at data path `path` (`$/user/name`, `$/items/0/state`) in
/// `model`, creating the maps and lists on the way (`setNestedPath`).
pub(crate) fn set_nested_path(
    model: &mut Map<String, Value>,
    path: &str,
    value: Value,
) -> Result<(), CompileError> {
    let clean = strip_path_prefix(path);
    if clean.is_empty() {
        let Value::Object(map) = value else {
            return Err(CompileError::validation(format!(
                "Cannot set root data model '{path}' to a non-map value."
            )));
        };
        *model = map;
        return Ok(());
    }

    let keys: Vec<&str> = clean.split('/').collect();
    let mut root = Value::Object(std::mem::take(model));
    let result = (|| {
        let mut current = &mut root;
        for pair in keys.windows(2) {
            current = step(current, pair[0], pair[1], path)?;
        }
        let last = keys[keys.len() - 1];
        match current {
            Value::Object(map) => {
                map.insert(last.to_owned(), value);
            }
            Value::Array(items) => {
                let idx = parse_array_index(last, path)?;
                if items.len() <= idx {
                    items.resize(idx + 1, Value::Null);
                }
                items[idx] = value;
            }
            _ => unreachable!("only containers are stepped into"),
        }
        Ok(())
    })();
    let Value::Object(map) = root else {
        unreachable!("the root stays a map")
    };
    *model = map;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn model(assignments: &[(&str, Value)]) -> Result<Value, CompileError> {
        let mut m = Map::new();
        for (p, v) in assignments {
            set_nested_path(&mut m, p, v.clone())?;
        }
        Ok(Value::Object(m))
    }

    #[test]
    fn nested_maps_and_lists_are_created() {
        let m = model(&[
            ("$/user/name", json!("Ada")),
            ("$/items/1/state", json!("done")),
        ])
        .unwrap();
        assert_eq!(
            m,
            json!({"user": {"name": "Ada"}, "items": [null, {"state": "done"}]})
        );
    }

    #[test]
    fn the_root_is_replaced_by_a_map_only() {
        let m = model(&[("$/a", json!(1)), ("$/", json!({"b": 2}))]).unwrap();
        assert_eq!(m, json!({"b": 2}));
        assert!(model(&[("$", json!(3))]).is_err());
    }

    #[test]
    fn stepping_through_a_primitive_fails() {
        let e = model(&[("$/a", json!(1)), ("$/a/b", json!(2))]).unwrap_err();
        assert!(e.message.contains("primitive"), "{}", e.message);
    }

    #[test]
    fn array_indexes_are_checked() {
        assert!(model(&[("$/l/0", json!(1)), ("$/l/x", json!(2))]).is_err());
        assert!(model(&[("$/l/10001", json!(1))]).is_err());
    }

    #[test]
    fn pointers() {
        assert_eq!(to_json_pointer("$/a/b"), "/a/b");
        assert_eq!(to_json_pointer("$"), "/");
        assert_eq!(to_json_pointer("$/"), "/");
    }
}
