//! The order components are written in.

use std::collections::{HashMap, HashSet, VecDeque};

use serde_json::{Map, Value};

use crate::schema::Catalog;

/// The order a block's components are written in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Order {
    /// The order the messages list them in, as upstream's decompilers do.
    #[default]
    Upstream,
    /// `root` first, then breadth-first through the child references, so a
    /// streaming renderer can draw the surface's outline before its leaves.
    ///
    /// Components that no other component of the block references (other
    /// than `root`) seed further walks, in message order; anything left over
    /// (a cycle) follows in message order. Recompiling writes the components
    /// in this order, so the round trip holds up to component order, which
    /// carries no meaning in A2UI.
    TopDown,
}

/// Returns the indices of `components` in the order `order` writes them.
///
/// `catalog_of` gives the catalog a component is read with, or `None` when
/// it cannot be resolved (it is then a leaf; the writer reports the error).
pub(crate) fn arrange<'c>(
    components: &[Map<String, Value>],
    order: Order,
    catalog_of: impl Fn(&Map<String, Value>) -> Option<&'c Catalog>,
) -> Vec<usize> {
    if order == Order::Upstream {
        return (0..components.len()).collect();
    }
    let mut by_id: HashMap<&str, usize> = HashMap::new();
    for (i, c) in components.iter().enumerate() {
        if let Some(id) = c.get("id").and_then(Value::as_str) {
            by_id.entry(id).or_insert(i);
        }
    }
    let children: Vec<Vec<usize>> = components
        .iter()
        .map(|c| {
            let mut refs = Vec::new();
            if let Some(catalog) = catalog_of(c) {
                component_references(c, catalog, &mut refs);
            }
            refs.iter()
                .filter_map(|r| by_id.get(r.as_str()).copied())
                .collect()
        })
        .collect();
    let mut referenced = vec![false; components.len()];
    for (parent, kids) in children.iter().enumerate() {
        for &k in kids {
            if k != parent {
                referenced[k] = true;
            }
        }
    }

    let mut seeds: Vec<usize> = Vec::new();
    if let Some(&root) = by_id.get("root") {
        seeds.push(root);
    }
    seeds.extend((0..components.len()).filter(|&i| !referenced[i]));
    seeds.extend(0..components.len());

    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(components.len());
    for seed in seeds {
        if !seen.insert(seed) {
            continue;
        }
        let mut queue = VecDeque::from([seed]);
        while let Some(i) = queue.pop_front() {
            out.push(i);
            for &k in &children[i] {
                if seen.insert(k) {
                    queue.push_back(k);
                }
            }
        }
    }
    out
}

/// Collects the component ids a component references, in the order its
/// properties write them.
fn component_references(component: &Map<String, Value>, catalog: &Catalog, out: &mut Vec<String>) {
    let name = component
        .get("component")
        .and_then(Value::as_str)
        .unwrap_or("");
    for (prop, value) in component {
        if prop == "id" || prop == "component" {
            continue;
        }
        let is_ref = catalog
            .property_schema(name, prop)
            .is_some_and(|s| catalog.is_component_reference(s));
        value_references(value, is_ref, out);
    }
}

fn value_references(value: &Value, is_ref: bool, out: &mut Vec<String>) {
    match value {
        Value::String(s) if is_ref => out.push(s.clone()),
        Value::Array(items) => items.iter().for_each(|v| value_references(v, is_ref, out)),
        Value::Object(map) => {
            if let (Some(Value::String(_)), Some(Value::String(id))) =
                (map.get("path"), map.get("componentId"))
            {
                out.push(id.clone());
                return;
            }
            for (k, v) in map {
                value_references(v, is_ref || k == "child" || k == "componentId", out);
            }
        }
        _ => {}
    }
}
