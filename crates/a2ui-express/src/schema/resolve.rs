//! Walking schemas: `$ref`s into the common types and the catalog's own
//! `#/...` (including `#/$defs/*`), `allOf`/`oneOf`/`anyOf`, array `items` and
//! object `properties`, with cycle tracking.
//!
//! Cycle tracking is by identity, as upstream does: a set of the addresses of
//! the schema values already visited. Every value walked is borrowed from the
//! catalog, the common types or the caller's schema, so addresses stay valid
//! for the whole walk. Dereferencing a local `$ref` therefore yields a
//! [`View`] over borrowed layers instead of a merged copy.

use std::collections::HashSet;

use serde_json::{Map, Value};

use super::common::defs_of;
use super::refs::{
    common_def_name, common_def_of, local_def_name, local_ref, resolve_json_pointer,
};

const COMBINATORS: [&str; 3] = ["allOf", "oneOf", "anyOf"];
const COMBINATORS_ONE_FIRST: [&str; 3] = ["oneOf", "anyOf", "allOf"];

type Visited = HashSet<*const Value>;

/// Marks `v` visited; true if it already was.
fn seen(visited: &mut Visited, v: &Value) -> bool {
    !visited.insert(v as *const Value)
}

/// The members of combinator `key` of a schema, or nothing.
fn members(get: Option<&Value>) -> &[Value] {
    get.and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

/// What a component property, or a position inside its value, is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SlotKind {
    /// One component id (`ComponentId` or `Child`).
    Child,
    /// A list of child ids, or a template (`ChildList`).
    ChildList,
    /// An interaction handler (`Action`).
    Action,
}

/// One step into a value: an array item, or an object property.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Segment<'s> {
    /// Any item of an array (`items`).
    Item,
    /// A property of an object (`properties[name]`, else `additionalProperties`).
    Property(&'s str),
}

/// A schema with its chain of local `$ref`s dereferenced.
///
/// Reads like upstream's merged object `{...target, ...overrides}`: a key is
/// read from the outermost layer that has it. `$ref` is the innermost
/// target's own `$ref`, or, when it has none, the last consumed reference if
/// that names a standard common type (so `#/$defs/ChildList` stays
/// recognizable after it is followed).
#[derive(Debug, Clone)]
pub(crate) struct View<'x> {
    /// Outermost (the schema itself) first.
    layers: Vec<&'x Map<String, Value>>,
    reference: Option<&'x Value>,
    innermost_ref: bool,
}

impl<'x> View<'x> {
    pub(crate) fn get(&self, key: &str) -> Option<&'x Value> {
        if key == "$ref" {
            return self.reference;
        }
        self.layers.iter().find_map(|m| m.get(key))
    }

    fn ref_str(&self) -> Option<&'x str> {
        self.reference.and_then(Value::as_str)
    }

    /// The merged object, in upstream's key order.
    pub(crate) fn to_value(&self) -> Value {
        let mut out = Map::new();
        let innermost = self.layers.len() - 1;
        for (i, layer) in self.layers.iter().enumerate().rev() {
            for (k, v) in layer.iter() {
                if k == "$ref" && !(i == innermost && self.innermost_ref) {
                    continue;
                }
                out.insert(k.clone(), v.clone());
            }
        }
        if let (Some(r), false) = (self.reference, self.innermost_ref) {
            out.insert("$ref".into(), r.clone());
        }
        Value::Object(out)
    }
}

/// A read-only view of a catalog document and its common types.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Schemas<'a> {
    pub doc: &'a Value,
    pub common: &'a Value,
    /// The binding key: `@path`.
    pub path_key: &'static str,
}

impl<'a> Schemas<'a> {
    pub(crate) fn common_def(&self, name: &str) -> Option<&'a Value> {
        defs_of(self.common).and_then(|d| d.get(name))
    }

    pub(crate) fn pointer(&self, pointer: &str) -> Option<&'a Value> {
        resolve_json_pointer(self.doc, pointer)
    }

    /// The target of a local `#/...` reference, if `sub` is one that resolves.
    pub(crate) fn resolve_local_ref(&self, sub: &Value) -> Option<&'a Value> {
        local_ref(sub).and_then(|r| self.pointer(r))
    }

    /// Follows the chain of local `$ref`s on a schema object.
    pub(crate) fn view<'x>(&self, schema: &'x Value) -> Option<View<'x>>
    where
        'a: 'x,
    {
        let first = schema.as_object()?;
        let mut layers = vec![first];
        let mut pointers: HashSet<&'x str> = HashSet::new();
        let mut consumed: Option<&'x Value> = None;
        let mut current: &'x Value = schema;
        while let Some(r) = local_ref(current) {
            if !pointers.insert(r) {
                break;
            }
            let Some(target) = self.pointer(r).filter(|t| t.is_object()) else {
                break;
            };
            consumed = current.get("$ref");
            layers.push(target.as_object().expect("checked object"));
            current = target;
        }
        let innermost = *layers.last().expect("one layer");
        let (reference, innermost_ref) = match innermost.get("$ref") {
            Some(r) => (Some(r), true),
            None => (
                consumed.filter(|c| c.as_str().and_then(common_def_name).is_some()),
                false,
            ),
        };
        Some(View {
            layers,
            reference,
            innermost_ref,
        })
    }

    /// The schema with its local `$ref` chain resolved and the outer keys
    /// (such as `description`) laid over the target.
    pub(crate) fn dereference(&self, schema: &Value) -> Option<Value> {
        self.view(schema).map(|v| v.to_value())
    }

    /// Resolves a `$ref` seen while walking, with whether the target lives in
    /// the common types.
    fn follow<'x>(&self, reference: &str, in_common: bool) -> Option<(&'x Value, bool)>
    where
        'a: 'x,
    {
        if let Some(name) = common_def_name(reference) {
            return self.common_def(name).map(|t| (t, true));
        }
        if in_common {
            if let Some(name) = local_def_name(reference) {
                return self.common_def(name).map(|t| (t, true));
            }
        }
        if reference.starts_with("#/") {
            return self.pointer(reference).map(|t| (t, false));
        }
        None
    }

    /// Whether a schema admits a data binding.
    ///
    /// True when the schema is, or combines via `$ref`/`oneOf`/`anyOf`/`allOf`,
    /// an object schema whose only property is the required binding key
    /// (`@path`), or the common types' child-list
    /// template (`componentId` + `path`). Never descends into `items` or
    /// property values: see [`Schemas::schema_at`] for positions inside a value.
    pub(crate) fn admits_path(&self, schema: &Value) -> bool {
        self.admits_path_in(schema, false, &mut Visited::new())
    }

    fn admits_path_in(&self, s: &Value, in_common: bool, visited: &mut Visited) -> bool {
        if !s.is_object() || seen(visited, s) {
            return false;
        }
        let ty = s.get("type");
        if let (None | Some(Value::String(_)), Some(Value::Object(props))) =
            (ty, s.get("properties"))
        {
            let object_type = ty.is_none_or(|t| t == "object");
            let reqs = s.get("required").and_then(Value::as_array);
            let requires = |k: &str| reqs.is_some_and(|r| r.iter().any(|x| x == k));
            if object_type {
                if props.len() == 1 && props.contains_key(self.path_key) && requires(self.path_key)
                {
                    return true;
                }
                if in_common
                    && props.len() == 2
                    && props.contains_key("componentId")
                    && props.contains_key("path")
                    && requires("componentId")
                    && requires("path")
                {
                    return true;
                }
            }
        }
        if let Some(r) = s.get("$ref").and_then(Value::as_str) {
            if let Some((target, common)) = self.follow(r, in_common) {
                if self.admits_path_in(target, common, visited) {
                    return true;
                }
            }
        }
        COMBINATORS.iter().any(|k| {
            members(s.get(*k))
                .iter()
                .any(|sub| self.admits_path_in(sub, in_common, visited))
        })
    }

    /// Whether a property schema accepts a number. Unknown or unconstrained
    /// schemas accept one.
    pub(crate) fn accepts_number(&self, schema: &Value) -> bool {
        self.accepts_number_in(schema, false, &mut Visited::new())
    }

    fn accepts_number_in(&self, s: &Value, in_common: bool, visited: &mut Visited) -> bool {
        if !s.is_object() || seen(visited, s) {
            return false;
        }
        match s.get("type") {
            Some(Value::String(t)) if t == "number" || t == "integer" => return true,
            Some(Value::Array(ts)) if ts.iter().any(|t| t == "number" || t == "integer") => {
                return true;
            }
            _ => {}
        }
        if s.get("const").is_some_and(Value::is_number) {
            return true;
        }
        if members(s.get("enum")).iter().any(Value::is_number) {
            return true;
        }
        if let Some(r) = s.get("$ref").and_then(Value::as_str) {
            let target = if common_def_name(r).is_some()
                || (in_common && local_def_name(r).is_some())
                || r.starts_with("#/")
            {
                self.follow(r, in_common)
            } else if let Some(pointer) = r.strip_prefix("catalog.json") {
                if pointer.starts_with("#/") {
                    self.pointer(pointer).map(|t| (t, false))
                } else {
                    return true;
                }
            } else {
                return true;
            };
            match target {
                None => return true,
                Some((t, common)) => {
                    if self.accepts_number_in(t, common, visited) {
                        return true;
                    }
                }
            }
        }
        if COMBINATORS.iter().any(|k| {
            members(s.get(*k))
                .iter()
                .any(|sub| self.accepts_number_in(sub, in_common, visited))
        }) {
            return true;
        }
        const TYPE_KEYS: [&str; 11] = [
            "type",
            "const",
            "enum",
            "$ref",
            "allOf",
            "oneOf",
            "anyOf",
            "properties",
            "items",
            "prefixItems",
            "not",
        ];
        !TYPE_KEYS.iter().any(|k| s.get(*k).is_some())
    }

    /// The schema declared one step into a value: the items of an array
    /// (`Segment::Item`) or a property of an object, falling back to
    /// `additionalProperties`. Follows `$ref`s and combinators; the first
    /// declaration found wins. A property named `items` is a property.
    pub(crate) fn resolve_at<'x>(&self, schema: &'x Value, step: Segment<'_>) -> Option<&'x Value>
    where
        'a: 'x,
    {
        self.resolve_at_in(schema, step, false, &mut Visited::new())
    }

    fn resolve_at_in<'x>(
        &self,
        s: &'x Value,
        step: Segment<'_>,
        in_common: bool,
        visited: &mut Visited,
    ) -> Option<&'x Value>
    where
        'a: 'x,
    {
        if !s.is_object() || seen(visited, s) {
            return None;
        }
        match step {
            Segment::Item => {
                if let Some(items) = s.get("items").filter(|i| i.is_object()) {
                    return Some(items);
                }
            }
            Segment::Property(name) => {
                if let Some(declared) = s
                    .get("properties")
                    .and_then(|p| p.get(name))
                    .filter(|d| d.is_object())
                {
                    return Some(declared);
                }
                if let Some(extra) = s.get("additionalProperties").filter(|a| a.is_object()) {
                    return Some(extra);
                }
            }
        }
        if let Some(r) = s.get("$ref").and_then(Value::as_str) {
            if let Some((target, common)) = self.follow(r, in_common) {
                if let Some(found) = self.resolve_at_in(target, step, common, visited) {
                    return Some(found);
                }
            }
        }
        for k in COMBINATORS {
            for sub in members(s.get(k)) {
                if let Some(found) = self.resolve_at_in(sub, step, in_common, visited) {
                    return Some(found);
                }
            }
        }
        None
    }

    /// The schema at a position inside a value, or `None` when the schema does
    /// not declare one there.
    pub(crate) fn schema_at<'x>(&self, schema: &'x Value, path: &[Segment<'_>]) -> Option<&'x Value>
    where
        'a: 'x,
    {
        path.iter()
            .try_fold(schema, |s, step| self.resolve_at(s, *step))
    }

    /// Whether a property schema is an `Action` slot: it references the
    /// common `Action`, or declares a flattened branch (`{event}` or
    /// `{functionCall}`), directly or through a combinator.
    pub(crate) fn is_action_slot(&self, schema: &Value) -> bool {
        self.is_action_slot_in(schema, &mut Visited::new())
    }

    fn is_action_slot_in(&self, schema: &Value, visited: &mut Visited) -> bool {
        if !schema.is_object() || seen(visited, schema) {
            return false;
        }
        let Some(s) = self.view(schema) else {
            return false;
        };
        if s.ref_str().and_then(common_def_name) == Some("Action")
            || is_flattened_action(&s)
            || is_combined_action(&s)
        {
            return true;
        }
        COMBINATORS.iter().any(|k| {
            members(s.get(k))
                .iter()
                .any(|sub| self.is_action_slot_in(sub, visited))
        })
    }

    /// Whether a property expects a list of option objects (items with both
    /// `label` and `value` properties).
    pub(crate) fn expects_option_objects(&self, schema: &Value) -> bool {
        self.expects_option_objects_in(schema, &mut Visited::new())
    }

    fn expects_option_objects_in(&self, schema: &Value, visited: &mut Visited) -> bool {
        if !schema.is_object() || seen(visited, schema) {
            return false;
        }
        let Some(s) = self.view(schema) else {
            return false;
        };
        if let Some(items) = s.get("items") {
            return self.has_label_value(items, &mut Visited::new());
        }
        COMBINATORS.iter().any(|k| {
            members(s.get(k))
                .iter()
                .any(|sub| self.expects_option_objects_in(sub, visited))
        })
    }

    fn has_label_value(&self, sub: &Value, visited: &mut Visited) -> bool {
        if !sub.is_object() || seen(visited, sub) {
            return false;
        }
        let Some(s) = self.view(sub) else {
            return false;
        };
        if let Some(Value::Object(props)) = s.get("properties") {
            if props.contains_key("label") && props.contains_key("value") {
                return true;
            }
        }
        COMBINATORS.iter().any(|k| {
            members(s.get(k))
                .iter()
                .any(|m| self.has_label_value(m, visited))
        })
    }

    /// What a property is for, read from the names of the definitions its
    /// schema references (as upstream's compiler does).
    pub(crate) fn slot_kind(&self, schema: &Value) -> Option<SlotKind> {
        self.slot_kind_in(schema, &mut Visited::new())
    }

    fn slot_kind_in(&self, schema: &Value, visited: &mut Visited) -> Option<SlotKind> {
        if !schema.is_object() || seen(visited, schema) {
            return None;
        }
        let s = self.view(schema)?;
        if let Some(kind) = s.ref_str().and_then(classify_ref) {
            return Some(kind);
        }
        COMBINATORS_ONE_FIRST.iter().find_map(|k| {
            members(s.get(k))
                .iter()
                .find_map(|sub| self.slot_kind_in(sub, visited))
        })
    }

    /// Whether a property holds component ids: it references `ComponentId`,
    /// `Child` or `ChildList` exactly, through combinators or array `items`.
    pub(crate) fn is_component_reference(&self, schema: &Value) -> bool {
        self.is_component_reference_in(schema, &mut Visited::new())
    }

    fn is_component_reference_in(&self, schema: &Value, visited: &mut Visited) -> bool {
        if !schema.is_object() || seen(visited, schema) {
            return false;
        }
        let Some(s) = self.view(schema) else {
            return false;
        };
        if matches!(
            s.ref_str().and_then(common_def_name),
            Some("ComponentId" | "Child" | "ChildList")
        ) {
            return true;
        }
        if COMBINATORS_ONE_FIRST.iter().any(|k| {
            members(s.get(k))
                .iter()
                .any(|sub| self.is_component_reference_in(sub, visited))
        }) {
            return true;
        }
        match (s.get("type"), s.get("items")) {
            (Some(t), Some(items)) if t == "array" => {
                self.is_component_reference_in(items, visited)
            }
            _ => false,
        }
    }

    /// The enum a property declares, through local `$ref`s and combinators.
    pub(crate) fn find_enum(&self, schema: &Value) -> Option<Vec<Value>> {
        self.find_enum_in(schema, &mut Visited::new())
    }

    fn find_enum_in(&self, schema: &Value, visited: &mut Visited) -> Option<Vec<Value>> {
        if !schema.is_object() || seen(visited, schema) {
            return None;
        }
        let mut obj = schema;
        let mut pointers: HashSet<&str> = HashSet::new();
        while let Some(r) = local_ref(obj) {
            if !pointers.insert(r) {
                break;
            }
            match self.pointer(r).filter(|t| t.is_object()) {
                Some(target) => {
                    if seen(visited, target) {
                        return None;
                    }
                    obj = target;
                }
                None => break,
            }
        }
        if let Some(Value::Array(values)) = obj.get("enum") {
            return Some(values.clone());
        }
        COMBINATORS_ONE_FIRST.iter().find_map(|k| {
            members(obj.get(*k))
                .iter()
                .find_map(|sub| self.find_enum_in(sub, visited))
        })
    }

    /// Whether `schema` is `{type: array, items: {$ref: <common CheckRule>}}`.
    pub(crate) fn is_check_rule_array(schema: &Value) -> bool {
        schema.get("type").is_some_and(|t| t == "array")
            && schema
                .get("items")
                .filter(|i| i.is_object())
                .and_then(common_def_of)
                == Some("CheckRule")
    }
}

fn classify_ref(reference: &str) -> Option<SlotKind> {
    if reference.contains("ChildList") {
        Some(SlotKind::ChildList)
    } else if reference.contains("Child") || reference.contains("ComponentId") {
        Some(SlotKind::Child)
    } else if reference.contains("Action") {
        Some(SlotKind::Action)
    } else {
        None
    }
}

/// Whether `sub` declares an object, a `$ref` or a combinator.
pub(crate) fn is_object_or_ref_schema(sub: &Value) -> bool {
    let Some(s) = sub.as_object() else {
        return false;
    };
    s.contains_key("$ref")
        || s.get("type").is_some_and(|t| t == "object")
        || s.get("properties").is_some_and(Value::is_object)
        || COMBINATORS.iter().any(|k| s.contains_key(*k))
}

fn is_flattened_action(s: &View<'_>) -> bool {
    if s.get("type").is_some_and(|t| t != "object") {
        return false;
    }
    let (Some(Value::Object(props)), Some(Value::Array(reqs))) =
        (s.get("properties"), s.get("required"))
    else {
        return false;
    };
    if props.len() == 1 {
        let (key, value) = props.iter().next().expect("one property");
        return (key == "event" || key == "functionCall")
            && reqs.iter().any(|r| r == key.as_str())
            && is_object_or_ref_schema(value);
    }
    false
}

/// An action written as one object with both branches as optional
/// properties (`{event?, functionCall?}`, at least one required through
/// `anyOf`/`oneOf`), as eui's `EuiAction` is: either or both may be given.
fn is_combined_action(s: &View<'_>) -> bool {
    if s.get("type").is_some_and(|t| t != "object") {
        return false;
    }
    let Some(Value::Object(props)) = s.get("properties") else {
        return false;
    };
    if props.len() != 2 {
        return false;
    }
    let (Some(event), Some(call)) = (props.get("event"), props.get("functionCall")) else {
        return false;
    };
    let requires = |key: &str| {
        ["anyOf", "oneOf"].iter().any(|k| {
            members(s.get(k)).iter().any(|m| {
                m.get("required")
                    .and_then(Value::as_array)
                    .is_some_and(|r| r.iter().any(|x| x == key))
            })
        })
    };
    is_object_or_ref_schema(event)
        && is_object_or_ref_schema(call)
        && requires("event")
        && requires("functionCall")
}
