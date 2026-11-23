//! Catalog loading and the schema helper.
//!
//! Everything the compiler, decompiler and prompt generator ask about a
//! catalog: the positional order of each component's properties, which are
//! required, static (admit no `$path`), enums, child ids, child lists,
//! actions and option lists, where a `$path` is allowed inside a value,
//! check rules, function signatures and callers, and lookup across several
//! catalogs.
//!
//! Ported from upstream's TypeScript `schema-helper.ts` and `catalogs.ts`
//! (a2ui-project/a2ui#4184, pinned in `vendor/a2ui/UPSTREAM`).
//!
//! ```
//! use a2ui_express::schema::{Catalog, Segment};
//! use serde_json::json;
//!
//! let catalog = Catalog::from_json(json!({
//!     "catalogId": "demo",
//!     "protocolVersion": "1.0",
//!     "components": {
//!         "Menu": {
//!             "properties": {
//!                 "component": {"const": "Menu"},
//!                 "items": {"type": "array", "items": {"$ref": "#/$defs/item"}}
//!             }
//!         }
//!     },
//!     "$defs": {
//!         "item": {
//!             "type": "object",
//!             "properties": {
//!                 "label": {"type": "string"},
//!                 "checked": {"$ref": "common_types.json#/$defs/DynamicBoolean"}
//!             }
//!         }
//!     }
//! }))
//! .unwrap();
//!
//! let at = |key| [Segment::Item, Segment::Property(key)];
//! assert_eq!(catalog.admits_path_at("Menu", "items", &at("checked")), Some(true));
//! assert_eq!(catalog.admits_path_at("Menu", "items", &at("label")), Some(false));
//! ```

mod catalog;
mod catalogs;
mod common;
mod refs;
mod resolve;
mod version;

use thiserror::Error;

pub use catalog::{AllowedCallers, Catalog, Component, Function, LoadOptions};
pub use catalogs::{CatalogSet, Resolved, SymbolKind};
pub use common::common_types;
pub use refs::{
    ENVELOPE_MIXIN_NAMES, common_def_name, envelope_key, envelope_read_key, envelope_value,
};
pub(crate) use resolve::is_object_or_ref_schema;
pub use resolve::{Segment, SlotKind};
pub use version::CatalogVersion;

/// A catalog document that cannot be loaded, or catalogs that cannot be
/// used together.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CatalogError {
    /// The text is not JSON.
    #[error("catalog is not valid JSON: {0}")]
    InvalidJson(String),
    /// The document is not a JSON object.
    #[error("catalog document must be a JSON object")]
    NotAnObject,
    /// No `catalogId`, `$id` or `id`.
    #[error("Catalog ID must be specified via catalog metadata ('catalogId' or '$id').")]
    MissingId,
    /// A `protocolVersion` this crate does not read.
    #[error("Unsupported catalog protocol version {0}; expected 0.9, 0.9.1 or 1.0.")]
    UnsupportedVersion(String),
    /// A v1.0 `$ref` to another document.
    #[error(
        "Unsupported external $ref '{reference}' at {pointer}: a v1.0 catalog may only reference its own components and functions (#/...) or common_types.json."
    )]
    ExternalRef {
        /// The reference.
        reference: String,
        /// Where it is, as a JSON Pointer.
        pointer: String,
    },
    /// `requiresUserActivation: true` with callers other than `rendererOnly`.
    #[error(
        "Function '{function}' declares requiresUserActivation=true with allowedCallers='{callers}'; requiresUserActivation=true requires allowedCallers='rendererOnly'."
    )]
    UserActivationCallers {
        /// The function.
        function: String,
        /// Its declared callers.
        callers: String,
    },
    /// A v1.0 component, property, function or argument name that is not a
    /// UAX #31 identifier.
    #[error("Invalid UAX #31 {kind} identifier: '{name}'{}", in_owner(.owner))]
    InvalidIdentifier {
        /// What is named: `component`, `property`, `function` or `argument`.
        kind: &'static str,
        /// The name.
        name: String,
        /// The component of a property or the function of an argument, as
        /// `component 'Name'` or `function 'name'`.
        owner: Option<String>,
    },
    /// A v1.0 `metadata` that is not `{"extensions": {<identifier>: …}}`.
    #[error("{reason}")]
    InvalidMetadata {
        /// Whose metadata it is: `catalog`, `component 'Name'` or
        /// `function 'name'`.
        owner: String,
        /// What is wrong.
        reason: String,
    },
    /// A function definition with an invalid field.
    #[error("Function '{function}': {reason}")]
    InvalidFunction {
        /// The function.
        function: String,
        /// What is wrong.
        reason: String,
    },
    /// No catalogs given.
    #[error("Express requires at least one catalog.")]
    NoCatalogs,
    /// Two catalogs with one ID.
    #[error("Duplicate catalog ID: '{0}'")]
    DuplicateId(String),
    /// Catalogs on different release lines.
    #[error(
        "Express catalogs must share one protocol version, but '{first}' is '{first_version}' and '{other}' is '{other_version}'."
    )]
    MixedVersions {
        /// The first catalog.
        first: String,
        /// Its version.
        first_version: String,
        /// The catalog that differs.
        other: String,
        /// Its version.
        other_version: String,
    },
    /// Several catalogs below v1.0.
    #[error(
        "Several catalogs need A2UI v1.0 or later, where components and function calls carry their own catalogId, but got '{0}'."
    )]
    SeveralBelowV1(String),
    /// A requested version off the catalogs' release line.
    #[error("Requested protocol version '{requested}' does not match catalog version '{catalogs}'")]
    RequestedVersionMismatch {
        /// The requested version.
        requested: String,
        /// The catalogs' version.
        catalogs: String,
    },
}

fn in_owner(owner: &Option<String>) -> String {
    owner
        .as_ref()
        .map_or_else(String::new, |owner| format!(" in {owner}"))
}
