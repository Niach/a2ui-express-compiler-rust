//! Literals and names: how numbers, strings and component ids are spelled.

use std::collections::{HashMap, HashSet};

use serde_json::Number;

use crate::syntax::lexer::{is_id_continue, is_id_start, is_identifier};
use crate::{Error, Result};

/// Whether `s` is a UAX #31 identifier (reserved words included), which is
/// what a v1.0 component id must be.
pub fn is_uax31_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if is_id_start(c)) && chars.all(is_id_continue)
}

/// Derives an Express variable name from an arbitrary component id.
///
/// Every code point outside `XID_Continue` becomes `_`, then `_` is
/// prefixed until the result is a non-reserved identifier.
pub fn derive_variable_name(component_id: &str) -> String {
    let mut base: String = component_id
        .chars()
        .map(|c| if is_id_continue(c) { c } else { '_' })
        .collect();
    while !is_identifier(&base) {
        base.insert(0, '_');
    }
    base
}

/// Maps each component id of a block to the variable name it is written as.
///
/// An id that is an Express identifier is its own name. Any other id (a
/// pre-v1.0 `content-grid`, or a reserved word like `true`) gets a derived
/// name (`content_grid`, `_true`) that no other id or variable of the block
/// uses, suffixed `_2`, `_3`, … on a collision.
pub fn assign_variable_names<'a>(
    component_ids: impl IntoIterator<Item = &'a str> + Clone,
) -> HashMap<String, String> {
    let mut names = HashMap::new();
    let mut taken = HashSet::new();
    for cid in component_ids.clone() {
        if is_identifier(cid) {
            names.insert(cid.to_owned(), cid.to_owned());
            taken.insert(cid.to_owned());
        }
    }
    for cid in component_ids {
        if names.contains_key(cid) {
            continue;
        }
        let base = derive_variable_name(cid);
        let mut name = base.clone();
        let mut suffix = 2;
        while taken.contains(&name) {
            name = format!("{base}_{suffix}");
            suffix += 1;
        }
        names.insert(cid.to_owned(), name.clone());
        taken.insert(name);
    }
    names
}

/// Writes a number the way ECMAScript `Number.prototype.toString` does:
/// plain decimal for 1e-6 <= |v| < 1e21, otherwise `<digits>e-7` /
/// `<digits>e+21`. Integers are written as their digits; an integral double
/// keeps a `.0` so it recompiles to a double; negative zero is `-0.0`.
pub fn number(n: &Number) -> Result<String> {
    if n.is_i64() || n.is_u64() {
        return Ok(n.to_string());
    }
    let v = n
        .as_f64()
        .ok_or_else(|| Error::Validation(format!("The number {n} has no Express notation.")))?;
    float(v)
}

/// [`number`] for a double.
pub fn float(v: f64) -> Result<String> {
    if !v.is_finite() {
        return Err(Error::Validation(format!(
            "The number {v} has no Express notation."
        )));
    }
    if v == 0.0 {
        return Ok(if v.is_sign_negative() { "-0.0" } else { "0.0" }.to_owned());
    }
    // `{:e}` gives the shortest round-tripping digits: `d.ddde<exp>`.
    let sci = format!("{:e}", v.abs());
    let (mantissa, exp) = sci.split_once('e').expect("`{:e}` writes an exponent");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let exp: i32 = exp.parse().expect("`{:e}` writes an integral exponent");
    let k = digits.len() as i32;
    let n = exp + 1;
    let body = if k <= n && n <= 21 {
        format!("{digits}{}.0", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let e = n - 1;
        let sign = if e < 0 { '-' } else { '+' };
        let mantissa = if k == 1 {
            digits
        } else {
            format!("{}.{}", &digits[..1], &digits[1..])
        };
        format!("{mantissa}e{sign}{}", e.abs())
    };
    Ok(if v < 0.0 { format!("-{body}") } else { body })
}

/// Writes a string literal in its most readable spelling: a (raw)
/// triple-quoted string for multi-line text, a raw `r"..."` string for text
/// with backslashes, otherwise a double-quoted string with escapes.
pub fn string(val: &str) -> String {
    let has_newline = val.contains('\n') || val.contains('\r');
    let has_tab = val.contains('\t');
    let has_backslash = val.contains('\\');

    if has_newline && !val.ends_with('"') && !val.contains("\"\"\"") {
        if has_backslash && !has_tab {
            return format!("r\"\"\"{val}\"\"\"");
        }
        let escaped = val.replace('\\', "\\\\").replace('\t', "\\t");
        return format!("\"\"\"{escaped}\"\"\"");
    }

    if has_backslash && !has_newline && !has_tab && !val.contains('"') {
        return format!("r\"{val}\"");
    }

    let mut out = String::with_capacity(val.len() + 2);
    out.push('"');
    for c in val.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Writes a map key: bare when it is an identifier, quoted otherwise.
pub fn key(k: &str) -> String {
    if is_identifier(k) {
        k.to_owned()
    } else {
        string(k)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_follow_ecmascript() {
        let cases = [
            (2.5e-7, "2.5e-7"),
            (1.5e21, "1.5e+21"),
            (1e21, "1e+21"),
            (0.000001, "0.000001"),
            (123456.789, "123456.789"),
            (3.0, "3.0"),
            (-0.0, "-0.0"),
            (-1.25, "-1.25"),
            (1e20, "100000000000000000000.0"),
            (0.1, "0.1"),
            (1e-7, "1e-7"),
        ];
        for (v, s) in cases {
            assert_eq!(float(v).unwrap(), s, "{v}");
        }
        assert!(float(f64::NAN).is_err());
    }

    #[test]
    fn strings_pick_the_cleanest_spelling() {
        assert_eq!(string("a\"b"), r#""a\"b""#);
        assert_eq!(string(r"C:\temp"), r#"r"C:\temp""#);
        assert_eq!(string("a\\b\nc"), "r\"\"\"a\\b\nc\"\"\"");
        assert_eq!(string("a\tb"), r#""a\tb""#);
        assert_eq!(string("ends\n\""), r#""ends\n\"""#);
    }

    #[test]
    fn variable_names_are_mangled_without_collisions() {
        let ids = ["title-heading", "title_heading", "true", "1st", "ok"];
        let names = assign_variable_names(ids);
        assert_eq!(names["title_heading"], "title_heading");
        assert_eq!(names["title-heading"], "title_heading_2");
        assert_eq!(names["true"], "_true");
        assert_eq!(names["1st"], "_1st");
        assert_eq!(names["ok"], "ok");
    }
}
