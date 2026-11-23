//! Decompiler: A2UI messages back into Express notation.
//!
//! The inverse of compiling. It exists so a prompt example or a training
//! sample can be authored as A2UI JSON and shown to a model in the notation
//! the model is asked to produce. Ported from upstream's TypeScript
//! `decompiler.ts` (pinned in `vendor/a2ui/UPSTREAM`); the behaviour is fixed
//! by `vendor/a2ui/conformance/agent/express/decompiler.yaml`.
//!
//! - `createSurface` / `updateComponents` / `updateDataModel` /
//!   `deleteSurface` / `callRendererFunction` become `surface(...)` lines and
//!   statements, after split messages of one surface are coalesced.
//! - Variable names come from component ids, mangled per UAX #31 with
//!   collision suffixes ([`assign_variable_names`]).
//! - Arguments are positional by default ([`DecompileOptions::keyword_args`]
//!   writes `name=value`); trailing `_` placeholders are dropped and a check
//!   value that equals the component's own bound value is left implicit.
//! - [`Order::TopDown`] writes `root` first and then breadth-first, which is
//!   what training data and prompt examples should use so streaming renders
//!   early; [`Order::Upstream`] keeps the messages' component order.
//!
//! ```
//! use a2ui_express::decompile::{decompile, DecompileOptions, Order};
//! use a2ui_express::schema::{Catalog, CatalogSet};
//! use serde_json::json;
//!
//! let catalog = Catalog::from_json(json!({
//!     "catalogId": "demo",
//!     "protocolVersion": "1.0",
//!     "components": {
//!         "Card": {"properties": {
//!             "component": {"const": "Card"},
//!             "child": {"$ref": "common_types.json#/$defs/ComponentId"}
//!         }},
//!         "Text": {"properties": {
//!             "component": {"const": "Text"},
//!             "text": {"$ref": "common_types.json#/$defs/DynamicString"}
//!         }}
//!     }
//! }))
//! .unwrap();
//! let catalogs = CatalogSet::new(vec![catalog]).unwrap();
//! let messages = [json!({"version": "1.0", "createSurface": {
//!     "surfaceId": "s1", "catalogId": "demo",
//!     "components": [
//!         {"id": "body", "component": "Text", "text": "Hello"},
//!         {"id": "root", "component": "Card", "child": "body"}
//!     ]
//! }})];
//! let options = DecompileOptions { order: Order::TopDown, ..Default::default() };
//! assert_eq!(
//!     decompile(&catalogs, &messages, &options).unwrap(),
//!     "surface(\"s1\")\nroot = Card(body)\nbody = Text(\"Hello\")"
//! );
//! ```

mod literal;
mod order;

use std::collections::HashMap;

use serde_json::{Map, Value};

use crate::schema::{Catalog, CatalogSet, SymbolKind, envelope_value};
use crate::{Error, Result};

pub use literal::{assign_variable_names, derive_variable_name, is_uax31_identifier};
pub use order::Order;

/// How messages are written out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct DecompileOptions {
    /// Write component arguments as `name=value` instead of positionally.
    pub keyword_args: bool,
    /// The order a block's components are written in.
    pub order: Order,
}

/// Writes A2UI messages as one Express block (without the `<a2ui>` tags).
///
/// The messages must be of the catalogs' protocol version. A component id
/// that is not a UAX #31 identifier is a [`Error::Validation`] from v1.0; so
/// is a component, function or catalog the active catalogs do not define.
pub fn decompile(
    catalogs: &CatalogSet,
    messages: &[Value],
    options: &DecompileOptions,
) -> Result<String> {
    Decompiler::new(catalogs, *options).decompile(messages)
}

type Obj = Map<String, Value>;
type VarNames = HashMap<String, String>;

fn validation(message: impl Into<String>) -> Error {
    Error::Validation(message.into())
}

/// A non-empty string field, or `default_surface`.
fn surface_id_of(op: &Obj) -> String {
    op.get("surfaceId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("default_surface")
        .to_owned()
}

/// JS `??`: a missing or `null` field.
fn present(value: Option<&Value>) -> Option<&Value> {
    value.filter(|v| !v.is_null())
}

/// Reads an optional object-valued field such as an event's `context`.
fn optional_object(value: Option<&Value>, owner: &str, field: &str) -> Result<Obj> {
    match present(value) {
        None => Ok(Obj::new()),
        Some(Value::Object(map)) => Ok(map.clone()),
        Some(_) => Err(validation(format!("{owner} has a non-object '{field}'"))),
    }
}

/// Whether a data model value has anything to write: only an empty object
/// has nothing.
fn has_data_to_write(value: &Value) -> bool {
    !matches!(value, Value::Object(map) if map.is_empty())
}

/// Flattens nested maps into `(pointer, leaf)` pairs; a non-map is one
/// entry at the root path `""`.
fn flatten_data_model<'v>(value: &'v Value, path: String, out: &mut Vec<(String, &'v Value)>) {
    match value {
        Value::Object(map) if !map.is_empty() => {
            for (k, v) in map {
                flatten_data_model(v, format!("{path}/{k}"), out);
            }
        }
        _ => out.push((path, value)),
    }
}

/// Removes trailing `_` placeholders.
fn trim_placeholders(args: &mut Vec<String>) {
    while args.last().is_some_and(|a| a == "_") {
        args.pop();
    }
}

/// Writes a reference to a component by id: its variable name when the
/// block defines it, a bare name when the id is an identifier no variable of
/// the block uses, a quoted id otherwise.
fn reference(component_id: &str, var_names: &VarNames) -> String {
    if let Some(name) = var_names.get(component_id) {
        return name.clone();
    }
    if crate::syntax::is_identifier(component_id) && !var_names.values().any(|v| v == component_id)
    {
        return component_id.to_owned();
    }
    literal::string(component_id)
}

struct Decompiler<'c> {
    catalogs: &'c CatalogSet,
    sole_catalog_id: Option<&'c str>,
    is_v1: bool,
    options: DecompileOptions,
}

impl<'c> Decompiler<'c> {
    fn new(catalogs: &'c CatalogSet, options: DecompileOptions) -> Self {
        Decompiler {
            catalogs,
            sole_catalog_id: catalogs.surface_catalog_id(),
            is_v1: catalogs.version().at_least_v1(),
            options,
        }
    }

    fn first(&self) -> &'c Catalog {
        &self.catalogs.catalogs()[0]
    }

    // ---- messages --------------------------------------------------------

    /// Folds `updateComponents` and a root `updateDataModel` into the open
    /// `createSurface` of their surface, keeping incremental updates apart.
    fn coalesce(&self, messages: &[Value]) -> Vec<Obj> {
        let mut result: Vec<Obj> = Vec::new();
        let mut open: HashMap<String, usize> = HashMap::new();
        for raw in messages {
            let Some(msg) = raw.as_object().filter(|m| !m.is_empty()) else {
                continue;
            };
            if let Some(cs) = msg.get("createSurface").and_then(Value::as_object) {
                open.insert(surface_id_of(cs), result.len());
                result.push(msg.clone());
                continue;
            }
            if let Some(ds) = msg.get("deleteSurface").and_then(Value::as_object) {
                open.remove(&surface_id_of(ds));
                result.push(msg.clone());
                continue;
            }
            if msg
                .get("callRendererFunction")
                .is_some_and(Value::is_object)
                || msg.get("callFunction").is_some_and(Value::is_object)
            {
                open.clear();
                result.push(msg.clone());
                continue;
            }
            if let Some(uc) = msg.get("updateComponents").and_then(Value::as_object) {
                let sid = surface_id_of(uc);
                if let Some(&at) = open.get(&sid) {
                    let cs = open_create(&mut result[at]);
                    if present(cs.get("components")).is_none() {
                        cs.insert(
                            "components".into(),
                            uc.get("components").cloned().unwrap_or(Value::Null),
                        );
                        continue;
                    }
                    open.remove(&sid);
                }
                result.push(msg.clone());
                continue;
            }
            if let Some(udm) = msg.get("updateDataModel").and_then(Value::as_object) {
                let sid = surface_id_of(udm);
                if let Some(&at) = open.get(&sid) {
                    let is_root = match present(udm.get("path")) {
                        None => true,
                        Some(Value::String(p)) => p.is_empty() || p == "/",
                        Some(_) => false,
                    };
                    if let (true, Some(value)) =
                        (is_root, udm.get("value").filter(|v| v.is_object()))
                    {
                        open_create(&mut result[at]).insert("dataModel".into(), value.clone());
                        continue;
                    }
                    open.remove(&sid);
                }
                result.push(msg.clone());
                continue;
            }
            result.push(msg.clone());
        }
        result
    }

    fn decompile(&self, messages: &[Value]) -> Result<String> {
        let coalesced = self.coalesce(messages);
        let mut surface_catalogs: HashMap<String, String> = HashMap::new();
        let mut blocks: Vec<String> = Vec::new();
        let mut call_index = 0usize;

        for envelope in &coalesced {
            let is_first = blocks.is_empty();
            if let Some(ds) = envelope.get("deleteSurface").and_then(Value::as_object) {
                let sid = ds.get("surfaceId").and_then(Value::as_str).unwrap_or("");
                surface_catalogs.remove(if sid.is_empty() {
                    "default_surface"
                } else {
                    sid
                });
                blocks.push(format!("deleteSurface({})", literal::string(sid)));
                continue;
            }

            let crf = envelope
                .get("callRendererFunction")
                .and_then(Value::as_object);
            let func_op = match crf {
                Some(crf) => crf.get("callFunction").and_then(Value::as_object),
                None => envelope.get("callFunction").and_then(Value::as_object),
            };
            if let Some(func_op) = func_op {
                call_index += 1;
                let call_id = crf
                    .and_then(|c| c.get("functionCallId"))
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty() && *s != format!("call_{call_index}"));
                let extra: Vec<String> = call_id
                    .map(|id| format!("functionCallId={}", literal::string(id)))
                    .into_iter()
                    .collect();
                blocks.push(self.function_call(
                    func_op,
                    &VarNames::new(),
                    self.sole_catalog_id,
                    &extra,
                )?);
                continue;
            }

            if let Some(udm) = envelope.get("updateDataModel").and_then(Value::as_object) {
                blocks.push(self.update_data_model(udm, &surface_catalogs, is_first)?);
                continue;
            }
            if let Some(cs) = envelope.get("createSurface").and_then(Value::as_object) {
                if let Some(block) = self.create_surface(cs, &mut surface_catalogs, is_first)? {
                    blocks.push(block);
                }
                continue;
            }
            if let Some(uc) = envelope.get("updateComponents").and_then(Value::as_object) {
                if let Some(block) = self.update_components(uc, &surface_catalogs, is_first)? {
                    blocks.push(block);
                }
                continue;
            }
        }
        Ok(blocks.join("\n"))
    }

    fn surface_context(
        &self,
        surface_id: &str,
        surface_catalogs: &HashMap<String, String>,
    ) -> (Option<&'c str>, &'c Catalog) {
        let id: Option<&'c str> = match surface_catalogs.get(surface_id) {
            Some(id) => self.catalogs.get(id).map(Catalog::id),
            None => self.sole_catalog_id,
        };
        let catalog = id
            .and_then(|id| self.catalogs.get(id))
            .unwrap_or_else(|| self.first());
        (id, catalog)
    }

    fn surface_header(
        &self,
        surface_id: &str,
        send_data_model: Option<bool>,
        update: bool,
        extensions: Option<&Value>,
    ) -> Result<String> {
        let mut parts = vec![literal::string(surface_id)];
        if let Some(send) = send_data_model {
            parts.push(format!("sendDataModel={send}"));
        }
        if update {
            parts.push("update=true".into());
        }
        if let Some(ext) = extensions {
            parts.push(format!(
                "extensions={}",
                self.value(
                    ext,
                    &VarNames::new(),
                    false,
                    self.first(),
                    self.sole_catalog_id
                )?
            ));
        }
        Ok(format!("surface({})", parts.join(", ")))
    }

    fn update_data_model(
        &self,
        op: &Obj,
        surface_catalogs: &HashMap<String, String>,
        is_first: bool,
    ) -> Result<String> {
        let surface_id = surface_id_of(op);
        let (surface_catalog_id, catalog) = self.surface_context(&surface_id, surface_catalogs);
        let raw_path = op
            .get("path")
            .and_then(Value::as_str)
            .filter(|p| !p.is_empty())
            .unwrap_or("/");
        let base = raw_path.trim_end_matches('/');
        let data = match present(op.get("value")) {
            Some(v) => v.clone(),
            None if base.is_empty() => Value::Object(Obj::new()),
            None => Value::Null,
        };
        let value = self.value(&data, &VarNames::new(), false, catalog, surface_catalog_id)?;
        let path = if base.is_empty() {
            "$/".to_owned()
        } else {
            format!("${base}")
        };
        let mut lines = Vec::new();
        if !(is_first && surface_id == "default_surface") {
            lines.push(self.surface_header(&surface_id, None, false, None)?);
        }
        lines.push(format!("{path} = {value}"));
        Ok(lines.join("\n"))
    }

    fn create_surface(
        &self,
        cs: &Obj,
        surface_catalogs: &mut HashMap<String, String>,
        is_first: bool,
    ) -> Result<Option<String>> {
        let surface_id = surface_id_of(cs);
        match cs
            .get("catalogId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            Some(cid) => {
                if self.catalogs.get(cid).is_none() {
                    return Err(self
                        .catalogs
                        .unknown_catalog_error(&format!("Surface '{surface_id}'"), cid));
                }
                surface_catalogs.insert(surface_id.clone(), cid.to_owned());
            }
            None => {
                surface_catalogs.remove(&surface_id);
            }
        }
        let (surface_catalog_id, catalog) = self.surface_context(&surface_id, surface_catalogs);

        let send_data_model = cs.get("sendDataModel").and_then(Value::as_bool);
        let extensions = if self.is_v1 {
            cs.get("metadata")
                .and_then(Value::as_object)
                .and_then(|m| m.get("extensions"))
                .filter(|e| e.is_object())
        } else {
            None
        };
        let mut lines = Vec::new();
        if !(is_first
            && surface_id == "default_surface"
            && send_data_model.is_none()
            && extensions.is_none())
        {
            lines.push(self.surface_header(&surface_id, send_data_model, false, extensions)?);
        }

        let components = component_list(cs);
        let var_names = self.variable_names(&components)?;

        if let Some(data) = cs.get("dataModel").filter(|d| d.is_object()) {
            if has_data_to_write(data) {
                let mut leaves = Vec::new();
                flatten_data_model(data, String::new(), &mut leaves);
                leaves.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
                for (path, value) in leaves {
                    let value =
                        self.value(value, &var_names, false, catalog, surface_catalog_id)?;
                    lines.push(format!("${path} = {value}"));
                }
            }
        }

        self.write_components(&components, &var_names, surface_catalog_id, &mut lines)?;
        Ok((!lines.is_empty()).then(|| lines.join("\n")))
    }

    fn update_components(
        &self,
        uc: &Obj,
        surface_catalogs: &HashMap<String, String>,
        is_first: bool,
    ) -> Result<Option<String>> {
        let surface_id = surface_id_of(uc);
        let (surface_catalog_id, _) = self.surface_context(&surface_id, surface_catalogs);
        let components = component_list(uc);
        let has_root = components
            .iter()
            .any(|c| c.get("id").and_then(Value::as_str) == Some("root"));

        let mut lines = Vec::new();
        if has_root || !(is_first && surface_id == "default_surface") {
            lines.push(self.surface_header(&surface_id, None, has_root, None)?);
        }
        let var_names = self.variable_names(&components)?;
        self.write_components(&components, &var_names, surface_catalog_id, &mut lines)?;
        Ok((!lines.is_empty()).then(|| lines.join("\n")))
    }

    fn write_components(
        &self,
        components: &[Obj],
        var_names: &VarNames,
        surface_catalog_id: Option<&str>,
        lines: &mut Vec<String>,
    ) -> Result<()> {
        let arranged = order::arrange(components, self.options.order, |c| {
            let name = c.get("component").and_then(Value::as_str)?;
            self.resolve(
                SymbolKind::Component,
                name,
                self.component_catalog_id(c, surface_catalog_id),
                surface_catalog_id,
            )
            .ok()
            .map(|(catalog, _)| catalog)
        });
        for i in arranged {
            lines.push(self.component(&components[i], var_names, surface_catalog_id)?);
        }
        Ok(())
    }

    fn variable_names(&self, components: &[Obj]) -> Result<VarNames> {
        let ids: Vec<&str> = components
            .iter()
            .filter_map(|c| c.get("id").and_then(Value::as_str))
            .collect();
        if self.is_v1 {
            if let Some(bad) = ids.iter().find(|id| !is_uax31_identifier(id)) {
                return Err(validation(format!(
                    "Component id '{bad}' must be a valid UAX #31 identifier"
                )));
            }
        }
        Ok(assign_variable_names(ids.iter().copied()))
    }

    // ---- symbols ---------------------------------------------------------

    /// Resolves a component or function against its explicit or surface
    /// catalog: the catalog, and the `catalogId` to write, if any.
    fn resolve(
        &self,
        kind: SymbolKind,
        name: &str,
        item_catalog_id: Option<&Value>,
        surface_catalog_id: Option<&str>,
    ) -> Result<(&'c Catalog, Option<&'c str>)> {
        let label = match kind {
            SymbolKind::Component => "Component",
            SymbolKind::Function => "Function",
        };
        let actual = item_catalog_id
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .or(surface_catalog_id)
            .ok_or_else(|| {
                validation(format!(
                    "{label} '{name}' has no catalogId and its surface has no catalogId."
                ))
            })?;
        let catalog = self.catalogs.get(actual).ok_or_else(|| {
            self.catalogs
                .unknown_catalog_error(&format!("{label} '{name}'"), actual)
        })?;
        let defines = match kind {
            SymbolKind::Component => catalog.has_component(name),
            SymbolKind::Function => catalog.has_function(name),
        };
        if !defines {
            return Err(self.catalogs.unknown_symbol_error(kind, name, Some(actual)));
        }
        if self.sole_catalog_id.is_some() {
            return Ok((catalog, None));
        }
        let defining = self.catalogs.catalogs_defining(kind, name);
        if defining.as_slice() == [actual] {
            return Ok((catalog, None));
        }
        Ok((catalog, Some(catalog.id())))
    }

    /// The system function catalog for a `@name` call.
    fn system_function(&self, call: &Obj, name: &str) -> Result<&'c Catalog> {
        let catalog = self.first();
        if !catalog.system_functions().iter().any(|f| f.name() == name) {
            return Err(validation(format!("Unknown system function '{name}'.")));
        }
        if call.contains_key("catalogId") {
            return Err(validation(format!(
                "System function '{name}' is defined by the protocol, not a catalog, so it takes no catalogId."
            )));
        }
        Ok(catalog)
    }

    /// The catalog override of a component: `@catalogId` from v1.0, or a
    /// legacy bare `catalogId` unless the component declares `catalogId` as
    /// its own property.
    fn component_catalog_id<'v>(
        &self,
        comp: &'v Obj,
        surface_catalog_id: Option<&str>,
    ) -> Option<&'v Value> {
        if !self.is_v1 {
            return comp.get("catalogId");
        }
        if let Some(v) = comp.get("@catalogId") {
            return Some(v);
        }
        let legacy = comp.get("catalogId")?;
        let declares = |c: &Catalog, name: &str| {
            c.component(name).is_some_and(|comp| {
                comp.custom_envelope_properties()
                    .iter()
                    .any(|p| p == "catalogId")
            })
        };
        if let Some(name) = comp.get("component").and_then(Value::as_str) {
            let home = surface_catalog_id.and_then(|id| self.catalogs.get(id));
            match home {
                Some(home) if home.has_component(name) => {
                    if declares(home, name) {
                        return None;
                    }
                }
                _ => {
                    let declaring: Vec<&Catalog> = self
                        .catalogs
                        .catalogs()
                        .iter()
                        .filter(|c| c.has_component(name))
                        .collect();
                    if !declaring.is_empty() && declaring.iter().all(|c| declares(c, name)) {
                        return None;
                    }
                }
            }
        }
        Some(legacy)
    }

    // ---- components ------------------------------------------------------

    fn component(
        &self,
        comp: &Obj,
        var_names: &VarNames,
        surface_catalog_id: Option<&str>,
    ) -> Result<String> {
        let comp_id = comp.get("id").and_then(Value::as_str).unwrap_or("");
        let var_name = var_names
            .get(comp_id)
            .map(String::as_str)
            .unwrap_or(comp_id);
        let comp_name = comp.get("component").and_then(Value::as_str).unwrap_or("");

        let (catalog, emit_catalog_id) = self.resolve(
            SymbolKind::Component,
            comp_name,
            self.component_catalog_id(comp, surface_catalog_id),
            surface_catalog_id,
        )?;

        let properties = catalog.component_properties(comp_name);
        let custom: &[String] = catalog
            .component(comp_name)
            .map_or(&[], |c| c.custom_envelope_properties());
        let is_custom = |p: &str| custom.iter().any(|c| c == p);
        let check_prop = catalog.check_rule_property(comp_name);
        let extensions = if self.is_v1 {
            envelope_value(comp, "metadata", true, custom)
                .and_then(Value::as_object)
                .and_then(|m| m.get("extensions"))
                .filter(|e| e.is_object())
        } else {
            None
        };
        let keyword_args = self.options.keyword_args;
        let positional_until = if keyword_args && comp.contains_key("catalogId") {
            properties.iter().position(|p| p == "catalogId")
        } else {
            None
        };

        let mut args: Vec<String> = Vec::new();
        // Rules the `?name(…)` shorthand cannot spell, written after the
        // positional arguments as `checks=[{condition: …, message: …}]`.
        let mut long_form_checks: Option<String> = None;
        for (idx, prop) in properties.iter().enumerate() {
            let keyword = keyword_args && positional_until.is_none_or(|until| idx > until);
            if prop == "metadata" && !is_custom("metadata") && extensions.is_some() {
                continue;
            }
            let arg = match check_prop {
                Some(cp) if cp == prop && !is_custom(cp) => {
                    let checks = if cp == "checks" {
                        envelope_value(comp, "checks", self.is_v1, custom)
                    } else {
                        comp.get(cp)
                    };
                    let checks = match checks {
                        Some(rules) if self.needs_long_form(rules) => {
                            let rules =
                                self.value(rules, var_names, false, catalog, surface_catalog_id)?;
                            long_form_checks = Some(format!("{prop}={rules}"));
                            None
                        }
                        checks => checks,
                    };
                    self.check_property_arg(
                        prop,
                        checks,
                        comp,
                        var_names,
                        surface_catalog_id,
                        keyword,
                    )?
                }
                _ => {
                    if let Some(value) = comp.get(prop) {
                        let is_ref = catalog
                            .property_schema(comp_name, prop)
                            .is_some_and(|s| catalog.is_component_reference(s));
                        let value =
                            self.value(value, var_names, is_ref, catalog, surface_catalog_id)?;
                        Some(if keyword {
                            format!("{prop}={value}")
                        } else {
                            value
                        })
                    } else if keyword {
                        None
                    } else {
                        let skip = check_prop.filter(|cp| !is_custom(cp));
                        let later = properties[idx + 1..]
                            .iter()
                            .any(|p| Some(p.as_str()) != skip && comp.contains_key(p));
                        later.then(|| "_".to_owned())
                    }
                }
            };
            args.extend(arg);
        }
        trim_placeholders(&mut args);

        if check_prop == Some("checks") && is_custom("checks") {
            let checks = envelope_value(comp, "checks", self.is_v1, custom);
            let arg = match checks {
                Some(rules) if self.needs_long_form(rules) => Some(format!(
                    "checks={}",
                    self.value(rules, var_names, false, catalog, surface_catalog_id)?
                )),
                checks => self
                    .check_property_arg(
                        "checks",
                        checks,
                        comp,
                        var_names,
                        surface_catalog_id,
                        false,
                    )?
                    .filter(|a| a != "_"),
            };
            args.extend(arg);
        }
        args.extend(long_form_checks);
        if let Some(cid) = emit_catalog_id {
            args.push(format!("catalogId={}", literal::string(cid)));
        }
        if var_name != comp_id {
            args.push(format!("id={}", literal::string(comp_id)));
        }
        if let Some(ext) = extensions {
            args.push(format!(
                "extensions={}",
                self.value(ext, var_names, false, catalog, surface_catalog_id)?
            ));
        }
        Ok(format!("{var_name} = {comp_name}({})", args.join(", ")))
    }

    fn check_property_arg(
        &self,
        prop: &str,
        checks: Option<&Value>,
        comp: &Obj,
        var_names: &VarNames,
        surface_catalog_id: Option<&str>,
        keyword: bool,
    ) -> Result<Option<String>> {
        let rules = match checks {
            Some(Value::Array(rules)) if !rules.is_empty() => rules,
            _ => return Ok((!keyword).then(|| "_".to_owned())),
        };
        let mut written = Vec::new();
        for rule in rules.iter().filter_map(Value::as_object) {
            written.push(self.check_rule(rule, comp, var_names, surface_catalog_id)?);
        }
        let repr = if written.len() == 1 {
            written.pop().expect("one rule")
        } else {
            format!("[{}]", written.join(", "))
        };
        Ok(Some(if keyword {
            format!("{prop}={repr}")
        } else {
            repr
        }))
    }

    /// Whether a list of check rules has one the `?name(…)` shorthand cannot
    /// spell: a rule whose condition is not a function call, such as a
    /// binding (`{condition: {"@path": "/ok"}}`) or a literal. Upstream
    /// writes such a rule as `?None`, which does not compile (#AXC-18); the
    /// whole list is written in the long form instead, so the rules keep
    /// their order.
    fn needs_long_form(&self, checks: &Value) -> bool {
        let Value::Array(rules) = checks else {
            return false;
        };
        rules.iter().filter_map(Value::as_object).any(|rule| {
            let condition = rule
                .get("condition")
                .and_then(Value::as_object)
                .unwrap_or(rule);
            !present(condition.get("@call"))
                .or_else(|| condition.get("call"))
                .and_then(Value::as_str)
                .is_some_and(|name| !name.is_empty())
        })
    }

    /// Writes a check rule as `?name(...)` or `?name`.
    fn check_rule(
        &self,
        rule: &Obj,
        component: &Obj,
        var_names: &VarNames,
        surface_catalog_id: Option<&str>,
    ) -> Result<String> {
        let condition = rule.get("condition").and_then(Value::as_object);
        let message = match condition {
            Some(_) => rule.get("message").and_then(Value::as_str).unwrap_or(""),
            None => "",
        };
        let condition = condition.unwrap_or(rule);

        let func_name = present(condition.get("@call"))
            .or_else(|| condition.get("call"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("None");
        let check_args = optional_object(
            condition.get("args"),
            &format!("Check '{func_name}'"),
            "args",
        )?;

        let (catalog, emit_catalog_id) = if func_name != "None" {
            let (c, id) = self.resolve(
                SymbolKind::Function,
                func_name,
                condition.get("catalogId"),
                surface_catalog_id,
            )?;
            (Some(c), id)
        } else {
            (None, None)
        };
        let check_props: &[String] = catalog.map_or(&[], |c| c.function_arguments(func_name));
        let catalog = catalog.unwrap_or_else(|| self.first());

        let path_of = |v: Option<&Value>| -> Option<Value> {
            let obj = v?.as_object()?;
            present(obj.get("@path"))
                .or_else(|| obj.get("path"))
                .cloned()
        };
        let comp_path = path_of(component.get("value"));
        let check_path = path_of(check_args.get("value"));
        let start = usize::from(
            check_props.first().is_some_and(|p| p == "value")
                && matches!(&comp_path, Some(Value::String(_)))
                && comp_path == check_path,
        );

        let mut args = Vec::new();
        for p in &check_props[start..] {
            args.push(match check_args.get(p) {
                Some(v) => self.value(v, var_names, false, catalog, surface_catalog_id)?,
                None => "_".to_owned(),
            });
        }

        let mut chars = func_name.chars();
        let cap_name: String = match chars.next() {
            Some(c) => c
                .to_uppercase()
                .chain(chars.as_str().to_lowercase().chars())
                .collect(),
            None => String::new(),
        };
        if func_name != "None"
            && !message.is_empty()
            && message != format!("{cap_name} check failed")
        {
            args.push(literal::string(message));
        } else {
            trim_placeholders(&mut args);
        }
        if let Some(cid) = emit_catalog_id {
            args.push(format!("{{catalogId: {}}}", literal::string(cid)));
        }
        Ok(if args.is_empty() {
            format!("?{func_name}")
        } else {
            format!("?{func_name}({})", args.join(", "))
        })
    }

    /// Writes a function call object as `name(args...)`.
    fn function_call(
        &self,
        call: &Obj,
        var_names: &VarNames,
        surface_catalog_id: Option<&str>,
        extra_kwargs: &[String],
    ) -> Result<String> {
        let func_name = present(call.get("@call"))
            .or_else(|| call.get("call"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let list_args = call.get("args").and_then(Value::as_array);
        let dict_args = match list_args {
            Some(_) => Obj::new(),
            None => optional_object(
                call.get("args"),
                &format!("Function call '{func_name}'"),
                "args",
            )?,
        };

        let (catalog, emit_catalog_id) = if func_name.starts_with('@') {
            (self.system_function(call, func_name)?, None)
        } else {
            self.resolve(
                SymbolKind::Function,
                func_name,
                call.get("catalogId"),
                surface_catalog_id,
            )?
        };

        let mut args = Vec::new();
        // With keyword arguments, a call names what it sets and leaves out
        // what it doesn't (`f(toggle=true)`, not `f(_, _, true)`). Arguments
        // given as a list have no names and stay positional.
        let keyword = self.options.keyword_args && list_args.is_none();
        for (idx, p) in catalog.function_arguments(func_name).iter().enumerate() {
            let value = match list_args {
                Some(list) => list.get(idx),
                None => dict_args.get(p),
            };
            match value {
                Some(v) => {
                    let v = self.value(v, var_names, false, catalog, surface_catalog_id)?;
                    args.push(if keyword { format!("{p}={v}") } else { v });
                }
                None if keyword => {}
                None => args.push("_".to_owned()),
            }
        }
        trim_placeholders(&mut args);
        if let Some(cid) = emit_catalog_id {
            args.push(format!("catalogId={}", literal::string(cid)));
        }
        args.extend(extra_kwargs.iter().cloned());
        Ok(format!("{func_name}({})", args.join(", ")))
    }

    // ---- values ----------------------------------------------------------

    /// Writes one value in Express notation.
    fn value(
        &self,
        val: &Value,
        var_names: &VarNames,
        is_ref: bool,
        catalog: &'c Catalog,
        surface_catalog_id: Option<&str>,
    ) -> Result<String> {
        let obj = match val {
            Value::Null => return Ok("null".into()),
            Value::Bool(b) => return Ok(b.to_string()),
            Value::Number(n) => return literal::number(n),
            Value::String(s) if is_ref => return Ok(reference(s, var_names)),
            Value::String(s) => return Ok(literal::string(s)),
            Value::Array(items) => {
                let items = items
                    .iter()
                    .map(|v| self.value(v, var_names, is_ref, catalog, surface_catalog_id))
                    .collect::<Result<Vec<_>>>()?;
                return Ok(format!("[{}]", items.join(", ")));
            }
            Value::Object(obj) => obj,
        };

        // A template keeps the plain `path` key in every version.
        if let (Some(Value::String(path)), Some(Value::String(component_id))) =
            (obj.get("path"), obj.get("componentId"))
        {
            let mut binding = Obj::new();
            binding.insert(catalog.path_key().into(), Value::String(path.clone()));
            let path = self.value(
                &Value::Object(binding),
                var_names,
                false,
                catalog,
                surface_catalog_id,
            )?;
            let key = match obj.get("key") {
                Some(key @ Value::String(_)) => format!(
                    ", key={}",
                    self.value(key, var_names, false, catalog, surface_catalog_id)?
                ),
                _ => String::new(),
            };
            return Ok(format!(
                "_template({path}, {}{key})",
                reference(component_id, var_names)
            ));
        }

        // A data binding is an object whose only key is the binding key.
        if let (Some(Value::String(path)), 1) = (obj.get(catalog.path_key()), obj.len()) {
            return Ok(match path.strip_prefix('/') {
                Some(rest) => format!("$/{rest}"),
                None => format!("${path}"),
            });
        }

        // Before v1.0 an event has no userMessage, and the compiler rejects
        // `userMessage=`, so such an object is written as a map literal.
        if let Some(evt) = obj.get("event").and_then(Value::as_object) {
            if self.is_v1 || !evt.contains_key("userMessage") {
                let name = evt.get("name").and_then(Value::as_str).unwrap_or("");
                let mut args = vec![literal::string(name)];
                if present(evt.get("context")).is_some() {
                    let ctx =
                        optional_object(evt.get("context"), &format!("Event '{name}'"), "context")?;
                    let mut entries = Vec::new();
                    for (k, v) in &ctx {
                        entries.push(format!(
                            "{}: {}",
                            literal::key(k),
                            self.value(v, var_names, false, catalog, surface_catalog_id)?
                        ));
                    }
                    args.push(format!("{{{}}}", entries.join(", ")));
                }
                if let Some(message) = present(evt.get("userMessage")) {
                    args.push(format!(
                        "userMessage={}",
                        self.value(message, var_names, false, catalog, surface_catalog_id)?
                    ));
                }
                return Ok(format!("Event({})", args.join(", ")));
            }
        }

        if let Some(call) = obj.get("functionCall").and_then(Value::as_object) {
            return self.function_call(call, var_names, surface_catalog_id, &[]);
        }

        // Only the version's call key names a call; a plain `call` in v1.0 is data.
        if obj.get(catalog.call_key()).is_some_and(Value::is_string) {
            return self.function_call(obj, var_names, surface_catalog_id, &[]);
        }

        let mut entries = Vec::new();
        for (k, v) in obj {
            let item_is_ref = is_ref || k == "child" || k == "componentId";
            entries.push(format!(
                "{}: {}",
                literal::key(k),
                self.value(v, var_names, item_is_ref, catalog, surface_catalog_id)?
            ));
        }
        Ok(format!("{{{}}}", entries.join(", ")))
    }
}

/// The `createSurface` payload of a coalesced message.
fn open_create(msg: &mut Obj) -> &mut Obj {
    msg.get_mut("createSurface")
        .and_then(Value::as_object_mut)
        .expect("an open surface is a createSurface")
}

/// The component objects of a `createSurface` or `updateComponents`.
fn component_list(op: &Obj) -> Vec<Obj> {
    op.get("components")
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(Value::as_object).cloned().collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn catalogs() -> CatalogSet {
        let catalog = Catalog::from_json(json!({
            "catalogId": "demo",
            "protocolVersion": "1.0",
            "components": {
                "Column": {"properties": {
                    "component": {"const": "Column"},
                    "children": {"$ref": "common_types.json#/$defs/ChildList"}
                }},
                "Card": {"properties": {
                    "component": {"const": "Card"},
                    "child": {"$ref": "common_types.json#/$defs/ComponentId"}
                }},
                "Text": {"properties": {
                    "component": {"const": "Text"},
                    "text": {"$ref": "common_types.json#/$defs/DynamicString"},
                    "variant": {"type": "string"}
                }}
            }
        }))
        .unwrap();
        CatalogSet::new(vec![catalog]).unwrap()
    }

    fn surface(components: Value) -> Vec<Value> {
        vec![json!({"version": "1.0", "createSurface": {
            "surfaceId": "s1", "catalogId": "demo", "components": components
        }})]
    }

    fn run(messages: &[Value], options: DecompileOptions) -> String {
        decompile(&catalogs(), messages, &options).unwrap()
    }

    #[test]
    fn upstream_order_keeps_the_message_order() {
        let messages = surface(json!([
            {"id": "a", "component": "Text", "text": "A"},
            {"id": "root", "component": "Column", "children": ["card", "a"]},
            {"id": "card", "component": "Card", "child": "b"},
            {"id": "b", "component": "Text", "text": "B"}
        ]));
        assert_eq!(
            run(&messages, DecompileOptions::default()),
            "surface(\"s1\")\na = Text(\"A\")\nroot = Column([card, a])\n\
             card = Card(b)\nb = Text(\"B\")"
        );
    }

    #[test]
    fn top_down_writes_root_first_then_breadth_first() {
        let messages = surface(json!([
            {"id": "b", "component": "Text", "text": "B"},
            {"id": "a", "component": "Text", "text": "A"},
            {"id": "orphan", "component": "Text", "text": "O"},
            {"id": "card", "component": "Card", "child": "b"},
            {"id": "root", "component": "Column", "children": ["card", "a"]}
        ]));
        let options = DecompileOptions {
            order: Order::TopDown,
            ..Default::default()
        };
        assert_eq!(
            run(&messages, options),
            "surface(\"s1\")\nroot = Column([card, a])\ncard = Card(b)\n\
             a = Text(\"A\")\nb = Text(\"B\")\norphan = Text(\"O\")"
        );
    }

    #[test]
    fn top_down_follows_templates_and_survives_cycles() {
        let messages = surface(json!([
            {"id": "x", "component": "Card", "child": "y"},
            {"id": "y", "component": "Card", "child": "x"},
            {"id": "row", "component": "Text", "text": {"@path": "name"}},
            {"id": "root", "component": "Column",
             "children": {"path": "/items", "componentId": "row"}}
        ]));
        let options = DecompileOptions {
            order: Order::TopDown,
            ..Default::default()
        };
        assert_eq!(
            run(&messages, options),
            "surface(\"s1\")\nroot = Column(_template($/items, row))\nrow = Text($name)\n\
             x = Card(y)\ny = Card(x)"
        );
    }

    #[test]
    fn keyword_args_name_every_argument() {
        let messages = surface(json!([
            {"id": "root", "component": "Text", "variant": "h1"}
        ]));
        assert_eq!(
            run(&messages, DecompileOptions::default()),
            "surface(\"s1\")\nroot = Text(_, \"h1\")"
        );
        let options = DecompileOptions {
            keyword_args: true,
            ..Default::default()
        };
        assert_eq!(
            run(&messages, options),
            "surface(\"s1\")\nroot = Text(variant=\"h1\")"
        );
    }

    #[test]
    fn a_non_identifier_id_is_rejected_from_v1_0() {
        let messages = surface(json!([{"id": "a-b", "component": "Text", "text": "x"}]));
        let err = decompile(&catalogs(), &messages, &DecompileOptions::default()).unwrap_err();
        assert!(matches!(err, Error::Validation(m) if m.contains("UAX #31")));
    }
}
