//! Compiling Express to A2UI messages.
//!
//! A port of upstream's TypeScript `compiler.ts` (`ExpressCompiler`), with
//! Python's `compiler.py` as a second reference, pinned in
//! `vendor/a2ui/UPSTREAM`. [`Compiler::compile`] reads one block and returns
//! the messages an agent sends for it:
//!
//! - Statements are grouped into *units*: a surface scope (opened by
//!   `surface(...)`, or implicitly by the first assignment), a
//!   `deleteSurface(...)`, or a standalone function call.
//! - A scope whose components include `root` becomes `createSurface` with its
//!   data assignments folded into one data model. A scope with
//!   `update=true`, or without `root`, becomes `updateComponents` and one
//!   `updateDataModel` per assigned path.
//! - A standalone call becomes `callRendererFunction`.
//! - The messages are checked with [`validate`] (a standalone call must
//!   name a function the agent may call, and so on) before they are
//!   returned, as upstream validates what it compiled.
//!
//! ```
//! use a2ui_express::compile::Compiler;
//! use a2ui_express::schema::Catalog;
//! use serde_json::json;
//!
//! let catalog = Catalog::from_json(json!({
//!     "catalogId": "demo",
//!     "protocolVersion": "1.0",
//!     "components": {
//!         "Text": {"properties": {
//!             "component": {"const": "Text"},
//!             "text": {"$ref": "common_types.json#/$defs/DynamicString"}
//!         }, "required": ["component", "text"]},
//!         "Column": {"properties": {
//!             "component": {"const": "Column"},
//!             "children": {"$ref": "common_types.json#/$defs/ChildList"}
//!         }}
//!     }
//! }))
//! .unwrap();
//! let compiler = Compiler::new(vec![catalog], None).unwrap();
//! let messages = compiler
//!     .compile("root = Column([Text($/name)])\n$/name = \"Ada\"", "main")
//!     .unwrap();
//! assert_eq!(
//!     messages,
//!     vec![json!({"version": "1.0", "createSurface": {
//!         "surfaceId": "main",
//!         "catalogId": "demo",
//!         "components": [
//!             {"id": "root", "component": "Column", "children": ["root_children_0"]},
//!             {"id": "root_children_0", "component": "Text", "text": {"@path": "/name"}}
//!         ],
//!         "dataModel": {"name": "Ada"}
//!     }})]
//! );
//! ```

mod data;
mod error;
mod pass;
mod sentinel;
mod validate;
pub(crate) use sentinel::strip_sentinel_tags;
pub(crate) mod session;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde_json::{Map, Value, json};

use crate::ext::{self, Express};

use crate::schema::{Catalog, CatalogError, CatalogSet, CatalogVersion, SymbolKind};
use crate::syntax::{self, Call, Expr, ExprKind, Program, Statement, StatementForm, Target};

pub use error::{Category, CompileError, CompileErrorKind};
pub use validate::{MAX_DEPTH, validate};

use data::{set_nested_path, to_json_pointer};
use pass::{CallArgs, Ctx, Pass};

/// The default surface id of a block without a `surface(...)` line, as the
/// conformance suite uses it.
pub const DEFAULT_SURFACE_ID: &str = "default_surface";

/// Compiles Express blocks against a set of active catalogs.
#[derive(Debug, Clone)]
pub struct Compiler {
    set: CatalogSet,
    version: CatalogVersion,
    /// The `at_exponential_express` extension, when a catalog declares it.
    ext: Option<Arc<Express>>,
}

impl Compiler {
    /// A compiler for `catalogs`, which must have unique ids, emitting
    /// `version` (v1.0, the only one, also when `None`).
    pub fn new(
        catalogs: Vec<Catalog>,
        version: Option<CatalogVersion>,
    ) -> Result<Compiler, CatalogError> {
        let set = CatalogSet::new(catalogs)?;
        let version = version.unwrap_or_else(|| set.version());
        let ext = Express::from_catalogs(&set).map(Arc::new);
        Ok(Compiler { set, version, ext })
    }

    /// The `at_exponential_express` extension, when one of the catalogs
    /// declares it (see [`crate::ext`]).
    pub fn extension(&self) -> Option<&Express> {
        self.ext.as_deref()
    }

    /// The dialect blocks are read in: upstream Express, or the extension's.
    pub fn dialect(&self) -> syntax::Dialect {
        if self.ext.is_some() {
            syntax::Dialect::EXTENDED
        } else {
            syntax::Dialect::UPSTREAM
        }
    }

    /// Compiles `patch` against `screen`, the Express of a surface that
    /// exists: the messages that change it, and the new screen (see
    /// [`crate::ext::patch`]). Needs the `at_exponential_express` extension.
    pub fn compile_patch(
        &self,
        screen: &str,
        patch: &str,
        surface_id: &str,
    ) -> Result<ext::patch::Patch, CompileError> {
        ext::patch::compile_patch(self, screen, patch, surface_id)
    }

    /// Parses a block (without sentinel tags) in [`Compiler::dialect`].
    pub fn parse_block(&self, body: &str) -> syntax::ParseOutput {
        syntax::parse_dialect(body, syntax::Pos::START, &HashSet::new(), self.dialect())
    }

    /// The statements with the extension's sugar expanded (as they are,
    /// without the extension).
    pub(crate) fn desugar<'s>(
        &self,
        statements: &'s [Statement],
    ) -> Result<Desugared<'s>, CompileError> {
        match &self.ext {
            None => Ok((std::borrow::Cow::Borrowed(statements), Arc::default())),
            Some(e) => {
                let (statements, classes) = ext::desugar(statements, e, self)?;
                Ok((std::borrow::Cow::Owned(statements), Arc::new(classes)))
            }
        }
    }

    /// The active catalogs.
    pub fn catalogs(&self) -> &CatalogSet {
        &self.set
    }

    /// The protocol version the messages are emitted for.
    pub fn version(&self) -> CatalogVersion {
        self.version
    }

    /// Compiles one block into messages for surface `surface_id` (the
    /// surface of statements before any `surface(...)` line).
    ///
    /// `<a2ui>` sentinel tags around the block are stripped first. Errors
    /// are those upstream raises, with its categories and wording. The
    /// messages are checked with [`validate`] before they are returned.
    pub fn compile(&self, source: &str, surface_id: &str) -> Result<Vec<Value>, CompileError> {
        let body = sentinel::strip_sentinel_tags(source);
        let program = self.parse_block(&body).into_result()?;
        self.compile_program(&program, surface_id)
    }

    /// Compiles a parsed block.
    pub fn compile_program(
        &self,
        program: &Program,
        surface_id: &str,
    ) -> Result<Vec<Value>, CompileError> {
        let (statements, classes) = self.desugar(&program.statements)?;
        let (scopes, units) = self.build_units(statements.iter(), surface_id)?;
        if units.is_empty() {
            return Err(CompileError::undefined_root("root"));
        }
        let mut ctx = Ctx::new();
        ctx.classes = classes;
        let mut out = Vec::new();
        for unit in units {
            match unit {
                Unit::Delete(surface_id) => out.push(json!({
                    "version": self.version.wire(),
                    "deleteSurface": {"surfaceId": surface_id},
                })),
                Unit::Call(call, scope) => {
                    out.push(self.compile_standalone_call(
                        call,
                        scope.map(|i| &scopes[i]),
                        &mut ctx,
                    )?);
                }
                Unit::Scope(i) => out.extend(self.compile_scope(&scopes[i], &mut ctx)?),
            }
        }
        validate(&self.set, &out)?;
        Ok(out)
    }

    // ---- symbols ----------------------------------------------------------

    /// Whether `name` is a component: in the catalog `explicit` names, when
    /// it names an active one, else in any active catalog.
    pub(crate) fn defines_component(&self, name: &str, explicit: Option<&Expr>) -> bool {
        if let Some(cat) = explicit
            .and_then(string_literal)
            .and_then(|id| self.set.get(id))
        {
            return cat.has_component(name);
        }
        !self
            .set
            .catalogs_defining(SymbolKind::Component, name)
            .is_empty()
    }

    /// Whether `name` is a function. A `@name` always is: a system function
    /// is checked when it is compiled.
    pub(crate) fn defines_function(&self, name: &str, explicit: Option<&Expr>) -> bool {
        if name.starts_with('@') {
            return true;
        }
        if let Some(explicit) = explicit {
            return string_literal(explicit)
                .and_then(|id| self.set.get(id))
                .is_some_and(|c| c.has_function(name));
        }
        !self
            .set
            .catalogs_defining(SymbolKind::Function, name)
            .is_empty()
    }

    /// Resolves a component or function to its catalog.
    pub(crate) fn resolve(
        &self,
        kind: SymbolKind,
        name: &str,
        explicit: Option<&Expr>,
    ) -> Result<crate::schema::Resolved<'_>, CompileError> {
        let explicit = explicit.map(literal_value);
        self.set
            .resolve(kind, name, explicit.as_ref())
            .map_err(error::from_schema_error)
    }

    /// Checks a system function call (`@name`), and returns the catalog
    /// that reads it: the protocol, not a catalog, defines it.
    pub(crate) fn system_function(
        &self,
        name: &str,
        names_catalog: bool,
    ) -> Result<&Catalog, CompileError> {
        let catalog = &self.set.catalogs()[0];
        if !catalog.system_functions().iter().any(|f| f.name() == name) {
            let mut known: Vec<&str> = catalog
                .system_functions()
                .iter()
                .map(|f| f.name())
                .collect();
            known.sort_unstable();
            let known = if known.is_empty() {
                "none".to_owned()
            } else {
                known.join(", ")
            };
            return Err(CompileError::validation(format!(
                "Unknown system function '{name}' in A2UI {}. System functions: {known}.",
                self.version.wire()
            )));
        }
        if names_catalog {
            return Err(CompileError::validation(format!(
                "System function '{name}' is defined by the protocol, not a catalog, so it takes no catalogId."
            )));
        }
        Ok(catalog)
    }

    // ---- units ------------------------------------------------------------

    pub(crate) fn build_units<'a>(
        &self,
        statements: impl IntoIterator<Item = &'a Statement>,
        surface_id: &str,
    ) -> Result<(Vec<Scope<'a>>, Vec<Unit<'a>>), CompileError> {
        let mut scopes: Vec<Scope<'a>> = Vec::new();
        let mut units = Vec::new();
        let mut current: Option<usize> = None;
        for statement in statements {
            match statement.form() {
                StatementForm::Assign(target, value) => {
                    let i = *current.get_or_insert_with(|| {
                        scopes.push(Scope::new(surface_id.to_owned()));
                        units.push(Unit::Scope(scopes.len() - 1));
                        scopes.len() - 1
                    });
                    scopes[i].assign(target, value);
                }
                StatementForm::Surface(call) => {
                    let scope = self.parse_surface_call(call)?;
                    scopes.push(scope);
                    units.push(Unit::Scope(scopes.len() - 1));
                    current = Some(scopes.len() - 1);
                }
                StatementForm::DeleteSurface(call) => {
                    units.push(Unit::Delete(parse_delete_surface_id(call)?));
                }
                StatementForm::Call(call) if self.ext.is_some() && call.name == "theme" => {
                    let i = *current.get_or_insert_with(|| {
                        scopes.push(Scope::new(surface_id.to_owned()));
                        units.push(Unit::Scope(scopes.len() - 1));
                        scopes.len() - 1
                    });
                    scopes[i].theme = Some(ext::theme_value(call)?);
                }
                StatementForm::Call(call) if self.ext.is_some() && call.name == "del" => {
                    return Err(CompileError::validation(
                        "del(id) removes a component in a patch (compile_patch), not in a block.",
                    ));
                }
                StatementForm::Call(call) => units.push(Unit::Call(call, current)),
                StatementForm::Other(_) => {}
            }
        }
        Ok((scopes, units))
    }

    fn parse_surface_call<'a>(&self, call: &'a Call) -> Result<Scope<'a>, CompileError> {
        let args = CallArgs::of(call);
        let v = |m: &str| CompileError::validation(m);
        if args.pos.is_empty() {
            return Err(v(
                "surface() requires at least 1 argument: surface(\"surface_id\").",
            ));
        }
        if args.pos.len() > 2 {
            return Err(v(
                "surface() takes at most 2 positional arguments: surface(id) or surface(id, catalogId).",
            ));
        }
        let Some(surface_id) = string_literal(args.pos[0]) else {
            return Err(v(
                "The first argument to surface() must be a string literal.",
            ));
        };
        let mut catalog_id = String::new();
        if args.pos.len() == 2 {
            if args.get("catalogId").is_some() {
                return Err(v(
                    "Duplicate catalogId in surface(): specified both positionally and as a keyword argument.",
                ));
            }
            let Some(id) = string_literal(args.pos[1]) else {
                return Err(v(
                    "The second argument to surface() must be a catalog ID string.",
                ));
            };
            catalog_id = id.to_owned();
        }

        let mut scope = Scope::new(surface_id.to_owned());
        scope.directive = true;
        for &(key, value) in &args.kw {
            match key {
                "catalogId" => {
                    let Some(id) = string_literal(value) else {
                        return Err(v("The catalogId argument to surface() must be a string."));
                    };
                    catalog_id = id.to_owned();
                }
                "sendDataModel" => {
                    let ExprKind::Bool(b) = value.kind else {
                        return Err(v(
                            "The sendDataModel argument to surface() must be a boolean.",
                        ));
                    };
                    scope.send_data_model = Some(b);
                }
                "update" => {
                    let ExprKind::Bool(b) = value.kind else {
                        return Err(v("The update argument to surface() must be a boolean."));
                    };
                    scope.update = b;
                }
                "extensions" => {
                    scope.raw_extensions = Some(value);
                }
                other => {
                    return Err(CompileError::validation(format!(
                        "Unknown keyword argument '{other}' in surface()."
                    )));
                }
            }
        }
        if scope.update && scope.raw_extensions.is_some() {
            return Err(v("surface() with update=true does not accept extensions."));
        }
        if !catalog_id.is_empty() {
            if self.set.catalogs().len() > 1 {
                return Err(CompileError::validation(format!(
                    "surface() names catalog '{catalog_id}', but a surface names no catalog when several catalogs are active. Component and function names resolve across the active catalogs, or take catalogId=\"...\" when shared."
                )));
            }
            if catalog_id != self.set.catalogs()[0].id() {
                return Err(error::from_schema_error(self.set.unknown_catalog_error(
                    &format!("Surface '{surface_id}'"),
                    &catalog_id,
                )));
            }
        }
        Ok(scope)
    }

    // ---- scopes -----------------------------------------------------------

    /// The id of every variable holding a component: its `id=`, else its name.
    pub(crate) fn collect_component_ids(&self, scope: &Scope<'_>) -> Result<CompIds, CompileError> {
        let mut ids = CompIds::default();
        let mut seen = HashSet::new();
        for (var, ast) in scope.syms.iter() {
            let ExprKind::Call(call) = &ast.kind else {
                continue;
            };
            let args = CallArgs::of(call);
            let explicit_id = self.explicit_id(&call.name, args.get("id"))?;
            if self.defines_component(&call.name, args.get("catalogId")) {
                let id = explicit_id.unwrap_or(var).to_owned();
                if !seen.insert(id.clone()) {
                    return Err(CompileError::id_collision(&id));
                }
                ids.insert(var.to_owned(), id);
            }
        }
        Ok(ids)
    }

    /// Checks an `id=` argument: a non-empty string and a UAX #31
    /// identifier.
    pub(crate) fn explicit_id<'e>(
        &self,
        component: &str,
        id: Option<&'e Expr>,
    ) -> Result<Option<&'e str>, CompileError> {
        let Some(id) = id else {
            return Ok(None);
        };
        let Some(id) = string_literal(id).filter(|s| !s.is_empty()) else {
            return Err(CompileError::validation(format!(
                "id on component '{component}' must be a non-empty string."
            )));
        };
        if !is_uax31_identifier(id) {
            return Err(CompileError::validation(format!(
                "Component id '{id}' on '{component}' is not a valid UAX 31 identifier."
            )));
        }
        Ok(Some(id))
    }

    fn compile_scope(&self, scope: &Scope<'_>, ctx: &mut Ctx) -> Result<Vec<Value>, CompileError> {
        ctx.generated_ids.clear();
        let ids = self.collect_component_ids(scope)?;
        let mut pass = Pass::new(self, &scope.syms, &ids, ctx);

        let mut components = Vec::new();
        for (var, ast) in scope.syms.iter() {
            if let Some(component) = pass.ast_node(var, ast)? {
                components.push(Value::Object(component));
            }
        }
        components.extend(
            std::mem::take(&mut pass.ctx.extra_components)
                .into_iter()
                .map(Value::Object),
        );
        let has_root = components.iter().any(|c| c["id"] == "root");

        if scope.update || !has_root {
            if scope.raw_extensions.is_some() {
                return Err(CompileError::validation(
                    "surface() extensions require a surface-creating scope with a root component.",
                ));
            }
            let mut out = Vec::new();
            if let Some(theme) = &scope.theme {
                out.push(self.theme_call(&scope.surface_id, theme, pass.ctx)?);
                if components.is_empty() && scope.data.is_empty() {
                    return Ok(out);
                }
            }
            out.extend(self.compile_update_scope(scope, components, &mut pass)?);
            return Ok(out);
        }

        let mut data_model = Map::new();
        for (path, ast) in &scope.data {
            let value = pass.plain_value(ast)?;
            set_nested_path(&mut data_model, path, value)?;
        }
        let extensions = match scope.raw_extensions {
            Some(raw) => {
                Some(pass.extensions_map(raw, &format!("surface '{}'", scope.surface_id))?)
            }
            None => None,
        };
        let metadata = self.surface_metadata(scope, extensions)?;
        Ok(self.create_messages(scope, components, data_model, metadata))
    }

    /// A new surface's `metadata`: its `extensions=` and its theme.
    pub(crate) fn surface_metadata(
        &self,
        scope: &Scope<'_>,
        mut extensions: Option<Map<String, Value>>,
    ) -> Result<Option<Value>, CompileError> {
        if let Some(theme) = &scope.theme {
            let ext = extensions.get_or_insert_with(Map::new);
            if ext.contains_key(ext::THEME_EXTENSION) {
                return Err(CompileError::validation(format!(
                    "Surface '{}' sets its theme twice: by theme(...) and in extensions.{}.",
                    scope.surface_id,
                    ext::THEME_EXTENSION
                )));
            }
            ext.insert(ext::THEME_EXTENSION.into(), theme.clone());
        }
        Ok(extensions.map(|e| json!({ "extensions": e })))
    }

    /// The `callRendererFunction` that changes a live surface's theme.
    pub(crate) fn theme_call(
        &self,
        surface_id: &str,
        theme: &Value,
        ctx: &mut Ctx,
    ) -> Result<Value, CompileError> {
        let ext = self.ext.as_deref().expect("theme() needs the extension");
        let function = &ext.theme_function;
        if !self.defines_function(function, None) {
            return Err(CompileError::validation(format!(
                "theme(...) on surface '{surface_id}', which already exists, compiles to a call of the renderer function '{function}', which no active catalog declares. Write theme(...) before root, so it goes into createSurface."
            )));
        }
        ctx.call_counter += 1;
        let catalog_id = self
            .set
            .catalogs_defining(SymbolKind::Function, function)
            .first()
            .map(|c| (*c).to_owned())
            .unwrap_or_else(|| ext.catalog_id.clone());
        Ok(json!({
            "version": self.version.wire(),
            "callRendererFunction": {
                "surfaceId": surface_id,
                "functionCallId": format!("call_{}", ctx.call_counter),
                "callFunction": {
                    "catalogId": catalog_id,
                    "@call": function,
                    "args": {"theme": theme},
                },
            },
        }))
    }

    fn compile_update_scope(
        &self,
        scope: &Scope<'_>,
        components: Vec<Value>,
        pass: &mut Pass<'_, '_, '_>,
    ) -> Result<Vec<Value>, CompileError> {
        let version = self.version.wire();
        let mut data_updates = Vec::new();
        for (path, ast) in &scope.data {
            let pointer = to_json_pointer(path);
            let value = pass.plain_value(ast)?;
            if pointer == "/" && !value.is_object() {
                return Err(CompileError::validation(
                    "Cannot assign a non-map value to the root data path '$/'.",
                ));
            }
            data_updates.push(json!({
                "version": version,
                "updateDataModel": {
                    "surfaceId": scope.surface_id,
                    "path": pointer,
                    "value": value,
                },
            }));
        }
        if components.is_empty() {
            if data_updates.is_empty() {
                return Err(CompileError::undefined_root("root"));
            }
            return Ok(data_updates);
        }
        let mut out = vec![json!({
            "version": version,
            "updateComponents": {
                "surfaceId": scope.surface_id,
                "components": components,
            },
        })];
        out.extend(data_updates);
        Ok(out)
    }

    pub(crate) fn create_messages(
        &self,
        scope: &Scope<'_>,
        components: Vec<Value>,
        data_model: Map<String, Value>,
        metadata: Option<Value>,
    ) -> Vec<Value> {
        let version = self.version.wire();
        let mut create = Map::new();
        create.insert("surfaceId".into(), scope.surface_id.clone().into());
        if let Some(id) = self.set.surface_catalog_id() {
            create.insert("catalogId".into(), id.into());
        }
        if let Some(send) = scope.send_data_model {
            create.insert("sendDataModel".into(), send.into());
        }
        create.insert("components".into(), components.into());
        if !data_model.is_empty() {
            create.insert("dataModel".into(), data_model.into());
        }
        if let Some(metadata) = metadata {
            create.insert("metadata".into(), metadata);
        }
        vec![json!({"version": version, "createSurface": create})]
    }

    pub(crate) fn compile_standalone_call(
        &self,
        call: &Call,
        scope: Option<&Scope<'_>>,
        ctx: &mut Ctx,
    ) -> Result<Value, CompileError> {
        ctx.call_counter += 1;
        let empty = Symbols::default();
        let ids = CompIds::default();
        let syms = scope.map_or(&empty, |s| &s.syms);
        let mut pass = Pass::new(self, syms, &ids, ctx);
        let args = CallArgs::of(call);
        let (compiled, call_id) =
            pass.function_call(&call.name, &args.pos, args.kw, false, true)?;
        let mut call_function = Map::new();
        call_function.insert(
            "catalogId".into(),
            self.set.catalogs()[0].id().to_owned().into(),
        );
        call_function.extend(compiled);
        let call_id = call_id.unwrap_or_else(|| format!("call_{}", pass.ctx.call_counter));
        Ok(json!({
            "version": self.version.wire(),
            "callRendererFunction": {
                "functionCallId": call_id,
                "callFunction": call_function,
            },
        }))
    }
}

// ---- scopes and units -------------------------------------------------------

/// A scope's variables, in the order first assigned; a reassignment keeps
/// the first position and the last value, as upstream's object does.
#[derive(Debug, Default)]
pub(crate) struct Symbols<'a> {
    order: Vec<&'a str>,
    values: HashMap<&'a str, &'a Expr>,
}

impl<'a> Symbols<'a> {
    fn insert(&mut self, name: &'a str, value: &'a Expr) {
        if self.values.insert(name, value).is_none() {
            self.order.push(name);
        }
    }

    pub(crate) fn get(&self, name: &str) -> Option<&'a Expr> {
        self.values.get(name).copied()
    }

    pub(crate) fn contains(&self, name: &str) -> bool {
        self.values.contains_key(name)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&'a str, &'a Expr)> + '_ {
        self.order.iter().map(|n| (*n, self.values[n]))
    }
}

/// Variable name to component id.
#[derive(Debug, Default)]
pub(crate) struct CompIds {
    by_var: HashMap<String, String>,
    ids: HashSet<String>,
}

impl CompIds {
    fn insert(&mut self, var: String, id: String) {
        self.ids.insert(id.clone());
        self.by_var.insert(var, id);
    }

    pub(crate) fn get(&self, var: &str) -> Option<&str> {
        self.by_var.get(var).map(String::as_str)
    }

    pub(crate) fn has_id(&self, id: &str) -> bool {
        self.ids.contains(id)
    }
}

#[derive(Debug)]
pub(crate) struct Scope<'a> {
    pub(crate) surface_id: String,
    /// Opened by a `surface(...)` line, not implicitly.
    pub(crate) directive: bool,
    pub(crate) send_data_model: Option<bool>,
    pub(crate) update: bool,
    pub(crate) raw_extensions: Option<&'a Expr>,
    /// The `theme(...)` of the scope (extension), expanded.
    pub(crate) theme: Option<Value>,
    pub(crate) syms: Symbols<'a>,
    /// Data path assignments; a reassignment moves the path to the end.
    pub(crate) data: Vec<(String, &'a Expr)>,
}

impl<'a> Scope<'a> {
    fn new(surface_id: String) -> Self {
        Scope {
            surface_id,
            directive: false,
            send_data_model: None,
            update: false,
            raw_extensions: None,
            theme: None,
            syms: Symbols::default(),
            data: Vec::new(),
        }
    }

    fn assign(&mut self, target: &'a Target, value: &'a Expr) {
        match target {
            Target::Variable { name, .. } => self.syms.insert(name, value),
            Target::Path { .. } => {
                let key = target.text();
                self.data.retain(|(k, _)| *k != key);
                self.data.push((key, value));
            }
        }
    }
}

pub(crate) enum Unit<'a> {
    Scope(usize),
    Delete(String),
    /// A standalone call, with the scope it was written in.
    Call(&'a Call, Option<usize>),
}

fn parse_delete_surface_id(call: &Call) -> Result<String, CompileError> {
    let args = CallArgs::of(call);
    if let Some(id) = args.pos.first().and_then(|e| string_literal(e)) {
        return Ok(id.to_owned());
    }
    if let Some(id) = args.get("surfaceId").and_then(string_literal) {
        return Ok(id.to_owned());
    }
    Err(CompileError::validation(
        "deleteSurface() requires the id of the surface to delete.",
    ))
}

// ---- helpers ----------------------------------------------------------------

/// A block's statements with the extension's sugar expanded, and its style
/// classes.
pub(crate) type Desugared<'s> = (
    std::borrow::Cow<'s, [Statement]>,
    Arc<HashMap<String, String>>,
);

/// A fresh compile context (for the extension's messages).
pub(crate) fn pass_ctx() -> Ctx {
    Ctx::new()
}

/// The text of a string literal.
pub(crate) fn string_literal(e: &Expr) -> Option<&str> {
    match &e.kind {
        ExprKind::String(s) => Some(s),
        _ => None,
    }
}

/// A literal as JSON; anything else as `null`, which no check accepts where
/// a string is wanted.
fn literal_value(e: &Expr) -> Value {
    match &e.kind {
        ExprKind::String(s) => Value::String(s.clone()),
        ExprKind::Bool(b) => Value::Bool(*b),
        ExprKind::Number(n) => Value::Number(n.to_json()),
        _ => Value::Null,
    }
}

pub(crate) use syntax::uax31::is_uax31_identifier;

impl ext::Components for Compiler {
    fn is_component(&self, name: &str) -> bool {
        self.defines_component(name, None)
    }

    fn declares(&self, name: &str, prop: &str) -> bool {
        self.set.catalogs().iter().any(|c| {
            c.has_component(name) && c.component_properties(name).iter().any(|p| p == prop)
        })
    }
}
