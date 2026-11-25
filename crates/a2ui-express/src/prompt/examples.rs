//! Examples: A2UI messages rewritten as Express through the decompiler.

use std::collections::HashSet;

use serde_json::{Map, Value};

use crate::Result;
use crate::decompile::{DecompileOptions, decompile};
use crate::schema::{Catalog, CatalogSet};

/// The message types an example may hold to be rewritten as Express.
const TRANSLATABLE: [&str; 6] = [
    "createSurface",
    "updateComponents",
    "updateDataModel",
    "deleteSurface",
    "callFunction",
    "callRendererFunction",
];

/// JS truthiness of an optional field.
fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        Some(_) => true,
    }
}

fn surface_id(op: &Map<String, Value>) -> String {
    op.get("surfaceId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("default_surface")
        .to_owned()
}

/// Stamps `catalogId` on every catalog function call (not a system function
/// such as `@index`) that leaves it out.
fn default_catalog_on_calls(value: &Value, catalog_id: &str) -> Value {
    match value {
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|v| default_catalog_on_calls(v, catalog_id))
                .collect(),
        ),
        Value::Object(map) => {
            let mut copy: Map<String, Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), default_catalog_on_calls(v, catalog_id)))
                .collect();
            let call = copy.get("@call").and_then(Value::as_str);
            let is_catalog_call = call.is_some_and(|c| !c.starts_with('@'));
            if is_catalog_call && !truthy(copy.get("catalogId")) {
                copy.insert("catalogId".into(), catalog_id.into());
            }
            Value::Object(copy)
        }
        other => other.clone(),
    }
}

fn default_catalog_on_component(component: &Value, catalog: &Catalog) -> Value {
    let mut transformed = default_catalog_on_calls(component, catalog.id());
    let Some(map) = transformed.as_object_mut() else {
        return transformed;
    };
    if map.get("component").is_some_and(Value::is_string) && !map.contains_key("@catalogId") {
        map.insert("@catalogId".into(), catalog.id().into());
    }
    transformed
}

fn with_catalog_id(op: &Map<String, Value>, catalog_id: &str) -> Value {
    let mut copy = op.clone();
    copy.insert("catalogId".into(), catalog_id.into());
    Value::Object(copy)
}

/// Stamps the example's catalog on the messages that belong to a catalog but
/// leave it out, so the decompiler can resolve them with several catalogs
/// active.
pub(super) fn with_default_catalog(messages: &[Value], catalog: &Catalog) -> Vec<Value> {
    let id = catalog.id();
    let mut created: HashSet<String> = HashSet::new();
    messages
        .iter()
        .map(|raw| {
            let Some(msg) = raw.as_object() else {
                return raw.clone();
            };
            let mut copy = msg.clone();
            if let Some(Value::Object(cs)) = msg.get("createSurface") {
                created.insert(surface_id(cs));
                if !truthy(cs.get("catalogId")) {
                    copy.insert("createSurface".into(), with_catalog_id(cs, id));
                }
            } else if let Some(Value::Object(ds)) = msg.get("deleteSurface") {
                created.remove(&surface_id(ds));
            } else if let Some(Value::Object(uc)) = msg.get("updateComponents") {
                if let (false, Some(Value::Array(components))) =
                    (created.contains(&surface_id(uc)), uc.get("components"))
                {
                    let mut uc = uc.clone();
                    let stamped = components
                        .iter()
                        .map(|c| default_catalog_on_component(c, catalog))
                        .collect();
                    uc.insert("components".into(), Value::Array(stamped));
                    copy.insert("updateComponents".into(), Value::Object(uc));
                }
            } else if let Some(Value::Object(crf)) = msg.get("callRendererFunction") {
                if let Some(Value::Object(cf)) = crf.get("callFunction") {
                    if !truthy(cf.get("catalogId")) {
                        let mut crf = crf.clone();
                        crf.insert("callFunction".into(), with_catalog_id(cf, id));
                        copy.insert("callRendererFunction".into(), Value::Object(crf));
                    }
                }
            }
            Value::Object(copy)
        })
        .collect()
}

/// Wraps one Express block in its sentinel tags.
pub(super) fn wrap(dsl: &str) -> String {
    format!("<a2ui>\n{dsl}\n</a2ui>").trim_end().to_owned()
}

/// Decompiles the messages of one example into Express, without tags.
pub(super) fn decompile_turn(
    catalogs: &CatalogSet,
    catalog: &Catalog,
    messages: &[Value],
) -> Result<String> {
    let normalized = with_default_catalog(messages, catalog);
    decompile(catalogs, &normalized, &DecompileOptions::default())
}

/// Rewrites the fenced JSON blocks of a catalog's instructions as Express.
///
/// A block is rewritten when it parses and every message in it is one the
/// decompiler translates; any other block, or one that fails to decompile,
/// is left as written. Ports upstream's `/```json\s*\n([\s\S]*?)\n```/g`
/// replacement.
pub(super) fn rewrite_json_blocks(text: &str, catalogs: &CatalogSet, catalog: &Catalog) -> String {
    const OPEN: &str = "```json";
    const CLOSE: &str = "\n```";
    let mut out = String::with_capacity(text.len());
    let mut pos = 0;
    let mut search = 0;
    while let Some(found) = text[search..].find(OPEN) {
        let start = search + found;
        let after = start + OPEN.len();
        let ws_end = text[after..]
            .find(|c: char| !c.is_whitespace())
            .map_or(text.len(), |e| after + e);
        let Some(nl) = text[after..ws_end].rfind('\n') else {
            search = after;
            continue;
        };
        let content_start = after + nl + 1;
        let Some(close) = text[content_start..].find(CLOSE) else {
            break;
        };
        let content_end = content_start + close;
        let end = content_end + CLOSE.len();
        out.push_str(&text[pos..start]);
        match translate_block(&text[content_start..content_end], catalogs, catalog) {
            Some(dsl) => out.push_str(&format!("```\n{}\n```", wrap(&dsl))),
            None => out.push_str(&text[start..end]),
        }
        pos = end;
        search = end;
    }
    out.push_str(&text[pos..]);
    out
}

fn translate_block(json: &str, catalogs: &CatalogSet, catalog: &Catalog) -> Option<String> {
    let parsed: Value = serde_json::from_str(json.trim()).ok()?;
    let messages = match parsed {
        Value::Array(items) => items,
        obj @ Value::Object(_) => vec![obj],
        _ => return None,
    };
    let translatable = |m: &Value| {
        m.as_object()
            .is_some_and(|o| TRANSLATABLE.iter().any(|t| o.contains_key(*t)))
    };
    if messages.is_empty() || !messages.iter().all(translatable) {
        return None;
    }
    decompile_turn(catalogs, catalog, &messages).ok()
}
