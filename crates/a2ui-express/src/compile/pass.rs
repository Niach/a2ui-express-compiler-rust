//! Compiling one scope's values: components, calls, events, checks, maps.
//!
//! Follows `ExpressCompiler._compileAstNode` and `_compileValue` with their
//! helpers in upstream's `compiler.ts`.

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value, json};

use super::error::CompileError;
use super::{CompIds, Compiler, Symbols, is_uax31_identifier, string_literal};
use crate::schema::{Catalog, CatalogVersion, SlotKind, SymbolKind, envelope_key};
use crate::syntax::{Call, Expr, ExprKind, MapEntry};

/// Calls Express reads as constructs, which a variable holding them names
/// rather than becomes.
const RESERVED_CALLS: [&str; 4] = ["Event", "_template", "surface", "deleteSurface"];

/// The keywords `Event(...)` takes.
const EVENT_KEYWORDS: [&str; 2] = ["userMessage", "context"];

/// State shared by every scope of one block.
#[derive(Debug)]
pub(crate) struct Ctx {
    /// Inline components hoisted out of the scope being compiled.
    pub extra_components: Vec<Map<String, Value>>,
    /// Ids generated for inline components in the scope.
    pub generated_ids: HashSet<String>,
    pub inline_counter: usize,
    pub call_counter: usize,
    /// The data bindings of the component whose checks are compiled, by
    /// property.
    bound_paths: HashMap<String, Value>,
    /// Variables whose values are being compiled.
    resolving: HashSet<String>,
    version: CatalogVersion,
    path_key: &'static str,
    call_key: &'static str,
}

impl Ctx {
    pub fn new(version: CatalogVersion) -> Self {
        let v1 = version.at_least_v1();
        Ctx {
            extra_components: Vec::new(),
            generated_ids: HashSet::new(),
            inline_counter: 0,
            call_counter: 0,
            bound_paths: HashMap::new(),
            resolving: HashSet::new(),
            version,
            path_key: if v1 { "@path" } else { "path" },
            call_key: if v1 { "@call" } else { "call" },
        }
    }

    fn v1(&self) -> bool {
        self.version.at_least_v1()
    }
}

/// A call's arguments as upstream's visitor gives them: the positional ones
/// in order, and the keywords with the last of a repeated one winning at the
/// position of the first.
#[derive(Debug, Clone)]
pub(crate) struct CallArgs<'e> {
    pub pos: Vec<&'e Expr>,
    pub kw: Vec<(&'e str, &'e Expr)>,
}

impl<'e> CallArgs<'e> {
    pub fn of(call: &'e Call) -> Self {
        let mut kw: Vec<(&str, &Expr)> = Vec::new();
        for (k, v) in call.keywords() {
            match kw.iter_mut().find(|(key, _)| *key == k) {
                Some(slot) => slot.1 = v,
                None => kw.push((k, v)),
            }
        }
        CallArgs {
            pos: call.positional().collect(),
            kw,
        }
    }

    pub fn get(&self, key: &str) -> Option<&'e Expr> {
        get(&self.kw, key)
    }

    fn remove(&mut self, key: &str) -> Option<&'e Expr> {
        remove(&mut self.kw, key)
    }
}

fn get<'e>(kw: &[(&str, &'e Expr)], key: &str) -> Option<&'e Expr> {
    kw.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

fn remove<'e>(kw: &mut Vec<(&str, &'e Expr)>, key: &str) -> Option<&'e Expr> {
    let i = kw.iter().position(|(k, _)| *k == key)?;
    Some(kw.remove(i).1)
}

/// A map literal's entries, the last of a repeated key winning at the
/// position of the first.
fn map_entries(entries: &[MapEntry]) -> Vec<(&str, &Expr)> {
    let mut out: Vec<(&str, &Expr)> = Vec::new();
    for e in entries {
        match out.iter_mut().find(|(k, _)| *k == e.key) {
            Some(slot) => slot.1 = &e.value,
            None => out.push((&e.key, &e.value)),
        }
    }
    out
}

/// The schema declared at a value's position, with the catalog that owns it.
#[derive(Debug, Clone, Copy)]
struct Slot<'c> {
    schema: &'c Value,
    catalog: &'c Catalog,
}

impl<'c> Slot<'c> {
    fn item(self) -> Option<Slot<'c>> {
        let schema = self.catalog.resolve_item_schema(self.schema)?;
        Some(Slot { schema, ..self })
    }

    fn property(self, name: &str) -> Option<Slot<'c>> {
        let schema = self.catalog.resolve_property_schema(self.schema, name)?;
        Some(Slot { schema, ..self })
    }

    fn is_action(self) -> bool {
        self.catalog.is_action_slot(self.schema)
    }

    fn kind(self) -> Option<SlotKind> {
        self.catalog.slot_kind(self.schema)
    }
}

/// Where an inline component sits, for its generated id.
#[derive(Debug, Clone)]
struct Inline {
    parent: String,
    property: String,
    index: Option<usize>,
}

/// How a value is compiled at its position.
#[derive(Debug, Clone, Default)]
struct At<'c> {
    /// Inside an `Action`: a function call becomes `{functionCall: …}`.
    action: bool,
    inline: Option<Inline>,
    /// An unassigned variable here is a forward component id.
    child_ref: bool,
    /// The items of a list here are component ids.
    list_refs: bool,
    slot: Option<Slot<'c>>,
}

impl<'c> At<'c> {
    fn action(action: bool) -> Self {
        At {
            action,
            ..At::default()
        }
    }
}

/// One scope's compilation.
pub(crate) struct Pass<'c, 's, 'a> {
    c: &'c Compiler,
    syms: &'s Symbols<'a>,
    ids: &'s CompIds,
    pub ctx: &'s mut Ctx,
}

impl<'c, 's, 'a> Pass<'c, 's, 'a> {
    pub fn new(c: &'c Compiler, syms: &'s Symbols<'a>, ids: &'s CompIds, ctx: &'s mut Ctx) -> Self {
        Pass { c, syms, ids, ctx }
    }

    // ---- components -------------------------------------------------------

    /// A variable's value as a component, or `None` when it holds no
    /// component (a map, an event, a function call, …).
    pub fn ast_node(
        &mut self,
        var: &str,
        ast: &Expr,
    ) -> Result<Option<Map<String, Value>>, CompileError> {
        match &ast.kind {
            ExprKind::Call(call) => self.component(var, call),
            _ => Ok(None),
        }
    }

    fn component(
        &mut self,
        var: &str,
        call: &Call,
    ) -> Result<Option<Map<String, Value>>, CompileError> {
        let name = call.name.as_str();
        let mut args = CallArgs::of(call);
        if name.starts_with('@') {
            // Inlined wherever it is named, but checked here so that one
            // nothing names is rejected too.
            self.c
                .system_function(name, args.get("catalogId").is_some())?;
            return Ok(None);
        }

        let explicit_catalog = args.remove("catalogId");
        let explicit_id = self.c.explicit_id(name, args.remove("id"))?;

        if !self.c.defines_component(name, explicit_catalog)
            && ((explicit_catalog.is_none() && RESERVED_CALLS.contains(&name))
                || self.c.defines_function(name, explicit_catalog))
        {
            return Ok(None);
        }

        let resolved = self
            .c
            .resolve(SymbolKind::Component, name, explicit_catalog)?;
        let cat: &'c Catalog = resolved.catalog;
        let comp_id = explicit_id
            .or_else(|| self.ids.get(var))
            .unwrap_or(var)
            .to_owned();
        let properties = cat.component_properties(name);
        let raw_extensions = if properties.iter().any(|p| p == "extensions") {
            None
        } else {
            args.remove("extensions")
        };
        let v1 = self.ctx.v1();

        let mut dict = Map::new();
        dict.insert("id".into(), comp_id.clone().into());
        dict.insert("component".into(), name.into());
        if let Some(cid) = resolved.catalog_id {
            dict.insert(envelope_key("catalogId", v1), cid.into());
        }
        if let Some(raw) = raw_extensions {
            if !v1 {
                return Err(CompileError::validation(format!(
                    "extensions on component '{name}' is only supported in A2UI v1.0 or later."
                )));
            }
            let ext = self.extensions_map(raw, &format!("component '{name}'"))?;
            dict.insert(envelope_key("metadata", v1), json!({"extensions": ext}));
        }

        let check_prop = cat.check_rule_property(name);
        let custom = cat
            .component(name)
            .map_or(&[][..], |c| c.custom_envelope_properties());
        let non_check: Vec<&str> = properties
            .iter()
            .map(String::as_str)
            .filter(|p| match check_prop {
                Some(cp) if !custom.iter().any(|c| c == cp) => *p != cp,
                _ => true,
            })
            .collect();

        let mut raw_checks: Vec<&Expr> = Vec::new();
        let mut pos_args: Vec<&Expr> = Vec::new();
        for &arg in &args.pos {
            if is_check_expression(arg) {
                collect_checks(arg, &mut raw_checks);
            } else {
                pos_args.push(arg);
            }
        }

        let required = cat.component_required(name);
        let is_required = |p: &str| required.iter().any(|r| r == p);
        let accepts_number = |p: &str| {
            cat.property_schema(name, p)
                .is_some_and(|s| cat.accepts_number(s))
        };
        let mut pairs: Vec<(&str, &Expr)> = Vec::new();
        let mut prop_idx = 0;
        for (arg_idx, &arg) in pos_args.iter().enumerate() {
            if prop_idx >= non_check.len() {
                return Err(CompileError::validation(format!(
                    "{name} takes at most {} positional arguments ({}), but component '{comp_id}' passes {}.",
                    non_check.len(),
                    non_check.join(", "),
                    pos_args.len()
                )));
            }
            if matches!(arg.kind, ExprKind::Number(_)) && !accepts_number(non_check[prop_idx]) {
                // A trailing number skips optional enums to the one property
                // after them that takes a number.
                let mut moved = false;
                if arg_idx == pos_args.len() - 1 {
                    let later: Vec<usize> = (prop_idx + 1..non_check.len())
                        .filter(|&i| {
                            args.get(non_check[i]).is_none() && accepts_number(non_check[i])
                        })
                        .collect();
                    if let [target] = later[..] {
                        let all_optional_enums = (prop_idx..target).all(|i| {
                            let p = non_check[i];
                            !is_required(p)
                                && cat.property_enum(name, p).is_some_and(|e| !e.is_empty())
                        });
                        if all_optional_enums {
                            prop_idx = target;
                            moved = true;
                        }
                    }
                }
                if !moved && !is_required(non_check[prop_idx]) {
                    return Err(CompileError::validation(format!(
                        "Positional argument {} for component '{name}' is a number, which is not valid for optional property '{}'.",
                        arg_idx + 1,
                        non_check[prop_idx]
                    )));
                }
            }
            pairs.push((non_check[prop_idx], arg));
            prop_idx += 1;
        }
        for &(k, v) in &args.kw {
            if is_check_expression(v) {
                collect_checks(v, &mut raw_checks);
            } else {
                pairs.push((k, v));
            }
        }

        let mut seen: HashSet<&str> = HashSet::new();
        let mut bound = HashMap::new();
        for (prop, arg) in pairs {
            if !properties.iter().any(|p| p == prop) {
                return Err(CompileError::unknown_property(name, prop, properties));
            }
            if !seen.insert(prop) {
                return Err(CompileError::duplicate_property(name, prop));
            }
            if matches!(arg.kind, ExprKind::Skipped) {
                dict.insert(prop.into(), Value::Null);
                continue;
            }

            let schema = cat.property_schema(name, prop);
            let slot = schema.map(|schema| Slot {
                schema,
                catalog: cat,
            });
            let kind = slot.and_then(Slot::kind);
            let mut mapped = self.value(
                arg,
                At {
                    action: slot.is_some_and(Slot::is_action),
                    inline: Some(Inline {
                        parent: comp_id.clone(),
                        property: prop.to_owned(),
                        index: None,
                    }),
                    child_ref: kind == Some(SlotKind::Child),
                    list_refs: kind == Some(SlotKind::ChildList)
                        && matches!(arg.kind, ExprKind::List(_)),
                    slot,
                },
            )?;

            if let Some(schema) = schema {
                self.check_bindings(name, prop, arg, Some(schema), cat, &mut Vec::new())?;
                if let Value::Array(items) = &mut mapped {
                    if cat.expects_option_objects(schema) {
                        for item in items.iter_mut() {
                            if let Value::String(s) = item {
                                *item = json!({"label": s, "value": s});
                            }
                        }
                    }
                }
            }

            if let Some(allowed) = cat.property_enum(name, prop) {
                if !(mapped.is_object() || mapped.is_array())
                    && !allowed.iter().any(|a| loose_eq(a, &mapped))
                {
                    let list = allowed
                        .iter()
                        .map(|a| format!("'{}'", js_string(a)))
                        .collect::<Vec<_>>()
                        .join(", ");
                    return Err(CompileError::validation(format!(
                        "Value '{}' is not a valid enum choice for property '{prop}' of component '{name}'. Allowed values are: [{list}]",
                        js_string(&mapped)
                    )));
                }
            }

            if self.resolves_to_path(arg) {
                bound.insert(prop.to_owned(), mapped.clone());
            }
            dict.insert(prop.into(), mapped);
        }

        for req in required {
            let skip = req == "id"
                || req == "component"
                || (req == "checks" && !custom.iter().any(|c| c == "checks"))
                || req == "@checks";
            if !skip && !seen.contains(req.as_str()) {
                return Err(CompileError::missing_required(name, req));
            }
        }

        self.ctx.bound_paths = bound;
        if !raw_checks.is_empty() {
            let Some(check_prop) = check_prop else {
                return Err(CompileError::validation(format!(
                    "Component '{name}' does not accept checks."
                ))
                .with_help(
                    "Remove the check expressions, or use a component whose schema declares a list of CheckRule.",
                ));
            };
            let mut compiled = Vec::new();
            for rc in raw_checks {
                if let ExprKind::Check { name, args } = &rc.kind {
                    let (condition, message) = self.check_call(name, args, false)?;
                    compiled.push(json!({"condition": condition, "message": message}));
                }
            }
            if !compiled.is_empty() {
                let key = if check_prop == "checks" {
                    envelope_key("checks", v1)
                } else {
                    check_prop.to_owned()
                };
                dict.insert(key, compiled.into());
            }
        }
        self.ctx.bound_paths.clear();

        dict.retain(|_, v| !v.is_null());
        Ok(Some(dict))
    }

    /// Whether a value is a `$path`, directly or through variables. A map
    /// literal is data, even one shaped like a binding.
    fn resolves_to_path(&self, e: &Expr) -> bool {
        let mut seen = HashSet::new();
        let mut e: &Expr = e;
        while let ExprKind::Variable(name) = &e.kind {
            if !seen.insert(name.as_str()) {
                return false;
            }
            match self.syms.get(name) {
                Some(v) => e = v,
                None => return false,
            }
        }
        matches!(e.kind, ExprKind::Path(_))
    }

    /// Rejects a `$path` wherever the schema at its position admits none,
    /// walking lists, map literals and variables alongside the schema. Calls
    /// are not searched: they check their own arguments. A position the
    /// schema does not declare admits a binding.
    fn check_bindings(
        &self,
        component: &str,
        property: &str,
        v: &Expr,
        schema: Option<&Value>,
        cat: &Catalog,
        seen: &mut Vec<String>,
    ) -> Result<(), CompileError> {
        match &v.kind {
            ExprKind::Path(_) => {
                if schema.is_some_and(|s| !cat.admits_path(s)) {
                    return Err(CompileError::forbidden_binding(component, property));
                }
            }
            ExprKind::Variable(name) => {
                if !seen.contains(name) {
                    if let Some(value) = self.syms.get(name) {
                        seen.push(name.clone());
                        let r = self.check_bindings(component, property, value, schema, cat, seen);
                        seen.pop();
                        r?;
                    }
                }
            }
            ExprKind::List(items) => {
                let item_schema = schema.and_then(|s| cat.resolve_item_schema(s));
                for item in items {
                    self.check_bindings(component, property, item, item_schema, cat, seen)?;
                }
            }
            ExprKind::Map(entries) => {
                for (k, sub) in map_entries(entries) {
                    let sub_schema = schema.and_then(|s| cat.resolve_property_schema(s, k));
                    self.check_bindings(component, property, sub, sub_schema, cat, seen)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    // ---- values -----------------------------------------------------------

    /// A value outside any schema position and action.
    pub fn plain_value(&mut self, v: &Expr) -> Result<Value, CompileError> {
        self.value(v, At::default())
    }

    fn value(&mut self, v: &Expr, at: At<'c>) -> Result<Value, CompileError> {
        Ok(match &v.kind {
            ExprKind::Null => Value::Null,
            ExprKind::Bool(b) => Value::Bool(*b),
            ExprKind::Number(n) => Value::Number(n.to_json()),
            ExprKind::String(s) => Value::String(s.clone()),
            ExprKind::List(items) => {
                // Resolved once for the list: every item shares the schema.
                let item_slot = at.slot.and_then(Slot::item);
                let item_action = at.action || item_slot.is_some_and(Slot::is_action);
                let item_kind = item_slot.and_then(Slot::kind);
                let mut out = Vec::with_capacity(items.len());
                for (index, item) in items.iter().enumerate() {
                    out.push(self.value(
                        item,
                        At {
                            action: item_action,
                            inline: at.inline.as_ref().map(|i| Inline {
                                index: Some(index),
                                ..i.clone()
                            }),
                            child_ref: at.list_refs || item_kind == Some(SlotKind::Child),
                            list_refs: item_kind == Some(SlotKind::ChildList)
                                && matches!(item.kind, ExprKind::List(_)),
                            slot: item_slot,
                        },
                    )?);
                }
                Value::Array(out)
            }
            ExprKind::Map(entries) => {
                // A map literal is always data, whatever its keys are.
                let mut out = Map::new();
                for (k, sub) in map_entries(entries) {
                    let slot = at.slot.and_then(|s| s.property(k));
                    let kind = slot.and_then(Slot::kind);
                    let compiled = self.value(
                        sub,
                        At {
                            action: at.action || slot.is_some_and(Slot::is_action),
                            inline: None,
                            child_ref: kind == Some(SlotKind::Child),
                            list_refs: kind == Some(SlotKind::ChildList)
                                && matches!(sub.kind, ExprKind::List(_)),
                            slot,
                        },
                    )?;
                    out.insert(k.to_owned(), compiled);
                }
                Value::Object(out)
            }
            ExprKind::Path(p) => json!({ self.ctx.path_key: p }),
            ExprKind::Variable(name) => return self.variable_ref(name, at),
            ExprKind::Skipped => {
                return Err(CompileError::validation(
                    "'_' can only stand in for an argument.",
                ));
            }
            ExprKind::Check { name, args } => self.check_call(name, args, at.action)?.0,
            ExprKind::Call(call) => return self.call_value(call, at.action, at.inline),
        })
    }

    fn variable_ref(&mut self, name: &str, at: At<'c>) -> Result<Value, CompileError> {
        if let Some(id) = self.ids.get(name) {
            return Ok(id.into());
        }
        if let Some(value) = self.syms.get(name) {
            if !self.ctx.resolving.insert(name.to_owned()) {
                return Err(CompileError::validation(format!(
                    "Variable '{name}' refers to itself through its own value."
                )));
            }
            let r = self.value(value, At { inline: None, ..at });
            self.ctx.resolving.remove(name);
            return r;
        }
        if !at.child_ref {
            return Err(CompileError::validation(format!(
                "Variable '{name}' is not assigned in surface."
            )));
        }
        Ok(name.into())
    }

    fn call_value(
        &mut self,
        call: &Call,
        action: bool,
        inline: Option<Inline>,
    ) -> Result<Value, CompileError> {
        let args = CallArgs::of(call);
        match call.name.as_str() {
            "_template" => return self.template(&args.pos, action),
            "Event" => return self.event(&args),
            _ => {}
        }
        if self.c.defines_component(&call.name, args.get("catalogId")) {
            return self.inline_component(call, &args, inline);
        }
        let (compiled, _) = self.function_call(&call.name, &args.pos, args.kw, false, false)?;
        Ok(if action {
            json!({"functionCall": compiled})
        } else {
            Value::Object(compiled)
        })
    }

    fn template(&mut self, args: &[&Expr], action: bool) -> Result<Value, CompileError> {
        if args.len() < 2 {
            return Err(CompileError::parse(
                "_template helper requires exactly 2 arguments: path and templateComponent.",
            ));
        }
        let path = self.value(args[0], At::action(action))?;
        let Some(path) = path.get(self.ctx.path_key).cloned() else {
            return Err(CompileError::parse(format!(
                "The first argument to _template must be a dynamic data binding path (prefixed by $), got: {}",
                format_value(args[0])
            )));
        };
        let component = self.value(
            args[1],
            At {
                child_ref: true,
                ..At::action(action)
            },
        )?;
        Ok(json!({"path": path, "componentId": component}))
    }

    fn inline_component(
        &mut self,
        call: &Call,
        args: &CallArgs<'_>,
        inline: Option<Inline>,
    ) -> Result<Value, CompileError> {
        self.ctx.inline_counter += 1;
        let inline_id = match inline {
            Some(Inline {
                parent,
                property,
                index: Some(i),
            }) => format!("{parent}_{property}_{i}"),
            Some(Inline {
                parent, property, ..
            }) => format!("{parent}_{property}"),
            None => format!("_inline_{}", self.ctx.inline_counter),
        };
        let actual_id = args
            .get("id")
            .and_then(string_literal)
            .filter(|s| !s.is_empty())
            .unwrap_or(&inline_id)
            .to_owned();
        if self.syms.contains(&actual_id)
            || self.ids.has_id(&actual_id)
            || self.ctx.generated_ids.contains(&actual_id)
        {
            return Err(CompileError::id_collision(&actual_id));
        }
        self.ctx.generated_ids.insert(actual_id);
        let start = self.ctx.extra_components.len();
        match self.component(&inline_id, call)? {
            Some(compiled) => {
                let id = compiled["id"].clone();
                self.ctx.extra_components.insert(start, compiled);
                Ok(id)
            }
            None => Ok(inline_id.into()),
        }
    }

    // ---- function calls ---------------------------------------------------

    /// A function call as `{@call, catalogId?, args}`, and its
    /// `functionCallId` when `allow_call_id` (a standalone call).
    pub fn function_call(
        &mut self,
        name: &str,
        pos: &[&Expr],
        mut kw: Vec<(&str, &Expr)>,
        action: bool,
        allow_call_id: bool,
    ) -> Result<(Map<String, Value>, Option<String>), CompileError> {
        if name.starts_with('@') {
            if allow_call_id {
                return Err(CompileError::validation(format!(
                    "System function '{name}' cannot be called as a standalone statement, since a renderer function call names a catalog function."
                )));
            }
            return Ok((self.system_call(name, pos, kw, action)?, None));
        }
        let explicit_catalog = remove(&mut kw, "catalogId");
        let mut call_id = None;
        if allow_call_id {
            if let Some(raw) = remove(&mut kw, "functionCallId") {
                let Some(id) = string_literal(raw).filter(|s| !s.is_empty()) else {
                    return Err(CompileError::validation(format!(
                        "functionCallId on function '{name}' must be a non-empty string."
                    )));
                };
                call_id = Some(id.to_owned());
            }
        }
        let resolved = self
            .c
            .resolve(SymbolKind::Function, name, explicit_catalog)?;
        let args = self.call_args(resolved.catalog, name, pos, &kw, action)?;
        let mut out = Map::new();
        out.insert(self.ctx.call_key.into(), name.into());
        if let Some(cid) = resolved.catalog_id {
            out.insert("catalogId".into(), cid.into());
        }
        out.insert("args".into(), Value::Object(args));
        Ok((out, call_id))
    }

    /// A system function call, `@index(offset=1)`: no `catalogId`, and no
    /// `args` when there are none.
    fn system_call(
        &mut self,
        name: &str,
        pos: &[&Expr],
        kw: Vec<(&str, &Expr)>,
        action: bool,
    ) -> Result<Map<String, Value>, CompileError> {
        let cat = self
            .c
            .system_function(name, get(&kw, "catalogId").is_some())?;
        let args = self.call_args(cat, name, pos, &kw, action)?;
        let mut out = Map::new();
        out.insert(self.ctx.call_key.into(), name.into());
        if !args.is_empty() {
            out.insert("args".into(), Value::Object(args));
        }
        Ok(out)
    }

    /// Maps positional and keyword arguments to the function's parameters.
    fn call_args(
        &mut self,
        cat: &Catalog,
        name: &str,
        pos: &[&Expr],
        kw: &[(&str, &Expr)],
        action: bool,
    ) -> Result<Map<String, Value>, CompileError> {
        let params = cat.function_arguments(name);
        let mut out = Map::new();
        for (param, &arg) in params.iter().zip(pos) {
            if matches!(arg.kind, ExprKind::Skipped) {
                continue;
            }
            let v = self.value(arg, At::action(action))?;
            if !v.is_null() {
                out.insert(param.clone(), v);
            }
        }
        for &(k, arg) in kw {
            if !params.iter().any(|p| p == k) {
                return Err(CompileError::invalid_param(name, k, params));
            }
            if out.contains_key(k) {
                return Err(CompileError::duplicate_param(name, k));
            }
            if matches!(arg.kind, ExprKind::Skipped) {
                continue;
            }
            let v = self.value(arg, At::action(action))?;
            if !v.is_null() {
                out.insert(k.to_owned(), v);
            }
        }
        Ok(out)
    }

    /// `Event(name, context?, userMessage=?, context=?)`. Its name, message
    /// and context entries are values, not actions, so a function call
    /// among them stays a plain call.
    fn event(&mut self, args: &CallArgs<'_>) -> Result<Value, CompileError> {
        if args.pos.len() > 2 {
            return Err(CompileError::validation(format!(
                "Event accepts at most 2 positional arguments (name, context), got {}.",
                args.pos.len()
            )));
        }
        let keywords = EVENT_KEYWORDS.map(String::from);
        for &(k, _) in &args.kw {
            if !EVENT_KEYWORDS.contains(&k) {
                return Err(CompileError::invalid_param("Event", k, &keywords));
            }
        }
        if args.pos.len() == 2 && args.get("context").is_some() {
            return Err(CompileError::duplicate_param("Event", "context"));
        }

        let name = match args.pos.first() {
            Some(e) => self.plain_value(e)?,
            None => Value::String(String::new()),
        };
        let mut event = Map::new();
        event.insert("name".into(), name);

        if let Some(raw) = args.get("userMessage") {
            if !self.ctx.v1() {
                return Err(CompileError::validation(
                    "Event userMessage requires protocol v1.0 or later; the target version 0.9 has no userMessage field.",
                ));
            }
            // A DynamicString: a string, a data binding or a function call.
            let message = self.plain_value(raw)?;
            let binding_or_call = message.as_object().is_some_and(|m| {
                m.contains_key(self.ctx.path_key) || m.contains_key(self.ctx.call_key)
            });
            if !message.is_string() && !binding_or_call {
                return Err(CompileError::validation(format!(
                    "Event userMessage must be a string, a $path data binding or a function call, got {}.",
                    describe(raw)
                )));
            }
            event.insert("userMessage".into(), message);
        }

        let context = if args.pos.len() == 2 {
            Some(args.pos[1])
        } else {
            args.get("context")
        };
        if let Some(context) = context {
            let raw = self.plain_value(context)?;
            let mut compiled = Map::new();
            match raw {
                Value::Object(m) => compiled.extend(m),
                Value::Array(items) => {
                    for item in items {
                        if let Value::Object(m) = item {
                            compiled.extend(m);
                        }
                    }
                }
                _ => {}
            }
            event.insert("context".into(), Value::Object(compiled));
        }
        Ok(json!({"event": event}))
    }

    // ---- checks -----------------------------------------------------------

    /// `?name(args…)` as its condition (a function call) and message. The
    /// component's own binding of the function's first parameter is passed
    /// implicitly, a string where a non-string parameter is expected (or
    /// after the parameters) is the message, and a `{catalogId: …}` map names
    /// the function's catalog.
    fn check_call(
        &mut self,
        name: &str,
        raw_args: &[Expr],
        action: bool,
    ) -> Result<(Value, String), CompileError> {
        let mut remaining: Vec<&Expr> = Vec::new();
        let mut check_catalog: Option<&Expr> = None;
        for arg in raw_args {
            if let ExprKind::Map(entries) = &arg.kind {
                let entries = map_entries(entries);
                if let Some(cid) = get(&entries, "catalogId") {
                    let others: Vec<&str> = entries
                        .iter()
                        .map(|(k, _)| *k)
                        .filter(|k| *k != "catalogId")
                        .collect();
                    if !others.is_empty() {
                        return Err(CompileError::validation(format!(
                            "Unsupported key(s) in check options for '?{name}': {}. Only 'catalogId' is allowed.",
                            others.join(", ")
                        )));
                    }
                    if let Some(prev) = check_catalog {
                        let same = match (string_literal(prev), string_literal(cid)) {
                            (Some(a), Some(b)) => a == b,
                            _ => std::ptr::eq(prev, cid),
                        };
                        if !same {
                            return Err(CompileError::validation(format!(
                                "Check '?{name}' specifies conflicting catalogId values."
                            )));
                        }
                    }
                    check_catalog = Some(cid);
                    continue;
                }
            }
            remaining.push(arg);
        }

        let resolved = self.c.resolve(SymbolKind::Function, name, check_catalog)?;
        let cat = resolved.catalog;
        let params = cat.function_arguments(name);
        let mut chars = name.chars();
        let mut message = match chars.next() {
            Some(c) => format!("{}{} check failed", c.to_uppercase(), chars.as_str()),
            None => " check failed".to_owned(),
        };

        // The component's own bound value, as the first argument.
        let injected = params.first().and_then(|first| {
            let first_is_path = remaining
                .first()
                .is_some_and(|a| matches!(a.kind, ExprKind::Path(_)));
            if first_is_path {
                return None;
            }
            let bound = self.ctx.bound_paths.get(first)?;
            let raw = match bound.get(self.ctx.path_key) {
                Some(Value::String(s)) => s.clone(),
                Some(Value::Null) | None => String::new(),
                Some(other) => other.to_string(),
            };
            Some(Expr {
                kind: ExprKind::Path(raw),
                span: Default::default(),
            })
        });
        let mut call_args: Vec<&Expr> = Vec::new();
        if let Some(injected) = &injected {
            call_args.push(injected);
        }
        let start = usize::from(injected.is_some());
        for (i, &arg) in remaining.iter().enumerate() {
            match params.get(i + start) {
                Some(param) => {
                    let expects_non_string = cat
                        .function(name)
                        .and_then(|f| f.argument_schema(param))
                        .and_then(|s| s.get("type"))
                        .and_then(Value::as_str)
                        .is_some_and(|t| matches!(t, "integer" | "number" | "boolean"));
                    if let (Some(s), true) = (string_literal(arg), expects_non_string) {
                        message = s.to_owned();
                        break;
                    }
                    call_args.push(arg);
                }
                None => {
                    if let Some(s) = string_literal(arg) {
                        message = s.to_owned();
                    }
                }
            }
        }

        let mut kw = Vec::new();
        if let Some(cid) = check_catalog {
            kw.push(("catalogId", cid));
        }
        let (condition, _) = self.function_call(name, &call_args, kw, action, false)?;
        Ok((Value::Object(condition), message))
    }

    // ---- extensions -------------------------------------------------------

    /// Follows variables to the value they hold.
    fn resolve_ast_var<'e>(
        &self,
        mut node: &'e Expr,
        seen: &mut HashSet<String>,
    ) -> Result<&'e Expr, CompileError>
    where
        'a: 'e,
    {
        while let ExprKind::Variable(name) = &node.kind {
            if seen.contains(name) {
                return Err(CompileError::validation(format!(
                    "Variable '{name}' refers to itself through its own value."
                )));
            }
            let Some(value) = self.syms.get(name) else {
                return Err(CompileError::validation(format!(
                    "Undefined variable '{name}'."
                )));
            };
            seen.insert(name.clone());
            node = value;
        }
        Ok(node)
    }

    /// Rejects components and Express constructs inside `extensions`.
    fn check_extensions_ast(
        &self,
        node: &Expr,
        subject: &str,
        seen: &HashSet<String>,
    ) -> Result<(), CompileError> {
        let mut inner = seen.clone();
        let resolved = self.resolve_ast_var(node, &mut inner)?;
        match &resolved.kind {
            ExprKind::Map(entries) => {
                for (_, v) in map_entries(entries) {
                    self.check_extensions_ast(v, subject, &inner)?;
                }
            }
            ExprKind::List(items) => {
                for item in items {
                    self.check_extensions_ast(item, subject, &inner)?;
                }
            }
            ExprKind::Call(call) => {
                let args = CallArgs::of(call);
                let cat = args.get("catalogId");
                if self.c.defines_component(&call.name, cat) {
                    return Err(CompileError::validation(format!(
                        "extensions on {subject} cannot contain component '{}'.",
                        call.name
                    )));
                }
                if cat.is_none() && RESERVED_CALLS.contains(&call.name.as_str()) {
                    return Err(CompileError::validation(format!(
                        "extensions on {subject} cannot contain '{}()'.",
                        call.name
                    )));
                }
                for arg in &call.args {
                    self.check_extensions_ast(&arg.value, subject, &inner)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// An `extensions=` argument as a map with UAX #31 keys.
    pub fn extensions_map(
        &mut self,
        raw: &Expr,
        subject: &str,
    ) -> Result<Map<String, Value>, CompileError> {
        let not_a_map =
            || CompileError::validation(format!("extensions on {subject} must be a map."));
        let top = self.resolve_ast_var(raw, &mut HashSet::new())?;
        if !matches!(top.kind, ExprKind::Map(_)) {
            return Err(not_a_map());
        }
        self.check_extensions_ast(top, subject, &HashSet::new())?;
        let Value::Object(compiled) = self.plain_value(raw)? else {
            return Err(not_a_map());
        };
        if let Some(key) = compiled.keys().find(|k| !is_uax31_identifier(k)) {
            return Err(CompileError::validation(format!(
                "Extension key '{key}' on {subject} is not a valid UAX 31 identifier."
            )));
        }
        Ok(compiled)
    }
}

// ---- helpers ----------------------------------------------------------------

/// A check, or a non-empty list of checks.
fn is_check_expression(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Check { .. } => true,
        ExprKind::List(items) => !items.is_empty() && items.iter().all(is_check_expression),
        _ => false,
    }
}

fn collect_checks<'e>(e: &'e Expr, out: &mut Vec<&'e Expr>) {
    match &e.kind {
        ExprKind::Check { .. } => out.push(e),
        ExprKind::List(items) => out.extend(
            items
                .iter()
                .filter(|i| matches!(i.kind, ExprKind::Check { .. })),
        ),
        _ => {}
    }
}

/// JSON equality with numbers compared by value, as JavaScript compares them.
fn loose_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        _ => a == b,
    }
}

/// A scalar as JavaScript interpolates it into a string.
fn js_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => match n.as_f64() {
            Some(f) if f.fract() == 0.0 && f.abs() < 1e21 => format!("{}", f as i64),
            _ => n.to_string(),
        },
        other => other.to_string(),
    }
}

/// A parsed value, named in the terms the model wrote it in.
fn describe(e: &Expr) -> String {
    match &e.kind {
        ExprKind::Call(c) => format!("a call to {}(...)", c.name),
        ExprKind::Check { name, .. } => format!("a check (?{name})"),
        ExprKind::Variable(name) => format!("the variable {name}"),
        ExprKind::Path(p) => format!("the data binding ${p}"),
        ExprKind::Map(_) => "a map literal".into(),
        ExprKind::Skipped => "'_'".into(),
        ExprKind::List(_) => "a list".into(),
        ExprKind::Null => "null".into(),
        ExprKind::Bool(b) => format!("the boolean {b}"),
        ExprKind::Number(n) => format!("the number {}", js_string(&Value::Number(n.to_json()))),
        ExprKind::String(s) => format!("the string {}", Value::String(s.clone())),
    }
}

/// A parsed value for an error message: a string as it is, anything else as
/// the JSON of upstream's syntax tree.
fn format_value(e: &Expr) -> String {
    match &e.kind {
        ExprKind::String(s) => s.clone(),
        _ => ast_json(e).to_string(),
    }
}

/// Upstream's syntax tree for a value (`visitor.ts`).
fn ast_json(e: &Expr) -> Value {
    match &e.kind {
        ExprKind::Null => Value::Null,
        ExprKind::Bool(b) => Value::Bool(*b),
        ExprKind::Number(n) => Value::Number(n.to_json()),
        ExprKind::String(s) => Value::String(s.clone()),
        ExprKind::Path(p) => json!({"kind": "path", "path": p}),
        ExprKind::Variable(name) => json!({"kind": "variable", "name": name}),
        ExprKind::Skipped => json!({"kind": "skipped"}),
        ExprKind::Check { name, args } => json!({
            "kind": "check",
            "name": name,
            "args": args.iter().map(ast_json).collect::<Vec<_>>(),
        }),
        ExprKind::List(items) => Value::Array(items.iter().map(ast_json).collect()),
        ExprKind::Map(entries) => {
            let mut m = Map::new();
            for (k, v) in map_entries(entries) {
                m.insert(k.to_owned(), ast_json(v));
            }
            json!({"kind": "map", "entries": m})
        }
        ExprKind::Call(call) => {
            let args = CallArgs::of(call);
            let mut kwargs = Map::new();
            for (k, v) in &args.kw {
                kwargs.insert((*k).to_owned(), ast_json(v));
            }
            json!({
                "kind": "call",
                "name": call.name,
                "args": args.pos.iter().map(|a| ast_json(a)).collect::<Vec<_>>(),
                "kwargs": kwargs,
            })
        }
    }
}
