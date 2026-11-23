//! The compact signatures ([`PromptStyle::Compact`](super::PromptStyle)).
//!
//! The upstream format spells out every enum and every map's keys under each
//! property that uses them, so a catalog whose components share a style map
//! or an icon enum repeats them once per component. The compact format:
//!
//! - names each enum or map that several properties use and that is long
//!   to spell out (after its `$defs` entry when it has one, else after the
//!   property) and lists it once, under `### Shared Types`; a map that has
//!   another's keys but a few reads `Other without a, b`;
//! - lists a long enum (an icon registry) with runs of values that share a
//!   prefix grouped, `ui-{add|back}`;
//! - lists once, under `### Common Properties`, a property line several
//!   components share, and leaves out of the signatures the properties
//!   nearly every component takes;
//! - marks the properties that take a data binding with `~` instead of
//!   marking the others `(static)`, shows one bound beside one literal-only
//!   property, and writes an action slot `Action`;
//! - cuts component descriptions to their first sentence and leaves out
//!   property, argument and function descriptions.

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};

use super::signatures::{Structured, enum_value, schema_enum, str_field, structured_property};
use crate::schema::{Catalog, Component, Function, SlotKind, common_def_name};

/// How deep nested map keys are spelled out inline.
const MAX_INLINE_DEPTH: usize = 6;

/// A shape several properties use gets a name only when spelling it out is
/// longer than this; a short one reads better inline.
const SHARED_TYPE_MIN_CHARS: usize = 40;

/// A property line shared by at least this many components is listed once,
/// under `### Common Properties`.
const COMMON_PROPERTY_MIN_COMPONENTS: usize = 3;

/// A property (same label, same type) that at least this share of the
/// components take is left out of the signatures and listed once.
const UNIVERSAL_PROPERTY_SHARE: f64 = 0.75;

/// Properties are left out of the signatures as universal only in a
/// catalog with at least this many components.
const UNIVERSAL_MIN_COMPONENTS: usize = 8;

/// The heading and lead-in of the compact component signatures.
pub(super) const COMPONENTS_HEADING: &str = "## Component Signatures\n\nInstantiate components with these signatures, naming every argument (`Button(label=\"Save\", variant=\"outline\")`, `Stack(children=[a, b])`). Do not name any other key.";

/// The legend printed before the compact signatures. `literal` is a
/// component call that binds one property and gives another, which takes
/// no binding, a literal (`Progress(value=$/progress, max=5)`); `folded` says a long
/// enum groups values by prefix.
fn legend(literal: Option<&str>, folded: bool) -> String {
    let example = literal.map_or(String::new(), |c| format!(": {c}"));
    let fold = if folded {
        " In a long list, `pre-{a|b}` stands for pre-a|pre-b."
    } else {
        ""
    };
    format!(
        "Notation: `name?` is optional; `name~` also takes a data binding ($/path) or a function call; a property without `~` takes a literal only, never $ or a call{example}. `:n` marks a number (write 1.5, not \"1.5\"), `:b` a boolean. Under a signature, `- prop: type` gives a property's values: `a|b|c` is one of these strings (write \"a\"), `{{key, key: type}}` is a map with those keys (`{{a|b: type}}`: keys a and b, both of that type), `[type]` a list, `Action` an Event(...) or function call, and a capitalized name a type from Shared Types.{fold}"
    )
}

/// A shape the model has to be told: the allowed values of an enum, the
/// keys of a map (a list of maps is its item map), or an action.
enum Shape {
    Enum(Vec<Value>),
    Map(Map<String, Value>, bool),
    Action,
}

impl Shape {
    fn of(catalog: &Catalog, schema: &Value) -> Option<Shape> {
        if !schema.is_object() {
            return None;
        }
        if catalog.is_action_slot(schema) || is_action_map(catalog, schema) {
            return Some(Shape::Action);
        }
        if let Some(values) = schema_enum(catalog, schema, 0) {
            return Some(Shape::Enum(values));
        }
        match structured_property(catalog, schema, 0)? {
            Structured::Map(props) => Some(Shape::Map(props, false)),
            Structured::ListOfMaps(props) => Some(Shape::Map(props, true)),
        }
    }

    /// Identifies a shape by its content, so identical inline copies count
    /// as one type.
    fn key(&self) -> String {
        match self {
            Shape::Enum(values) => format!("e{}", Value::Array(values.clone())),
            Shape::Map(props, _) => format!("m{}", Value::Object(props.clone())),
            Shape::Action => "a".into(),
        }
    }
}

/// Whether a catalog-defined map is an action: its keys are `event` and
/// `functionCall` (one or both), which `Event(...)` and a function call
/// write.
fn is_action_map(catalog: &Catalog, schema: &Value) -> bool {
    let s = catalog
        .dereference_local_schema(schema)
        .unwrap_or_else(|| schema.clone());
    s.get("properties")
        .and_then(Value::as_object)
        .is_some_and(|p| !p.is_empty() && p.keys().all(|k| k == "event" || k == "functionCall"))
}

/// The `$defs` entry a schema (or its list's items) refers to.
fn def_name(schema: &Value) -> Option<&str> {
    fn local(s: &Value) -> Option<&str> {
        str_field(s, "$ref").and_then(|r| r.strip_prefix("#/$defs/"))
    }
    local(schema).or_else(|| schema.get("items").and_then(local))
}

fn pascal_case(name: &str) -> String {
    let mut out = String::new();
    let mut upper = true;
    for c in name.chars() {
        if c.is_alphanumeric() {
            if upper {
                out.extend(c.to_uppercase());
            } else {
                out.push(c);
            }
            upper = false;
        } else {
            upper = true;
        }
    }
    if out.is_empty() { "Type".into() } else { out }
}

fn enum_text(values: &[Value]) -> String {
    values.iter().map(enum_value).collect::<Vec<_>>().join("|")
}

/// An enum at least this long (an icon registry) is listed with the values
/// that share a prefix grouped: `pre-{a|b|c}`.
const FOLD_MIN_VALUES: usize = 40;

/// A prefix (up to and including its first `-`) is grouped when at least
/// this many values in a row share it.
const FOLD_MIN_GROUP: usize = 3;

/// A long enum's values, runs that share a prefix grouped (`ui-{add|back}`):
/// `None` when the enum is short or nothing groups.
fn folded_enum_text(values: &[Value]) -> Option<String> {
    if values.len() < FOLD_MIN_VALUES {
        return None;
    }
    let names: Vec<String> = values.iter().map(enum_value).collect();
    let prefix = |v: &str| v.find('-').map(|i| v[..=i].to_owned());
    let mut parts = Vec::new();
    let mut folded = false;
    let mut i = 0;
    while i < names.len() {
        let Some(pre) = prefix(&names[i]).filter(|p| p.len() < names[i].len()) else {
            parts.push(names[i].clone());
            i += 1;
            continue;
        };
        let run = names[i..]
            .iter()
            .take_while(|v| v.starts_with(&pre) && v.len() > pre.len())
            .count();
        if run >= FOLD_MIN_GROUP {
            let rest: Vec<&str> = names[i..i + run].iter().map(|v| &v[pre.len()..]).collect();
            parts.push(format!("{pre}{{{}}}", rest.join("|")));
            folded = true;
            i += run;
        } else {
            parts.push(names[i].clone());
            i += 1;
        }
    }
    folded.then(|| parts.join("|"))
}

struct Entry {
    /// The name the first occurrence suggests.
    hint: String,
    /// The schema of the first occurrence.
    schema: Value,
    count: usize,
}

/// The shapes of a catalog's component properties, with the names of those
/// listed once under Shared Types.
struct Types<'c> {
    catalog: &'c Catalog,
    entries: Vec<Entry>,
    by_key: HashMap<String, usize>,
    /// Entry index -> name, for the named shapes.
    names: HashMap<usize, String>,
}

impl<'c> Types<'c> {
    fn new(catalog: &'c Catalog, components: &[&Component]) -> Types<'c> {
        let mut types = Types {
            catalog,
            entries: Vec::new(),
            by_key: HashMap::new(),
            names: HashMap::new(),
        };
        for component in components {
            for p in component.properties() {
                if let Some(schema) = component.property_schema(p) {
                    types.visit(p, schema);
                }
            }
        }
        let mut taken: HashSet<String> = HashSet::new();
        for i in 0..types.entries.len() {
            let entry = &types.entries[i];
            if entry.count < 2 {
                continue;
            }
            let long = types
                .render(&entry.schema, 0, false)
                .is_some_and(|t| t.len() >= SHARED_TYPE_MIN_CHARS);
            if !long {
                continue;
            }
            let base = pascal_case(&entry.hint);
            let mut name = base.clone();
            let mut n = 2;
            while !taken.insert(name.clone()) {
                name = format!("{base}{n}");
                n += 1;
            }
            types.names.insert(i, name);
        }
        types
    }

    /// Counts a property's shape and, the first time it is seen, the shapes
    /// of its map's keys.
    fn visit(&mut self, prop: &str, schema: &Value) {
        let Some(shape) = Shape::of(self.catalog, schema) else {
            return;
        };
        let key = shape.key();
        if let Some(&i) = self.by_key.get(&key) {
            self.entries[i].count += 1;
            return;
        }
        let hint = def_name(schema).unwrap_or(prop).to_owned();
        self.by_key.insert(key, self.entries.len());
        self.entries.push(Entry {
            hint,
            schema: schema.clone(),
            count: 1,
        });
        if let Shape::Map(props, _) = shape {
            for (k, s) in &props {
                self.visit(k, s);
            }
        }
    }

    fn name_of(&self, shape: &Shape) -> Option<&String> {
        self.by_key
            .get(&shape.key())
            .and_then(|i| self.names.get(i))
    }

    /// How a property's type reads: a shared type's name, or the values or
    /// keys spelled out. `None` for a plain value.
    fn type_of(&self, schema: &Value) -> Option<String> {
        self.render(schema, 0, true)
    }

    fn render(&self, schema: &Value, depth: usize, use_names: bool) -> Option<String> {
        let shape = Shape::of(self.catalog, schema)?;
        let list = matches!(shape, Shape::Map(_, true));
        let wrap = |t: String| if list { format!("[{t}]") } else { t };
        if use_names {
            if let Some(name) = self.name_of(&shape) {
                return Some(wrap(name.clone()));
            }
        }
        Some(match shape {
            Shape::Action => "Action".into(),
            Shape::Enum(values) => enum_text(&values),
            Shape::Map(props, _) => wrap(format!("{{{}}}", self.keys(&props, depth))),
        })
    }

    /// A map's keys, each with its type; keys of one type (several, all
    /// typed alike, as an event map's actions) read `a|b: type`.
    fn keys(&self, props: &Map<String, Value>, depth: usize) -> String {
        let typed: Vec<(&String, Option<String>)> = props
            .iter()
            .map(|(k, s)| {
                let t = if depth < MAX_INLINE_DEPTH {
                    self.render(s, depth + 1, true)
                } else {
                    None
                };
                (k, t)
            })
            .collect();
        if let [(_, Some(first)), rest @ ..] = typed.as_slice() {
            if !rest.is_empty() && rest.iter().all(|(_, t)| t.as_ref() == Some(first)) {
                let names: Vec<&str> = typed.iter().map(|(k, _)| k.as_str()).collect();
                return format!("{}: {first}", names.join("|"));
            }
        }
        typed
            .into_iter()
            .map(|(k, t)| match t {
                Some(t) => format!("{k}: {t}"),
                None => k.clone(),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The `### Shared Types` lines, in the order the types were first met.
    fn definitions(&self) -> Vec<String> {
        let mut named: Vec<(usize, &String)> = self.names.iter().map(|(i, n)| (*i, n)).collect();
        named.sort_by_key(|(i, _)| *i);
        let maps: Vec<(&String, Map<String, Value>)> = named
            .iter()
            .filter_map(
                |(i, name)| match Shape::of(self.catalog, &self.entries[*i].schema) {
                    Some(Shape::Map(props, _)) => Some((*name, props)),
                    _ => None,
                },
            )
            .collect();
        named
            .into_iter()
            .filter_map(|(i, name)| {
                let body = match Shape::of(self.catalog, &self.entries[i].schema)? {
                    Shape::Action => return None,
                    Shape::Enum(values) => format!(
                        "one of {}",
                        folded_enum_text(&values).unwrap_or_else(|| enum_text(&values))
                    ),
                    Shape::Map(props, _) => {
                        // A map with another's keys but a few.
                        let base = maps.iter().find(|(other, theirs)| {
                            *other != name
                                && theirs.len() > props.len()
                                && (theirs.len() - props.len()) * 4 < props.len()
                                && props.iter().all(|(k, v)| theirs.get(k) == Some(v))
                        });
                        match base {
                            Some((other, theirs)) => format!(
                                "{other} without {}",
                                theirs
                                    .keys()
                                    .filter(|k| !props.contains_key(*k))
                                    .cloned()
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                            None => format!("{{{}}}", self.keys(&props, 0)),
                        }
                    }
                };
                Some(format!("- {name}: {body}"))
            })
            .collect()
    }
}

/// The first sentence of a description (up to the first `. ` before a
/// capital, or the first line break), whitespace collapsed.
pub(super) fn first_sentence(text: &str) -> String {
    let text = text.trim();
    let mut end = text.find('\n').unwrap_or(text.len());
    let bytes = text.as_bytes();
    for (i, _) in text.match_indices(". ") {
        let before = &text[..i];
        if ["e.g", "i.e", "etc", "vs"]
            .iter()
            .any(|a| before.ends_with(a))
        {
            continue;
        }
        if bytes
            .get(i + 2)
            .is_some_and(|b| b.is_ascii_uppercase() || matches!(b, b'`' | b'('))
        {
            end = end.min(i + 1);
            break;
        }
    }
    text[..end].split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The scalar type a property holds, when it is a number (`:n`) or a
/// boolean (`:b`): its type, or the common `Dynamic…` type it is.
fn scalar_mark(catalog: &Catalog, schema: &Value) -> &'static str {
    let s = catalog
        .dereference_local_schema(schema)
        .unwrap_or_else(|| schema.clone());
    match (
        str_field(schema, "$ref").and_then(common_def_name),
        str_field(&s, "type"),
    ) {
        (Some("DynamicNumber"), _) | (_, Some("number" | "integer")) => ":n",
        (Some("DynamicBoolean"), _) | (_, Some("boolean")) => ":b",
        _ => "",
    }
}

/// A component property as a compact signature lists it.
fn argument_label(catalog: &Catalog, component: &Component, p: &str) -> String {
    let schema = component.property_schema(p);
    let optional = if component.is_required(p) { "" } else { "?" };
    let is_component_id = schema
        .and_then(|s| str_field(s, "$ref"))
        .and_then(common_def_name)
        == Some("ComponentId");
    if is_component_id {
        return format!("{p}{optional} (component ID)");
    }
    let scalar = schema.map_or("", |s| scalar_mark(catalog, s));
    let dynamic = if schema.is_some_and(|s| catalog.admits_path(s)) {
        "~"
    } else {
        ""
    };
    format!("{p}{optional}{dynamic}{scalar}")
}

fn entry(name: &str, args: &[String], description: Option<&str>, details: &[String]) -> String {
    let mut sig = format!("• {name}({})", args.join(", "));
    if let Some(desc) = description.map(first_sentence).filter(|d| !d.is_empty()) {
        sig.push_str(" — ");
        sig.push_str(&desc);
    }
    for d in details {
        sig.push_str("\n  - ");
        sig.push_str(d);
    }
    sig
}

/// One component property: its signature label and its type line.
struct Arg {
    label: String,
    line: Option<String>,
}

/// How components take actions, when they take them in a map (as eui's
/// `on={press: ...}`) rather than in an `action` argument, which the
/// grammar rules' examples show.
fn action_hint(list: &[&Component], args: &[Vec<Arg>]) -> Option<String> {
    let (prop, key) = args.iter().flatten().find_map(|a| {
        let line = a.line.as_deref()?;
        let (prop, ty) = line.split_once(": {")?;
        let first = ty.split([':', '|', ',']).next()?;
        ty.contains(": Action")
            .then(|| (prop.to_owned(), first.to_owned()))
    })?;
    let mut hint = format!(
        "Actions go in a component's `{prop}` map, keyed by event: {prop}={{{key}: Event(\"save\")}}."
    );
    if !list
        .iter()
        .any(|c| c.properties().iter().any(|p| p == "action"))
    {
        hint.push_str(" No component takes an `action` argument.");
    }
    Some(hint)
}

/// A call that binds a component's number and gives a literal to another
/// optional number of it that takes no binding
/// (`Progress(value=$/progress, max=5)`): the component with the fewest
/// properties that has a required bindable number and an optional literal
/// one.
fn literal_example(list: &[&Component], args: &[Vec<Arg>]) -> Option<String> {
    // A required bindable number (`value~:n`) or an optional literal one
    // (`max?:n`).
    let number = |a: &Arg, bindable: bool| {
        let base = a.label.trim_end_matches(":n");
        a.label.ends_with(":n")
            && if bindable {
                base.ends_with('~') && !base.ends_with("?~")
            } else {
                base.ends_with('?')
            }
    };
    let name = |a: &Arg| {
        a.label
            .split(['?', '~', ':', ' '])
            .next()
            .unwrap_or_default()
            .to_owned()
    };
    list.iter()
        .zip(args)
        .filter_map(|(c, args)| {
            let bound = args.iter().find(|a| number(a, true))?;
            let literal = args.iter().find(|a| number(a, false))?;
            Some((args.len(), c.name(), name(bound), name(literal)))
        })
        .min_by_key(|(n, ..)| *n)
        .map(|(_, c, bound, literal)| format!("{c}({bound}=$/{}, {literal}=5)", c.to_lowercase()))
}

/// The compact component section: the legend, the shared types, the common
/// properties and one signature per component.
pub(super) fn components(catalog: &Catalog) -> String {
    let mut list: Vec<&Component> = catalog.components().iter().collect();
    list.sort_by(|a, b| a.name().cmp(b.name()));
    let types = Types::new(catalog, &list);

    let args: Vec<Vec<Arg>> = list
        .iter()
        .map(|component| {
            component
                .properties()
                .iter()
                .map(|p| Arg {
                    label: argument_label(catalog, component, p),
                    line: component
                        .property_schema(p)
                        .and_then(|s| types.type_of(s))
                        .map(|t| format!("{p}: {t}")),
                })
                .collect()
        })
        .collect();

    // The properties nearly every component takes, alike.
    let mut takers: HashMap<(&str, Option<&str>), usize> = HashMap::new();
    let mut first_seen = Vec::new();
    for arg in args.iter().flatten() {
        let k = (arg.label.as_str(), arg.line.as_deref());
        let n = takers.entry(k).or_insert(0);
        if *n == 0 {
            first_seen.push(k);
        }
        *n += 1;
    }
    let universal: Vec<(&str, Option<&str>)> = first_seen
        .iter()
        .copied()
        .filter(|k| {
            list.len() >= UNIVERSAL_MIN_COMPONENTS
                && takers[k] as f64 >= list.len() as f64 * UNIVERSAL_PROPERTY_SHARE
        })
        .collect();

    // The type lines several components share, one per property name.
    let mut uses: HashMap<&str, usize> = HashMap::new();
    let mut order = Vec::new();
    for arg in args.iter().flatten() {
        if let Some(line) = arg.line.as_deref() {
            if universal.iter().any(|(_, l)| *l == Some(line)) {
                continue;
            }
            let n = uses.entry(line).or_insert(0);
            if *n == 0 {
                order.push(line);
            }
            *n += 1;
        }
    }
    let name_of = |line: &str| line.split(':').next().unwrap_or_default().to_owned();
    let shared: Vec<&str> = order
        .iter()
        .copied()
        .filter(|l| uses[l] >= COMMON_PROPERTY_MIN_COMPONENTS)
        .collect();
    let common: Vec<&str> = shared
        .iter()
        .copied()
        .filter(|l| shared.iter().filter(|m| name_of(m) == name_of(l)).count() == 1)
        .collect();

    let signatures = list
        .iter()
        .zip(&args)
        .map(|(component, args)| {
            let kept: Vec<&Arg> = args
                .iter()
                .filter(|a| !universal.contains(&(a.label.as_str(), a.line.as_deref())))
                .collect();
            let labels: Vec<String> = kept.iter().map(|a| a.label.clone()).collect();
            let details: Vec<String> = kept
                .iter()
                .filter_map(|a| a.line.clone())
                .filter(|l| !common.contains(&l.as_str()))
                .collect();
            entry(component.name(), &labels, component.description(), &details)
        })
        .collect::<Vec<_>>()
        .join("\n");

    let defs = types.definitions();
    let folded = defs.iter().any(|d| d.contains("-{"));
    let mut out = vec![legend(literal_example(&list, &args).as_deref(), folded)];
    if let Some(hint) = action_hint(&list, &args) {
        out.push(hint);
    }
    if !defs.is_empty() {
        out.push(format!("\n### Shared Types\n{}", defs.join("\n")));
    }
    let mut common_lines = Vec::new();
    if !universal.is_empty() {
        let listed = universal
            .iter()
            .map(|(label, line)| match line {
                Some(line) => format!("{label} ({})", line.split_once(": ").map_or("", |x| x.1)),
                None => (*label).to_owned(),
            })
            .collect::<Vec<_>>()
            .join(", ");
        common_lines.push(format!(
            "Every component also takes {listed}: pass them by name."
        ));
    }
    if !common.is_empty() {
        common_lines.push(
            "A property with one of these names has this type unless its component says otherwise:"
                .into(),
        );
        common_lines.extend(common.iter().map(|l| format!("- {l}")));
    }
    if !common_lines.is_empty() {
        out.push(format!(
            "\n### Common Properties\n{}",
            common_lines.join("\n")
        ));
    }
    out.push(format!("\n### Components\n{signatures}"));
    out.join("\n")
}

/// Rules of upstream's text the compact signatures make moot: `(static)`
/// is never written (the legend's `~` replaces it), and the action rule
/// is about an `action` argument no component takes when actions go in a
/// map.
const STATIC_RULE: &str = "Static properties:";
const ACTION_RULE: &str = "Required actions:";

/// Where the check sentence goes: the end of rule 7.
const CHECK_RULE_END: &str = r#"?regex("^[0-9]{5}$", "Postal code must be 5 digits")."#;

/// Where the data rule goes: after rule 11 (templates).
const TEMPLATE_RULE_END: &str = "    itemTemplate = Image($url)";

/// Removes the rule paragraph (`\n\n<n>. <head>…`) whose text opens with
/// `head`.
fn remove_rule(text: &str, head: &str) -> String {
    let mut from = 0;
    while let Some(i) = text[from..].find("\n\n").map(|i| i + from) {
        let rest = &text[i + 2..];
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 && rest[digits..].starts_with(". ") && rest[digits + 2..].starts_with(head) {
            let end = text[i + 2..].find("\n\n").map_or(text.len(), |e| i + 2 + e);
            return format!("{}{}", &text[..i], &text[end..]);
        }
        from = i + 2;
    }
    text.to_owned()
}

/// What a compact data/check hint names, read from the catalogs: the
/// property a check reads (`value`), the list that takes a template, and a
/// component to show one item's field.
struct BindingNames<'c> {
    /// The check function and the component property its first argument
    /// reads when the component binds it.
    check: Option<(&'c str, &'c str)>,
    /// A check function whose second argument is a number (`length`), for
    /// the example of positional check arguments.
    numeric_check: Option<&'c str>,
    /// A component and its child-list property that takes a template.
    list: Option<(&'c str, &'c str)>,
    /// A component and a required property that takes a binding.
    item: Option<(&'c str, &'c str)>,
}

impl<'c> BindingNames<'c> {
    fn of(catalogs: &'c [Catalog]) -> BindingNames<'c> {
        let components = || {
            catalogs
                .iter()
                .flat_map(|c| c.components().iter().map(move |x| (c, x)))
        };
        let bindable = |c: &Catalog, x: &Component, p: &str| {
            x.property_schema(p)
                .is_some_and(|s| c.admits_path(s) && c.slot_kind(s).is_none())
        };
        // A check function (one returning a check result or a boolean)
        // whose first argument a checkable component binds; `required`
        // first, as the commonest check.
        let mut check = None;
        let mut numeric_check = None;
        for catalog in catalogs {
            let mut functions: Vec<&Function> = catalog
                .functions()
                .iter()
                .filter(|f| matches!(f.return_type(), Some("validationResult" | "boolean")))
                .collect();
            functions.sort_by_key(|f| f.name() != "required");
            numeric_check = numeric_check.or_else(|| {
                functions.iter().find_map(|f| {
                    let second = f.arguments().get(1)?;
                    let schema = f.argument_schema(second)?;
                    catalog.accepts_number(schema).then(|| f.name())
                })
            });
            check = functions.iter().find_map(|f| {
                let first = f.arguments().first()?;
                components()
                    .any(|(c, x)| x.is_checkable() && bindable(c, x, first))
                    .then(|| (f.name(), first.as_str()))
            });
            if check.is_some() {
                break;
            }
        }
        let prefer = |name: &str| {
            let name = name.to_owned();
            move |(_, x): &(&Catalog, &Component)| x.name() != name
        };
        let mut lists: Vec<(&Catalog, &Component)> = components()
            .filter(|(c, x)| {
                x.property_schema("children")
                    .is_some_and(|s| c.slot_kind(s) == Some(SlotKind::ChildList))
            })
            .collect();
        lists.sort_by_key(prefer("List"));
        let list = lists.first().map(|(_, x)| (x.name(), "children"));
        let mut items: Vec<(&Catalog, &Component, &str)> = components()
            .filter_map(|(c, x)| {
                let p = x.required().iter().find(|p| {
                    p.as_str() != "component"
                        && bindable(c, x, p)
                        && x.property_schema(p).is_some_and(|s| !c.accepts_number(s))
                })?;
                Some((c, x, p.as_str()))
            })
            .collect();
        items.sort_by_key(|(_, x, _)| x.name() != "Text");
        let item = items.first().map(|(_, x, p)| (x.name(), *p));
        BindingNames {
            check,
            numeric_check,
            list,
            item,
        }
    }
}

/// Upstream's rules, adapted to the compact signatures: the rules about
/// `(static)` and the `action` argument go when they do not apply, rule 7
/// says what a check reads, and rules after the templates say to keep data
/// in the data model and bind to it, and how a screen is laid out (#AXC-22).
/// Not renumbered.
pub(super) fn adapt_rules(rules: &str, catalogs: &[Catalog], writes_data: bool) -> String {
    let mut rules = remove_rule(rules, STATIC_RULE);
    let takes_action = catalogs
        .iter()
        .flat_map(Catalog::components)
        .any(|c| c.properties().iter().any(|p| p == "action"));
    if !takes_action {
        rules = remove_rule(&rules, ACTION_RULE);
    }
    let names = BindingNames::of(catalogs);
    if let Some((function, prop)) = names.check {
        let sentence = format!(
            " A check reads the value its component binds to `{prop}` (`{prop}=$/form/email`), so bind it; on a component that binds none, name the value first: ?{function}($/form/terms, \"Required\")."
        );
        let positional = match names.numeric_check {
            Some(f) => {
                format!(" Check arguments are positional, never named: ?{f}(8, \"Too short\").")
            }
            None => " Check arguments are positional, never named.".to_owned(),
        };
        rules = rules.replacen(
            CHECK_RULE_END,
            &format!("{CHECK_RULE_END}{sentence}{positional}"),
            1,
        );
    }
    let mut data = vec!["Keep data in the data model and bind to it.".to_owned()];
    if let Some((_, prop)) = names.check {
        data.push(format!(
            "Bind every input's `{prop}` to a path (`{prop}=$/form/name`) and send it in events: Event(\"save\", {{name: $/form/name}})."
        ));
    }
    if let (Some((list, slot)), Some((item, prop))) = (names.list, names.item) {
        let fill = if writes_data {
            "$/items = [{title: \"A\"}, {title: \"B\"}], "
        } else {
            ""
        };
        data.push(format!(
            "Repeated content (cards, rows, messages, events) is one template over a list, not copies: {fill}{list}({slot}=_template($/items, item)), item = {item}({prop}=$title) or any tree reading the item ($title). Only `~` properties take a binding, in a template too; give the others a literal."
        ));
    }
    if writes_data {
        if let Some((_, prop)) = names.check {
            data.push(format!(
                "Give every bound path a starting value: $/form/name = \"\" for `{prop}=$/form/name`."
            ));
        }
    }
    if data.len() > 1 && rules.contains(TEMPLATE_RULE_END) {
        let layout = layout_rule(catalogs)
            .map(|l| format!("\n\n99. Layout: {l}"))
            .unwrap_or_default();
        rules = rules.replacen(
            TEMPLATE_RULE_END,
            &format!(
                "{TEMPLATE_RULE_END}\n\n99. Data: {}{layout}",
                data.join(" ")
            ),
            1,
        );
    }
    rules
}

/// The layout a catalog's screens are built from, by the components'
/// conventional names, each part only when the catalog has its components:
/// a screen frame (`Page`), a titled box for each block (`Card`), list rows
/// (`List` of `Row`) instead of stacked text, and a labelled checkbox.
fn layout_rule(catalogs: &[Catalog]) -> Option<String> {
    let find = |name: &str| {
        catalogs
            .iter()
            .find_map(|c| c.component(name).map(|x| (c, x)))
    };
    let takes = |name: &str, prop: &str| {
        find(name).is_some_and(|(_, x)| x.properties().iter().any(|p| p == prop))
    };
    let mut parts = Vec::new();
    if takes("Page", "title") {
        parts.push("one Page(title=\"…\") frames the screen".to_owned());
    }
    if takes("Card", "title") {
        parts.push(
            "a form, a table or a group of related content sits in a Card(title=\"…\")".to_owned(),
        );
    }
    if let (Some(_), Some((catalog, row))) = (find("List"), find("Row")) {
        let fields: Vec<String> = ["title", "subtitle", "meta"]
            .into_iter()
            .filter(|p| {
                row.property_schema(p)
                    .is_some_and(|s| catalog.admits_path(s))
            })
            .map(|p| format!("{p}=${p}"))
            .collect();
        if !fields.is_empty() {
            parts.push(format!(
                "items, features or links are List rows, Row({}), not stacked Text",
                fields.join(", ")
            ));
        }
    }
    let labelled = catalogs.iter().flat_map(Catalog::components).find(|x| {
        x.is_checkable() && x.is_required("label") && x.properties().iter().any(|p| p == "checked")
    });
    if let Some(x) = labelled {
        parts.push(format!(
            "every control has a visible label, {}(label=$title) in a template too",
            x.name()
        ));
    }
    (!parts.is_empty()).then(|| {
        let mut text = parts.join("; ");
        text.push('.');
        let mut chars = text.chars();
        chars
            .next()
            .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
            .unwrap_or_default()
    })
}

/// The compact signatures of the named functions, in the order given: the
/// arguments, no descriptions, on one line.
pub(super) fn functions<'n>(catalog: &Catalog, names: impl IntoIterator<Item = &'n str>) -> String {
    let sigs: Vec<String> = names
        .into_iter()
        .filter_map(|name| catalog.function(name))
        .map(|function| {
            let args: Vec<String> = function
                .arguments()
                .iter()
                .map(|p| format!("{p}{}", if function.is_required(p) { "" } else { "?" }))
                .collect();
            format!("{}({})", function.name(), args.join(", "))
        })
        .collect();
    if sigs.is_empty() {
        String::new()
    } else {
        format!("• {}", sigs.join(", "))
    }
}

/// The catalog's own functions, sorted by name.
pub(super) fn catalog_functions(catalog: &Catalog) -> String {
    let mut names: Vec<&str> = catalog.functions().iter().map(|f| f.name()).collect();
    names.sort_unstable();
    functions(catalog, names)
}

#[cfg(test)]
mod tests {
    use super::{first_sentence, pascal_case};

    #[test]
    fn first_sentence_stops_at_a_sentence_end() {
        assert_eq!(first_sentence("One thing. Another."), "One thing.");
        assert_eq!(first_sentence("Use e.g. a card. Then"), "Use e.g. a card.");
        assert_eq!(first_sentence("Line one\nline two"), "Line one");
        assert_eq!(
            first_sentence("v1.2 is fine. lower case"),
            "v1.2 is fine. lower case"
        );
    }

    #[test]
    fn type_names_are_pascal_case() {
        assert_eq!(pascal_case("menuItem"), "MenuItem");
        assert_eq!(pascal_case("icon"), "Icon");
        assert_eq!(pascal_case("On_Button"), "OnButton");
    }
}
