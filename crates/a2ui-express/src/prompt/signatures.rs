//! Positional signatures of a catalog's components and functions.

use serde_json::{Map, Value};

use crate::schema::{Catalog, Component, common_def_name, is_object_or_ref_schema};

/// How deep the union walks go; stands in for upstream's visited sets, which
/// guard against `$ref` cycles.
const MAX_DEPTH: usize = 32;

/// The schema with its local `$ref` chain resolved, or the value itself when
/// it is not an object (upstream's `dereferenceLocalSchema(s) ?? s`).
fn deref(catalog: &Catalog, schema: &Value) -> Value {
    catalog
        .dereference_local_schema(schema)
        .unwrap_or_else(|| schema.clone())
}

pub(super) fn str_field<'v>(schema: &'v Value, key: &str) -> Option<&'v str> {
    schema.get(key).and_then(Value::as_str)
}

fn is_object_typed(s: &Value) -> bool {
    match s.get("type") {
        None => true,
        Some(t) => t == "object",
    }
}

/// The non-empty `properties` of an object schema.
fn own_properties(s: &Value) -> Option<&Map<String, Value>> {
    if !is_object_typed(s) {
        return None;
    }
    s.get("properties")
        .and_then(Value::as_object)
        .filter(|p| !p.is_empty())
}

/// The allowed values of an enum, searching unions.
pub(super) fn schema_enum(catalog: &Catalog, schema: &Value, depth: usize) -> Option<Vec<Value>> {
    if !schema.is_object() || depth > MAX_DEPTH {
        return None;
    }
    let s = deref(catalog, schema);
    if let Some(Value::Array(values)) = s.get("enum") {
        return Some(values.clone());
    }
    for key in ["oneOf", "anyOf", "allOf"] {
        for sub in s.get(key).and_then(Value::as_array).into_iter().flatten() {
            if let Some(values) = schema_enum(catalog, sub, depth + 1) {
                return Some(values);
            }
        }
    }
    None
}

/// Whether an object schema is a protocol wire envelope (a data binding, a
/// function call, a child-list template or an `Action` branch) rather than a
/// map the catalog defines.
fn is_wire_envelope(catalog: &Catalog, s: &Value, depth: usize) -> bool {
    if str_field(s, "$ref").and_then(common_def_name).is_some() {
        return true;
    }
    if depth > MAX_DEPTH {
        return false;
    }
    if let Some(Value::Array(all_of)) = s.get("allOf") {
        let any = all_of.iter().any(|sub| {
            sub.is_object() && is_wire_envelope(catalog, &deref(catalog, sub), depth + 1)
        });
        if any {
            return true;
        }
    }
    let (Some(Value::Object(props)), Some(Value::Array(reqs))) =
        (s.get("properties"), s.get("required"))
    else {
        return false;
    };
    let requires = |k: &str| reqs.iter().any(|r| r == k);
    if props.len() == 1 {
        let (k, v) = props.iter().next().expect("one property");
        if k == catalog.path_key() && requires(k) {
            return true;
        }
        if (k == "event" || k == "functionCall") && requires(k) {
            return is_object_or_ref_schema(v);
        }
        return false;
    }
    if catalog.call_key() == "@call" {
        if props.contains_key("@call") && requires("@call") {
            return true;
        }
    } else if props.contains_key("call")
        && (props.contains_key("args") || props.contains_key("returnType"))
        && requires("call")
    {
        return true;
    }
    props.len() == 2
        && props.contains_key("componentId")
        && props.contains_key("path")
        && requires("componentId")
        && requires("path")
}

/// The keys of an object schema, merging `allOf` and searching unions.
fn object_properties(
    catalog: &Catalog,
    schema: &Value,
    depth: usize,
) -> Option<Map<String, Value>> {
    if !schema.is_object() || depth > MAX_DEPTH {
        return None;
    }
    let s = deref(catalog, schema);
    if is_wire_envelope(catalog, &s, 0) {
        return None;
    }
    if let Some(Value::Array(all_of)) = s.get("allOf") {
        let mut merged = Map::new();
        for sub in all_of {
            if let Some(props) = object_properties(catalog, sub, depth + 1) {
                merged.extend(props);
            }
        }
        if let Some(Value::Object(props)) = s.get("properties") {
            merged.extend(props.clone());
        }
        if !merged.is_empty() {
            return Some(merged);
        }
    }
    if let Some(props) = own_properties(&s) {
        return Some(props.clone());
    }
    for key in ["oneOf", "anyOf"] {
        for sub in s.get(key).and_then(Value::as_array).into_iter().flatten() {
            if let Some(props) = object_properties(catalog, sub, depth + 1) {
                return Some(props);
            }
        }
    }
    None
}

/// A property that holds a map, or a list of maps, with keys the model has
/// to be told.
pub(super) enum Structured {
    Map(Map<String, Value>),
    ListOfMaps(Map<String, Value>),
}

pub(super) fn structured_property(
    catalog: &Catalog,
    schema: &Value,
    depth: usize,
) -> Option<Structured> {
    if !schema.is_object() || depth > MAX_DEPTH {
        return None;
    }
    let s = deref(catalog, schema);
    if is_wire_envelope(catalog, &s, 0) {
        return None;
    }
    if s.get("allOf").is_some_and(Value::is_array) {
        if let Some(props) = object_properties(catalog, schema, 0) {
            return Some(Structured::Map(props));
        }
    }
    if let Some(props) = own_properties(&s) {
        return Some(Structured::Map(props.clone()));
    }
    if s.get("type").is_some_and(|t| t == "array") {
        if let Some(items) = s.get("items").filter(|i| i.is_object()) {
            if let Some(props) = object_properties(catalog, items, 0) {
                return Some(Structured::ListOfMaps(props));
            }
        }
    }
    for key in ["oneOf", "anyOf", "allOf"] {
        for sub in s.get(key).and_then(Value::as_array).into_iter().flatten() {
            if let Some(found) = structured_property(catalog, sub, depth + 1) {
                return Some(found);
            }
        }
    }
    None
}

fn sub_key_lines(catalog: &Catalog, properties: &Map<String, Value>) -> String {
    properties
        .iter()
        .map(|(key, schema)| {
            let desc = catalog
                .dereference_local_schema(schema)
                .and_then(|s| {
                    s.get("description")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .unwrap_or_default();
            if desc.is_empty() {
                format!("    * {key}")
            } else {
                format!("    * {key} - {desc}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn enum_value(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn property_detail(catalog: &Catalog, name: &str, schema: Option<&Value>) -> Option<String> {
    let schema = schema.filter(|s| s.is_object())?;
    let enum_values = schema_enum(catalog, schema, 0);
    let mut parts = Vec::new();
    if let Some(desc) = str_field(schema, "description").filter(|d| !d.is_empty()) {
        parts.push(desc.to_owned());
    }
    if let Some(values) = &enum_values {
        let listed = values
            .iter()
            .map(|v| format!("'{}'", enum_value(v)))
            .collect::<Vec<_>>()
            .join(", ");
        parts.push(format!("Must be one of: {listed}"));
    }
    let mut detail = (!parts.is_empty()).then(|| format!("  - {name}: {}", parts.join(" ")));
    if enum_values.is_none() {
        match structured_property(catalog, schema, 0) {
            Some(Structured::Map(props)) => {
                let keys = sub_key_lines(catalog, &props);
                detail = Some(match detail {
                    Some(d) => format!("{d}\n    Map keys:\n{keys}"),
                    None => format!("  - {name}: Map with keys:\n{keys}"),
                });
            }
            Some(Structured::ListOfMaps(props)) => {
                let keys = sub_key_lines(catalog, &props);
                detail = Some(match detail {
                    Some(d) => format!("{d}\n    List of maps keys:\n{keys}"),
                    None => format!("  - {name}: List of maps with keys:\n{keys}"),
                });
            }
            None => {}
        }
    }
    detail
}

pub(super) fn entry(
    name: &str,
    args: &[String],
    description: Option<&str>,
    details: &[String],
) -> String {
    let mut sig = format!("• {name}({})", args.join(", "));
    if let Some(desc) = description.filter(|d| !d.is_empty()) {
        sig.push_str("\n  - Description: ");
        sig.push_str(&desc.replace('\n', "\n    "));
    }
    if !details.is_empty() {
        sig.push('\n');
        sig.push_str(&details.join("\n"));
    }
    sig
}

/// A component property as its signature lists it: `name`, `?` when
/// optional, then `(component ID)` or `(static)` when it takes no binding.
pub(super) fn argument_label(catalog: &Catalog, component: &Component, p: &str) -> String {
    let schema = component.property_schema(p);
    let mut label = format!("{p}{}", if component.is_required(p) { "" } else { "?" });
    let is_component_id = schema
        .and_then(|s| str_field(s, "$ref"))
        .and_then(common_def_name)
        == Some("ComponentId");
    if is_component_id {
        label.push_str(" (component ID)");
    } else if !schema.is_some_and(|s| catalog.admits_path(s)) {
        label.push_str(" (static)");
    }
    label
}

/// One signature per component, sorted by name.
pub(super) fn components(catalog: &Catalog) -> String {
    let mut list: Vec<_> = catalog.components().iter().collect();
    list.sort_by(|a, b| a.name().cmp(b.name()));
    list.iter()
        .map(|component| {
            let mut args = Vec::new();
            let mut details = Vec::new();
            for p in component.properties() {
                let schema = component.property_schema(p);
                args.push(argument_label(catalog, component, p));
                if let Some(detail) = property_detail(catalog, p, schema) {
                    details.push(detail);
                }
            }
            entry(component.name(), &args, component.description(), &details)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// One signature per named function (catalog or system), in the order given.
pub(super) fn functions<'n>(catalog: &Catalog, names: impl IntoIterator<Item = &'n str>) -> String {
    names
        .into_iter()
        .filter_map(|name| catalog.function(name))
        .map(|function| {
            let mut args = Vec::new();
            let mut details = Vec::new();
            for p in function.arguments() {
                args.push(format!(
                    "{p}{}",
                    if function.is_required(p) { "" } else { "?" }
                ));
                if let Some(desc) = function
                    .argument_schema(p)
                    .and_then(|s| str_field(s, "description"))
                    .filter(|d| !d.is_empty())
                {
                    details.push(format!("  - {p}: {desc}"));
                }
            }
            entry(function.name(), &args, function.description(), &details)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The catalog's own functions, sorted by name.
pub(super) fn catalog_functions(catalog: &Catalog) -> String {
    let mut names: Vec<&str> = catalog.functions().iter().map(|f| f.name()).collect();
    names.sort_unstable();
    functions(catalog, names)
}
