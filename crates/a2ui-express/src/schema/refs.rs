//! `$ref` recognition, JSON Pointers and the component envelope keys.

use serde_json::{Map, Value};

use super::common::common_def_names;

fn is_word(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// `<prefix>$defs/<Name>` or `<prefix>definitions/<Name>`: the name, if a word.
fn def_name_after(rest: &str) -> Option<&str> {
    let name = rest
        .strip_prefix("$defs/")
        .or_else(|| rest.strip_prefix("definitions/"))?;
    is_word(name).then_some(name)
}

/// Recognizes a protocol common type referenced by a `$ref`.
///
/// Matches `common_types.json#/$defs/<Name>` relative or absolute
/// (`https://a2ui.org/specification/.../common_types.json#/$defs/<Name>`), and
/// a catalog-local `#/$defs/<Name>` whose name is a standard common type, under
/// either `$defs` or the older `definitions`.
pub fn common_def_name(reference: &str) -> Option<&str> {
    const DOC: &str = "common_types.json#/";
    if let Some(at) = reference.rfind(DOC) {
        let before = &reference[..at];
        if before.is_empty() || before.ends_with('/') {
            if let Some(name) = def_name_after(&reference[at + DOC.len()..]) {
                return Some(name);
            }
        }
    }
    let name = def_name_after(reference.strip_prefix("#/")?)?;
    common_def_names().contains(name).then_some(name)
}

/// [`common_def_name`] of a schema's `$ref`, when it has a string one.
pub(crate) fn common_def_of(schema: &Value) -> Option<&str> {
    schema.get("$ref")?.as_str().and_then(common_def_name)
}

/// The `$ref` of a schema when it points inside the same document (`#/...`).
pub(crate) fn local_ref(schema: &Value) -> Option<&str> {
    schema.get("$ref")?.as_str().filter(|r| r.starts_with("#/"))
}

/// The def name of `#/$defs/<Name>` or `#/definitions/<Name>`.
pub(crate) fn local_def_name(reference: &str) -> Option<&str> {
    reference
        .strip_prefix("#/$defs/")
        .or_else(|| reference.strip_prefix("#/definitions/"))
}

/// Resolves an RFC 6901 `#/...` pointer inside `root`.
pub(crate) fn resolve_json_pointer<'a>(root: &'a Value, pointer: &str) -> Option<&'a Value> {
    let rest = pointer.strip_prefix("#/")?;
    let mut current = root;
    for part in rest.split('/') {
        let key = part.replace("~1", "/").replace("~0", "~");
        current = match current {
            Value::Object(map) => map.get(&key)?,
            Value::Array(items) => items.get(key.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(current)
}

/// The envelope mixin names that `ComponentCommon` and `Checkable` add to
/// component objects as `@<name>` (`id` and `component` stay bare).
pub const ENVELOPE_MIXIN_NAMES: [&str; 4] = ["catalogId", "metadata", "accessibility", "checks"];

/// The wire key of an envelope mixin: `@<name>`.
pub fn envelope_key(name: &str) -> String {
    format!("@{name}")
}

/// Reads envelope mixin `name` from a component object, under its `@` key.
pub fn envelope_value<'a>(component: &'a Map<String, Value>, name: &str) -> Option<&'a Value> {
    component.get(&envelope_key(name))
}

/// Whether `$ref` is one a catalog may hold: into itself or `common_types.json`.
pub(crate) fn is_supported_ref(reference: &str) -> bool {
    if reference.starts_with('#') {
        return true;
    }
    let document = reference.split('#').next().unwrap_or("");
    document == "catalog.json" || document.rsplit('/').next() == Some("common_types.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn recognizes_common_type_refs() {
        assert_eq!(
            common_def_name("https://a2ui.org/specification/v1_0/common_types.json#/$defs/Action"),
            Some("Action")
        );
        assert_eq!(
            common_def_name("common_types.json#/$defs/DynamicString"),
            Some("DynamicString")
        );
        assert_eq!(
            common_def_name("common_types.json#/definitions/Action"),
            Some("Action")
        );
        assert_eq!(common_def_name("#/$defs/ChildList"), Some("ChildList"));
        assert_eq!(common_def_name("#/$defs/MyDynamicString"), None);
        assert_eq!(common_def_name("#/definitions/MyThing"), None);
        assert_eq!(common_def_name("other_schema.json#/$defs/Action"), None);
        assert_eq!(common_def_name("my_common_types.json#/$defs/Action"), None);
        assert_eq!(common_def_name("common_types.json#/$defs/A-B"), None);
    }

    #[test]
    fn resolves_escaped_pointers() {
        let doc = json!({"a/b": {"~x": [1, {"y": 2}]}});
        assert_eq!(
            resolve_json_pointer(&doc, "#/a~1b/~0x/1/y"),
            Some(&json!(2))
        );
        assert_eq!(resolve_json_pointer(&doc, "#/nope"), None);
        assert_eq!(resolve_json_pointer(&doc, "a"), None);
    }

    #[test]
    fn envelope_keys_carry_an_at() {
        assert_eq!(envelope_key("checks"), "@checks");
        let comp = json!({"@checks": [1], "checks": 2, "metadata": 3});
        let comp = comp.as_object().unwrap();
        assert_eq!(envelope_value(comp, "checks"), Some(&json!([1])));
        assert_eq!(envelope_value(comp, "metadata"), None);
    }

    #[test]
    fn refs_stay_in_the_catalog_or_common_types() {
        assert!(is_supported_ref("#/$defs/X"));
        assert!(is_supported_ref("catalog.json#/components/X"));
        assert!(is_supported_ref(
            "https://a2ui.org/specification/v1_0/common_types.json#/$defs/X"
        ));
        assert!(!is_supported_ref("other.json#/$defs/X"));
    }
}
