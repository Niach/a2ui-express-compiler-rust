//! The protocol common types (`common_types.json`), embedded per version.
//!
//! `schemas/` in the crate holds copies of `vendor/a2ui/specification/*/json/
//! common_types.json`, written by `scripts/sync-upstream.sh`, so the
//! published crate carries them.

use std::collections::HashSet;
use std::sync::OnceLock;

use serde_json::{Map, Value};

use super::CatalogVersion;

const V0_9_COMMON_TYPES: &str = include_str!("../../schemas/v0_9/common_types.json");
const V1_0_COMMON_TYPES: &str = include_str!("../../schemas/v1_0/common_types.json");

fn parse(text: &str) -> Value {
    serde_json::from_str(text).expect("vendored common_types.json is valid JSON")
}

/// The `common_types.json` document of a protocol version.
pub fn common_types(version: CatalogVersion) -> &'static Value {
    static V0_9: OnceLock<Value> = OnceLock::new();
    static V1_0: OnceLock<Value> = OnceLock::new();
    match version {
        CatalogVersion::V0_9 | CatalogVersion::V0_9_1 => {
            V0_9.get_or_init(|| parse(V0_9_COMMON_TYPES))
        }
        CatalogVersion::V1_0 => V1_0.get_or_init(|| parse(V1_0_COMMON_TYPES)),
    }
}

/// The `$defs` (or older `definitions`) of a common types document.
pub(crate) fn defs_of(common: &Value) -> Option<&Map<String, Value>> {
    common
        .get("$defs")
        .or_else(|| common.get("definitions"))
        .and_then(Value::as_object)
}

/// The names of every standard common type, across protocol versions.
///
/// A catalog-local `#/$defs/<Name>` whose name is one of these is read as an
/// alias of the common type.
pub(crate) fn common_def_names() -> &'static HashSet<String> {
    static NAMES: OnceLock<HashSet<String>> = OnceLock::new();
    NAMES.get_or_init(|| {
        let mut names: HashSet<String> =
            ["ComponentCommon", "ComponentCommonMetadata", "Checkable"]
                .into_iter()
                .map(String::from)
                .collect();
        for version in [CatalogVersion::V0_9, CatalogVersion::V1_0] {
            if let Some(defs) = defs_of(common_types(version)) {
                names.extend(defs.keys().cloned());
            }
        }
        names
    })
}

/// The system functions a common types document defines, keyed by `@` name.
///
/// A system function is a `$defs` entry whose `@call` property is a constant
/// name starting with `@`, such as v1.0's `@index`.
pub(crate) fn system_function_schemas(common: &Value) -> Vec<(String, Value)> {
    let Some(defs) = defs_of(common) else {
        return Vec::new();
    };
    defs.values()
        .filter_map(|schema| {
            let name = schema.pointer("/properties/@call/const")?.as_str()?;
            name.starts_with('@')
                .then(|| (name.to_owned(), schema.clone()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_0_defines_the_index_system_function() {
        let names: Vec<String> = system_function_schemas(common_types(CatalogVersion::V1_0))
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        assert_eq!(names, ["@index"]);
        assert!(system_function_schemas(common_types(CatalogVersion::V0_9)).is_empty());
    }

    #[test]
    fn v0_9_1_reads_the_v0_9_common_types() {
        assert!(std::ptr::eq(
            common_types(CatalogVersion::V0_9),
            common_types(CatalogVersion::V0_9_1)
        ));
    }

    #[test]
    fn standard_names_cover_both_versions() {
        let names = common_def_names();
        for n in [
            "Action",
            "DynamicString",
            "ChildList",
            "CallId",
            "Checkable",
        ] {
            assert!(names.contains(n), "{n}");
        }
        assert!(!names.contains("MyAction"));
    }
}
