//! Several active catalogs: lookup by name, and the `catalogId` override.

use serde_json::{Map, Value};

use super::{Catalog, CatalogError, CatalogVersion, ENVELOPE_MIXIN_NAMES};
use crate::Error;

/// Whether a name is looked up among components or functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    /// A component.
    Component,
    /// A function.
    Function,
}

impl SymbolKind {
    fn word(self) -> &'static str {
        match self {
            SymbolKind::Component => "component",
            SymbolKind::Function => "function",
        }
    }

    fn label(self) -> &'static str {
        match self {
            SymbolKind::Component => "Component",
            SymbolKind::Function => "Function",
        }
    }
}

/// The catalog a name resolved to.
#[derive(Debug, Clone, Copy)]
pub struct Resolved<'c> {
    /// The catalog that defines the name.
    pub catalog: &'c Catalog,
    /// The `catalogId` to write on the component or call: set when it was
    /// named explicitly, or when several catalogs are active.
    pub catalog_id: Option<&'c str>,
}

/// The active catalogs of an Express block, in priority order.
///
/// IDs are unique; components and function calls carry their own
/// `catalogId` when more than one catalog is active.
#[derive(Debug, Clone)]
pub struct CatalogSet {
    catalogs: Vec<Catalog>,
}

impl CatalogSet {
    /// Checks and wraps the active catalogs.
    pub fn new(catalogs: Vec<Catalog>) -> Result<CatalogSet, CatalogError> {
        if catalogs.is_empty() {
            return Err(CatalogError::NoCatalogs);
        }
        let mut seen = std::collections::HashSet::new();
        for catalog in &catalogs {
            if !seen.insert(catalog.id()) {
                return Err(CatalogError::DuplicateId(catalog.id().to_owned()));
            }
        }
        Ok(CatalogSet { catalogs })
    }

    /// The catalogs, in priority order.
    pub fn catalogs(&self) -> &[Catalog] {
        &self.catalogs
    }

    /// The catalogs' version, which messages are emitted for.
    pub fn version(&self) -> CatalogVersion {
        self.catalogs[0].version()
    }

    /// A catalog by ID.
    pub fn get(&self, id: &str) -> Option<&Catalog> {
        self.catalogs.iter().find(|c| c.id() == id)
    }

    /// The `catalogId` to put on `createSurface`: the catalog's when only one
    /// is active, none when several are.
    pub fn surface_catalog_id(&self) -> Option<&str> {
        match self.catalogs.as_slice() {
            [only] => Some(only.id()),
            _ => None,
        }
    }

    /// Refuses a component that carries an envelope mixin under its bare
    /// pre-1.0 key (`checks` for `@checks`), unless its schema declares a
    /// property of that name of its own.
    ///
    /// The schema is that of the catalog the component names (`@catalogId`,
    /// else `surface_catalog_id`); without either, any active catalog that
    /// defines the component and declares the property allows it.
    pub(crate) fn refuse_legacy_envelope(
        &self,
        component: &Map<String, Value>,
        surface_catalog_id: Option<&str>,
    ) -> Result<(), Error> {
        let Some(name) = component.get("component").and_then(Value::as_str) else {
            return Ok(());
        };
        let named = component
            .get("@catalogId")
            .and_then(Value::as_str)
            .or(surface_catalog_id);
        for mixin in ENVELOPE_MIXIN_NAMES {
            if !component.contains_key(mixin) {
                continue;
            }
            let declared = self
                .catalogs
                .iter()
                .filter(|c| named.is_none_or(|id| c.id() == id))
                .filter_map(|c| c.component(name))
                .any(|c| c.custom_envelope_properties().iter().any(|p| p == mixin));
            if !declared {
                return Err(Error::Validation(format!(
                    "Component '{name}' carries '{mixin}', which its schema does not declare; the envelope key is '@{mixin}'."
                )));
            }
        }
        Ok(())
    }

    /// The IDs, in catalog order, of the catalogs that define `name`.
    pub fn catalogs_defining(&self, kind: SymbolKind, name: &str) -> Vec<&str> {
        self.catalogs
            .iter()
            .filter(|c| match kind {
                SymbolKind::Component => c.has_component(name),
                SymbolKind::Function => c.has_function(name),
            })
            .map(Catalog::id)
            .collect()
    }

    /// Resolves a component or function name to the catalog that defines it.
    ///
    /// `explicit_catalog_id` is the `catalogId` argument the Express call
    /// gave, if any. Without one, the name must be defined in
    /// exactly one active catalog.
    pub fn resolve(
        &self,
        kind: SymbolKind,
        name: &str,
        explicit_catalog_id: Option<&Value>,
    ) -> Result<Resolved<'_>, Error> {
        let label = kind.label();
        if let Some(explicit) = explicit_catalog_id {
            let Some(cid) = explicit.as_str().filter(|s| !s.is_empty()) else {
                return Err(Error::Validation(format!(
                    "catalogId on {} '{name}' must be a non-empty string.",
                    kind.word()
                )));
            };
            let Some(catalog) = self.get(cid) else {
                return Err(self.unknown_catalog_error(&format!("{label} '{name}'"), cid));
            };
            let defines = match kind {
                SymbolKind::Component => catalog.has_component(name),
                SymbolKind::Function => catalog.has_function(name),
            };
            if !defines {
                return Err(self.unknown_symbol_error(kind, name, Some(cid)));
            }
            return Ok(Resolved {
                catalog,
                catalog_id: Some(catalog.id()),
            });
        }

        let matches = self.catalogs_defining(kind, name);
        match matches.as_slice() {
            [] => Err(self.unknown_symbol_error(kind, name, None)),
            [cid] => {
                let catalog = self.get(cid).expect("defining catalog is active");
                Ok(Resolved {
                    catalog,
                    catalog_id: (self.catalogs.len() > 1).then(|| catalog.id()),
                })
            }
            several => Err(Error::Validation(format!(
                "{label} '{name}' is defined in several catalogs: {}. Name the catalog to use with a catalogId argument.",
                quoted(several)
            ))),
        }
    }

    /// The error for a surface, component or function that targets a
    /// catalog that is not active.
    pub fn unknown_catalog_error(&self, target: &str, catalog_id: &str) -> Error {
        let active: Vec<&str> = self.catalogs.iter().map(Catalog::id).collect();
        Error::Validation(format!(
            "{target} targets unknown catalog '{catalog_id}'. Active catalogs: {}.",
            quoted(&active)
        ))
    }

    /// The error for a name a target catalog (or every active one) lacks.
    pub fn unknown_symbol_error(
        &self,
        kind: SymbolKind,
        name: &str,
        catalog_id: Option<&str>,
    ) -> Error {
        let defining = self.catalogs_defining(kind, name);
        if defining.is_empty() {
            return Error::Validation(format!(
                "{} '{name}' is not defined in the catalog.",
                kind.label()
            ));
        }
        Error::Validation(format!(
            "{} '{name}' is not in catalog '{}'. It is defined in: {}.",
            kind.label(),
            catalog_id.unwrap_or_default(),
            quoted(&defining)
        ))
    }

    /// The names of the functions the agent may call directly, across the
    /// catalogs in order, each once.
    pub fn agent_callable_functions(&self) -> Vec<&str> {
        let mut names: Vec<&str> = Vec::new();
        for name in self
            .catalogs
            .iter()
            .flat_map(Catalog::agent_callable_functions)
        {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        names
    }
}

fn quoted(ids: &[&str]) -> String {
    ids.iter()
        .map(|id| format!("'{id}'"))
        .collect::<Vec<_>>()
        .join(", ")
}
