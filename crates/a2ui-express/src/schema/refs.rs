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
/// component objects (`id` and `component` stay bare in every version).
pub const ENVELOPE_MIXIN_NAMES: [&str; 4] = ["catalogId", "metadata", "accessibility", "checks"];

/// The wire key of an envelope mixin: `@<name>` on v1.0, `<name>` below.
pub fn envelope_key(name: &str, at_least_v1: bool) -> String {
    if at_least_v1 {
        format!("@{name}")
    } else {
        name.to_owned()
    }
}

/// The key under which `component` holds envelope mixin `name`, if any.
///
/// On v1.0 prefers `@<name>`, falling back to a legacy bare `<name>` only when
/// the component does not declare `<name>` as a custom property.
pub fn envelope_read_key(
    component: &Map<String, Value>,
    name: &str,
    at_least_v1: bool,
    custom_properties: &[String],
) -> Option<String> {
    if at_least_v1 {
        let at_key = format!("@{name}");
        if component.contains_key(&at_key) {
            return Some(at_key);
        }
        if custom_properties.iter().any(|p| p == name) {
            return None;
        }
    }
    component.contains_key(name).then(|| name.to_owned())
}

/// Reads envelope mixin `name` from a component object.
pub fn envelope_value<'a>(
    component: &'a Map<String, Value>,
    name: &str,
    at_least_v1: bool,
    custom_properties: &[String],
) -> Option<&'a Value> {
    envelope_read_key(component, name, at_least_v1, custom_properties)
        .and_then(|k| component.get(&k))
}

/// Whether `$ref` is one a v1.0 catalog may hold: into itself or `common_types.json`.
pub(crate) fn is_supported_v1_ref(reference: &str) -> bool {
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
            common_def_name("https://a2ui.org/specification/v0_9/common_types.json#/$defs/Action"),
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
    fn envelope_keys_follow_the_version() {
        assert_eq!(envelope_key("checks", true), "@checks");
        assert_eq!(envelope_key("checks", false), "checks");
        let comp = json!({"@checks": [1], "checks": 2});
        let comp = comp.as_object().unwrap();
        assert_eq!(envelope_value(comp, "checks", true, &[]), Some(&json!([1])));
        assert_eq!(envelope_value(comp, "checks", false, &[]), Some(&json!(2)));
        let bare = json!({"metadata": 3});
        let bare = bare.as_object().unwrap();
        assert_eq!(envelope_value(bare, "metadata", true, &[]), Some(&json!(3)));
        assert_eq!(
            envelope_value(bare, "metadata", true, &["metadata".into()]),
            None
        );
    }

    #[test]
    fn v1_refs_stay_in_the_catalog_or_common_types() {
        assert!(is_supported_v1_ref("#/$defs/X"));
        assert!(is_supported_v1_ref("catalog.json#/components/X"));
        assert!(is_supported_v1_ref(
            "https://a2ui.org/specification/v1_0/common_types.json#/$defs/X"
        ));
        assert!(!is_supported_v1_ref("other.json#/$defs/X"));
    }
}
