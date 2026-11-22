//! Comparing what the crate returned with what a case expects.

use regex::Regex;
use serde_json::{Value, json};

use crate::sdk::{Part, RawPart, SdkError, SdkResult};

/// Deep JSON equality, as the Python and TypeScript harnesses compare: key
/// order does not matter, and numbers compare by value, so `3` equals `3.0`.
pub fn json_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            if x.is_f64() || y.is_f64() {
                x.as_f64() == y.as_f64()
            } else {
                x == y
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| json_eq(a, b))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| json_eq(v, w)))
        }
        _ => a == b,
    }
}

fn pretty(value: &Value) -> String {
    let text = serde_json::to_string_pretty(value).unwrap_or_default();
    const LIMIT: usize = 4000;
    if text.len() > LIMIT {
        let mut end = LIMIT;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}\n… ({} bytes)", &text[..end], text.len())
    } else {
        text
    }
}

pub fn expect_json(what: &str, actual: &Value, expected: &Value) -> Result<(), String> {
    if json_eq(actual, expected) {
        Ok(())
    } else {
        Err(format!(
            "{what} differs\n--- expected\n{}\n--- actual\n{}",
            pretty(expected),
            pretty(actual)
        ))
    }
}

pub fn expect_text(what: &str, actual: &str, expected: &str) -> Result<(), String> {
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "{what} differs\n--- expected\n{expected:?}\n--- actual\n{actual:?}"
        ))
    }
}

pub fn expect_bool(what: &str, actual: bool, expected: bool) -> Result<(), String> {
    if actual == expected {
        Ok(())
    } else {
        Err(format!("{what}: expected {expected}, got {actual}"))
    }
}

fn strings(value: Option<&Value>) -> Vec<&str> {
    value
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

/// `expect_contains` and `expect_absent` on a text.
pub fn expect_fragments(
    text: &str,
    contains: Option<&Value>,
    absent: Option<&Value>,
) -> Result<(), String> {
    for fragment in strings(contains) {
        if !text.contains(fragment) {
            return Err(format!("{fragment:?} not in output:\n{text}"));
        }
    }
    for fragment in strings(absent) {
        if text.contains(fragment) {
            return Err(format!("{fragment:?} unexpectedly in output:\n{text}"));
        }
    }
    Ok(())
}

/// Checks an error against `expect_error`: its category, and its message
/// against `message` as an unanchored, case-sensitive regex (Python's
/// `re.search`).
pub fn check_error(error: &SdkError, expected: &Value) -> Result<(), String> {
    let category = expected.get("category").and_then(Value::as_str);
    let message = expected.get("message").and_then(Value::as_str);
    if let Some(category) = category {
        if error.category != category {
            return Err(format!(
                "expected a {category}, got {}: {}",
                error.category, error.message
            ));
        }
    }
    if let Some(pattern) = message.filter(|m| !m.is_empty()) {
        let re = Regex::new(pattern).map_err(|e| format!("bad message regex {pattern:?}: {e}"))?;
        if !re.is_match(&error.message) {
            return Err(format!(
                "{} message {:?} does not match {pattern:?}",
                error.category, error.message
            ));
        }
    }
    Ok(())
}

/// Runs `call` and checks that it fails as `expected` says.
pub fn expect_failure<T: std::fmt::Debug>(
    result: SdkResult<T>,
    expected: &Value,
) -> Result<SdkError, String> {
    match result {
        Ok(value) => Err(format!(
            "expected an error {expected}, got success: {value:?}"
        )),
        Err(error) => check_error(&error, expected).map(|_| error),
    }
}

/// Unwraps a call that should succeed.
pub fn ok<T>(what: &str, result: SdkResult<T>) -> Result<T, String> {
    result.map_err(|e| format!("{what} failed: {}: {}", e.category, e.message))
}

/// A parsed part in the suites' shape.
pub fn part_to_json(part: &Part) -> Value {
    match part {
        Part::Text(text) => json!({"text": text}),
        Part::A2ui(messages) => json!({"a2ui": messages}),
        Part::Error { category } => json!({"error": {"category": category}}),
    }
}

/// A raw part in the suites' shape; `is_final` only when false (upstream's
/// `raw_part_to_dict`).
pub fn raw_part_to_json(part: &RawPart) -> Value {
    match part {
        RawPart::Text { text, .. } => json!({"text": text}),
        RawPart::A2ui { raw, is_final } => {
            if *is_final {
                json!({"a2ui_raw": raw})
            } else {
                json!({"a2ui_raw": raw, "is_final": false})
            }
        }
    }
}

/// Drops `is_final: true` from expected raw parts, since it is the default.
pub fn without_final_flags(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(without_final_flags).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(k, v)| !(k.as_str() == "is_final" && **v == json!(true)))
                .map(|(k, v)| (k.clone(), without_final_flags(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Raw parts as a `wrap` case writes them.
pub fn to_raw_parts(parts: &Value) -> Result<Vec<RawPart>, String> {
    parts
        .as_array()
        .ok_or("parts is not a list")?
        .iter()
        .map(|p| {
            let is_final = p.get("is_final").and_then(Value::as_bool).unwrap_or(true);
            if let Some(text) = p.get("text").and_then(Value::as_str) {
                Ok(RawPart::Text {
                    text: text.to_owned(),
                    is_final,
                })
            } else if let Some(raw) = p.get("a2ui_raw").and_then(Value::as_str) {
                Ok(RawPart::A2ui {
                    raw: raw.to_owned(),
                    is_final,
                })
            } else {
                Err(format!("invalid raw part: {p}"))
            }
        })
        .collect()
}

/// Joins adjacent text parts, so a stream and a single-shot parse that split
/// text differently compare equal.
pub fn merge_text_parts(parts: &[Value]) -> Vec<Value> {
    let mut merged: Vec<Value> = Vec::new();
    for part in parts {
        let last_text = merged
            .last()
            .and_then(|l| l.get("text"))
            .and_then(Value::as_str);
        match (last_text, part.get("text").and_then(Value::as_str)) {
            (Some(last), Some(text)) => {
                let joined = json!({"text": format!("{last}{text}")});
                *merged.last_mut().unwrap() = joined;
            }
            _ => merged.push(part.clone()),
        }
    }
    merged
}

fn pointer_tokens(pointer: &str) -> Vec<&str> {
    pointer.trim_matches('/').split('/').collect()
}

fn step<'a>(value: &'a Value, token: &str) -> Option<&'a Value> {
    match value {
        Value::Array(items) => items.get(token.parse::<usize>().ok()?),
        Value::Object(map) => map.get(token),
        _ => None,
    }
}

/// The value at a `/`-separated path (upstream's `resolve_pointer`).
pub fn resolve_pointer<'a>(value: &'a Value, pointer: &str) -> Option<&'a Value> {
    pointer_tokens(pointer)
        .into_iter()
        .try_fold(value, |v, token| step(v, token))
}

/// Removes the value at a `/`-separated path (upstream's `delete_pointer`).
pub fn delete_pointer(value: &mut Value, pointer: &str) {
    let tokens = pointer_tokens(pointer);
    let Some((last, parents)) = tokens.split_last() else {
        return;
    };
    let mut current = value;
    for token in parents {
        let next = match current {
            Value::Array(items) => token.parse::<usize>().ok().and_then(|i| items.get_mut(i)),
            Value::Object(map) => map.get_mut(*token),
            _ => None,
        };
        match next {
            Some(n) => current = n,
            None => return,
        }
    }
    match current {
        Value::Array(items) => {
            if let Ok(i) = last.parse::<usize>() {
                if i < items.len() {
                    items.remove(i);
                }
            }
        }
        Value::Object(map) => {
            map.shift_remove(*last);
        }
        _ => {}
    }
}
