//! The agent's mirror of the surfaces a renderer holds.
//!
//! A port of the parts of upstream's `RendererState` (`renderer_state.py`)
//! that the response parser and the response stream rely on. Messages are
//! checked against what the renderer already received before they are sent:
//!
//! - every message states the catalogs' protocol version;
//! - a `createSurface` names an active catalog and a surface that does not
//!   exist yet; every other message names a surface that does;
//! - every component names a component its catalog declares.
//!
//! [`RendererState::check_topology`] then checks a surface's tree as a
//! whole, once a block is complete: the root exists, every child reference
//! resolves, there is no cycle, and every component added or replaced since
//! the last check is reachable from the root.
//!
//! The data model is not mirrored: no check depends on it, and the renderer's
//! copy drifts anyway as the user edits bound inputs.
//!
//! Changes are logged while a transaction is open ([`RendererState::begin`]),
//! so a failure can be rolled back at the cost of what it changed, not the
//! size of the surfaces.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde_json::{Map, Value};

use crate::schema::{Catalog, CatalogSet, CatalogVersion, SlotKind};
use crate::{Error, Result};

type Component = Map<String, Value>;

#[derive(Debug, Clone, Default)]
struct Surface {
    catalog_id: Option<String>,
    components: HashMap<String, Component>,
    /// Component ids in the order first added.
    order: Vec<String>,
}

#[derive(Debug)]
enum Undo {
    Surface(String, Option<Surface>),
    Component(String, String, Option<Component>),
    Changed(String, Option<HashSet<String>>),
    Baseline(String, Option<HashMap<String, Option<Component>>>),
}

/// A transaction mark, from [`RendererState::begin`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mark(usize);

/// The surfaces a renderer holds, as far as the agent's checks need them.
#[derive(Debug)]
pub struct RendererState {
    catalogs: Arc<CatalogSet>,
    version: CatalogVersion,
    surfaces: HashMap<String, Surface>,
    /// Components each surface added or replaced since its last check.
    changed: HashMap<String, HashSet<String>>,
    /// The state before this turn of each component in `changed` (`None`
    /// when it did not exist).
    baseline: HashMap<String, HashMap<String, Option<Component>>>,
    log: Vec<Undo>,
    depth: usize,
    /// Surfaces whose bookkeeping is already saved in the open transaction.
    saved: HashSet<String>,
}

impl RendererState {
    /// The empty state of a renderer holding `catalogs`, receiving messages
    /// of `version`. The catalogs are shared, not copied.
    pub fn new(catalogs: impl Into<Arc<CatalogSet>>, version: CatalogVersion) -> Self {
        RendererState {
            catalogs: catalogs.into(),
            version,
            surfaces: HashMap::new(),
            changed: HashMap::new(),
            baseline: HashMap::new(),
            log: Vec::new(),
            depth: 0,
            saved: HashSet::new(),
        }
    }

    /// The ids of the surfaces the renderer holds.
    pub fn surface_ids(&self) -> HashSet<String> {
        self.surfaces.keys().cloned().collect()
    }

    /// Whether the renderer holds the surface.
    pub fn has_surface(&self, surface_id: &str) -> bool {
        self.surfaces.contains_key(surface_id)
    }

    /// The component `id` of a surface, as last applied.
    pub fn component(&self, surface_id: &str, id: &str) -> Option<&Map<String, Value>> {
        self.surfaces.get(surface_id)?.components.get(id)
    }

    /// The number of components a surface holds.
    pub fn component_count(&self, surface_id: &str) -> usize {
        self.surfaces
            .get(surface_id)
            .map_or(0, |s| s.components.len())
    }

    // ---- transactions -------------------------------------------------------

    /// Opens a transaction; [`RendererState::rollback`] to the mark undoes
    /// everything since. Transactions nest.
    pub fn begin(&mut self) -> Mark {
        self.depth += 1;
        Mark(self.log.len())
    }

    /// Closes the innermost transaction, keeping its changes.
    pub fn commit(&mut self, _mark: Mark) {
        self.depth = self.depth.saturating_sub(1);
        if self.depth == 0 {
            self.log.clear();
            self.saved.clear();
        }
    }

    /// Closes the innermost transaction, undoing its changes.
    pub fn rollback(&mut self, mark: Mark) {
        while self.log.len() > mark.0 {
            match self.log.pop().expect("checked") {
                Undo::Surface(id, old) => match old {
                    Some(s) => {
                        self.surfaces.insert(id, s);
                    }
                    None => {
                        self.surfaces.remove(&id);
                    }
                },
                Undo::Component(surface, id, old) => {
                    if let Some(s) = self.surfaces.get_mut(&surface) {
                        match old {
                            Some(c) => {
                                s.components.insert(id, c);
                            }
                            None => {
                                s.components.remove(&id);
                                if let Some(p) = s.order.iter().rposition(|x| *x == id) {
                                    s.order.remove(p);
                                }
                            }
                        }
                    }
                }
                Undo::Changed(surface, old) => match old {
                    Some(c) => {
                        self.changed.insert(surface, c);
                    }
                    None => {
                        self.changed.remove(&surface);
                    }
                },
                Undo::Baseline(surface, old) => match old {
                    Some(b) => {
                        self.baseline.insert(surface, b);
                    }
                    None => {
                        self.baseline.remove(&surface);
                    }
                },
            }
        }
        self.saved.clear();
        self.depth = self.depth.saturating_sub(1);
        if self.depth == 0 {
            self.log.clear();
        }
    }

    fn record(&mut self, undo: Undo) {
        if self.depth > 0 {
            self.log.push(undo);
        }
    }

    /// Saves a surface's bookkeeping once per transaction before it changes.
    fn save_book(&mut self, surface_id: &str) {
        if self.depth == 0 || !self.saved.insert(surface_id.to_owned()) {
            return;
        }
        let changed = self.changed.get(surface_id).cloned();
        let baseline = self.baseline.get(surface_id).cloned();
        self.log.push(Undo::Changed(surface_id.to_owned(), changed));
        self.log
            .push(Undo::Baseline(surface_id.to_owned(), baseline));
    }

    // ---- applying messages --------------------------------------------------

    /// Checks the messages against the state and applies them, all or none.
    ///
    /// Returns the surfaces they create, update or delete, in the order the
    /// messages first name them.
    pub fn apply(&mut self, messages: &[Value]) -> Result<Vec<String>> {
        let mark = self.begin();
        let mut touched: Vec<String> = Vec::new();
        for (index, message) in messages.iter().enumerate() {
            match self.apply_message(index, message) {
                Ok(Some(surface)) => {
                    if !touched.contains(&surface) {
                        touched.push(surface);
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    self.rollback(mark);
                    return Err(e);
                }
            }
        }
        self.commit(mark);
        Ok(touched)
    }

    fn apply_message(&mut self, index: usize, message: &Value) -> Result<Option<String>> {
        let invalid = |m: String| Error::Validation(m);
        let Some(map) = message.as_object() else {
            return Err(invalid(format!("Message {index} is not a JSON object.")));
        };
        let wire = self.version.wire();
        match map.get("version").and_then(Value::as_str) {
            Some(v) if versions_match(v, wire) => {}
            other => {
                return Err(invalid(format!(
                    "Message {index} states version {}, but the catalogs expect '{wire}'.",
                    other.map_or("none".to_owned(), |v| format!("'{v}'"))
                )));
            }
        }
        let surface_of = |body: &Value| -> Result<String> {
            body.get("surfaceId")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| invalid(format!("Message {index} names no surfaceId.")))
        };
        if let Some(body) = map.get("createSurface") {
            let id = surface_of(body)?;
            if self.surfaces.contains_key(&id) {
                return Err(invalid(format!(
                    "Surface '{id}' already exists. Update it with surface(\"{id}\", update=true), or delete it first."
                )));
            }
            let catalog_id = body
                .get("catalogId")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if let Some(cid) = &catalog_id {
                if self.catalogs.get(cid).is_none() {
                    return Err(invalid(format!(
                        "Surface '{id}' names catalog '{cid}', which isn't one of the active catalogs."
                    )));
                }
            }
            self.surfaces.insert(
                id.clone(),
                Surface {
                    catalog_id,
                    ..Surface::default()
                },
            );
            self.record(Undo::Surface(id.clone(), None));
            if let Some(components) = body.get("components") {
                self.update_components(&id, components)?;
            }
            return Ok(Some(id));
        }
        if let Some(body) = map.get("updateComponents") {
            let id = self.existing(surface_of(body)?, "updateComponents")?;
            self.update_components(&id, body.get("components").unwrap_or(&Value::Null))?;
            return Ok(Some(id));
        }
        if let Some(body) = map.get("updateDataModel") {
            let id = self.existing(surface_of(body)?, "updateDataModel")?;
            return Ok(Some(id));
        }
        if let Some(body) = map.get("deleteSurface") {
            let id = self.existing(surface_of(body)?, "deleteSurface")?;
            let old = self.surfaces.remove(&id);
            self.record(Undo::Surface(id.clone(), old));
            self.forget_changed(&id);
            return Ok(Some(id));
        }
        if map.contains_key("callRendererFunction") {
            return Ok(None);
        }
        Err(invalid(format!(
            "Message {index} is not a message a renderer accepts."
        )))
    }

    fn existing(&self, id: String, action: &str) -> Result<String> {
        if self.surfaces.contains_key(&id) {
            Ok(id)
        } else {
            Err(Error::Validation(format!(
                "{action} names surface '{id}', which has not been created."
            )))
        }
    }

    fn update_components(&mut self, surface_id: &str, components: &Value) -> Result<()> {
        let Some(list) = components.as_array() else {
            return Err(Error::Validation(format!(
                "The components of surface '{surface_id}' are not a list."
            )));
        };
        let surface_catalog = self.surfaces[surface_id].catalog_id.clone();
        for component in list {
            let Some(c) = component.as_object() else {
                return Err(Error::Validation(format!(
                    "A component of surface '{surface_id}' is not an object."
                )));
            };
            let Some(id) = c.get("id").and_then(Value::as_str) else {
                return Err(Error::Validation(format!(
                    "A component of surface '{surface_id}' has no id."
                )));
            };
            let Some(kind) = c.get("component").and_then(Value::as_str) else {
                return Err(Error::Validation(format!(
                    "Component '{id}' on surface '{surface_id}' names no component type."
                )));
            };
            if self
                .catalog_for(c, surface_catalog.as_deref(), kind)
                .is_none()
            {
                return Err(Error::Validation(format!(
                    "Component '{id}' is a '{kind}', which no active catalog declares."
                )));
            }
            self.note(surface_id, id);
            let surface = self.surfaces.get_mut(surface_id).expect("checked");
            let old = surface.components.insert(id.to_owned(), c.clone());
            if old.is_none() {
                surface.order.push(id.to_owned());
            }
            self.record(Undo::Component(surface_id.to_owned(), id.to_owned(), old));
        }
        Ok(())
    }

    /// Records a component's state before its first change since the last
    /// check, and that it changed.
    fn note(&mut self, surface_id: &str, id: &str) {
        let already = self.changed.get(surface_id).is_some_and(|c| c.contains(id));
        if already {
            return;
        }
        self.save_book(surface_id);
        let before = self.surfaces[surface_id].components.get(id).cloned();
        self.baseline
            .entry(surface_id.to_owned())
            .or_default()
            .entry(id.to_owned())
            .or_insert(before);
        self.changed
            .entry(surface_id.to_owned())
            .or_default()
            .insert(id.to_owned());
    }

    fn forget_changed(&mut self, surface_id: &str) {
        if self.changed.contains_key(surface_id) || self.baseline.contains_key(surface_id) {
            self.save_book(surface_id);
            self.changed.remove(surface_id);
            self.baseline.remove(surface_id);
        }
    }

    /// The catalog a component resolves to: its own `catalogId`, else the
    /// surface's, else the first active catalog declaring it.
    fn catalog_for(&self, c: &Component, surface: Option<&str>, kind: &str) -> Option<&Catalog> {
        let own = c
            .get("catalogId")
            .or_else(|| c.get("@catalogId"))
            .and_then(Value::as_str);
        if let Some(id) = own.or(surface) {
            return self.catalogs.get(id).filter(|cat| cat.has_component(kind));
        }
        self.catalogs
            .catalogs()
            .iter()
            .find(|cat| cat.has_component(kind))
    }

    // ---- checks -------------------------------------------------------------

    /// Stops holding the components each surface changed against later
    /// checks: a renderer that received them keeps them whatever follows.
    pub fn mark_checked<'s>(&mut self, surface_ids: impl IntoIterator<Item = &'s str>) {
        for id in surface_ids {
            self.forget_changed(id);
        }
    }

    /// Checks the whole tree of each surface: its root exists, every child
    /// reference resolves, nothing is its own ancestor, and every component
    /// changed since the last check is reachable from the root. A surface
    /// without components yet, or one that no longer exists, passes.
    pub fn check_topology<'s>(
        &mut self,
        surface_ids: impl IntoIterator<Item = &'s str> + Clone,
    ) -> Result<()> {
        for surface_id in surface_ids.clone() {
            let Some(surface) = self.surfaces.get(surface_id) else {
                continue;
            };
            if surface.components.is_empty() {
                continue;
            }
            let reachable = self.walk(surface_id, surface)?;
            let mut unreachable: Vec<&String> = self
                .changed
                .get(surface_id)
                .into_iter()
                .flatten()
                .filter(|id| surface.components.contains_key(*id) && !reachable.contains(*id))
                .collect();
            unreachable.sort();
            if let Some(id) = unreachable.first() {
                return Err(Error::Validation(format!(
                    "Component '{id}' is not reachable from 'root' (Orphaned component '{id}' is not reachable from root)"
                )));
            }
        }
        self.mark_checked(surface_ids);
        Ok(())
    }

    /// Every reference resolves and there is no cycle; returns what the root
    /// reaches.
    fn walk(&self, surface_id: &str, surface: &Surface) -> Result<HashSet<String>> {
        if !surface.components.contains_key("root") {
            return Err(Error::Validation(format!(
                "Missing root component: surface '{surface_id}' has no component with id 'root'."
            )));
        }
        for id in &surface.order {
            let Some(c) = surface.components.get(id) else {
                continue;
            };
            for child in self.references(surface, c) {
                if !surface.components.contains_key(&child) {
                    return Err(Error::Validation(format!(
                        "Component '{id}' references child '{child}', which surface '{surface_id}' does not hold."
                    )));
                }
            }
        }
        // Depth-first from the root, with the path on a stack for cycles.
        let mut reached: HashSet<String> = HashSet::new();
        let mut on_path: HashSet<String> = HashSet::new();
        let mut stack: Vec<(String, Vec<String>, usize)> = Vec::new();
        let children = |id: &str| self.references(surface, &surface.components[id]);
        reached.insert("root".into());
        on_path.insert("root".into());
        stack.push(("root".into(), children("root"), 0));
        while let Some((id, kids, i)) = stack.last_mut() {
            if *i >= kids.len() {
                on_path.remove(id.as_str());
                stack.pop();
                continue;
            }
            let child = kids[*i].clone();
            *i += 1;
            if on_path.contains(&child) {
                return Err(Error::Validation(format!(
                    "Circular reference: component '{child}' is its own ancestor on surface '{surface_id}'."
                )));
            }
            if reached.insert(child.clone()) {
                on_path.insert(child.clone());
                let next = children(&child);
                stack.push((child, next, 0));
            }
        }
        Ok(reached)
    }

    /// The component ids a component names in its child slots.
    fn references(&self, surface: &Surface, c: &Component) -> Vec<String> {
        let mut out = Vec::new();
        let Some(kind) = c.get("component").and_then(Value::as_str) else {
            return out;
        };
        let Some(cat) = self.catalog_for(c, surface.catalog_id.as_deref(), kind) else {
            return out;
        };
        for (prop, value) in c {
            if prop == "id" || prop == "component" {
                continue;
            }
            if let Some(schema) = cat.property_schema(kind, prop) {
                collect_refs(cat, schema, value, &mut out, 0);
            }
        }
        out
    }

    /// Reverts a component a streamed statement emitted under a provisional
    /// id that a later step replaced: to what it was before this turn, or
    /// away when it is new this turn.
    pub fn drop_provisional_component(&mut self, surface_id: &str, id: &str) {
        if !self.surfaces.contains_key(surface_id) {
            return;
        }
        let Some(base) = self
            .baseline
            .get(surface_id)
            .and_then(|b| b.get(id))
            .cloned()
        else {
            return;
        };
        self.save_book(surface_id);
        if let Some(b) = self.baseline.get_mut(surface_id) {
            b.remove(id);
        }
        if let Some(c) = self.changed.get_mut(surface_id) {
            c.remove(id);
        }
        let surface = self.surfaces.get_mut(surface_id).expect("checked");
        let old = match base {
            Some(component) => surface.components.insert(id.to_owned(), component),
            None => {
                let old = surface.components.remove(id);
                if old.is_some() {
                    if let Some(p) = surface.order.iter().position(|x| x == id) {
                        surface.order.remove(p);
                    }
                }
                old
            }
        };
        self.record(Undo::Component(surface_id.to_owned(), id.to_owned(), old));
    }
}

fn versions_match(stated: &str, wire: &str) -> bool {
    let norm = |v: &str| v.trim_start_matches('v').to_owned();
    let (a, b) = (norm(stated), norm(wire));
    a == b || (a.starts_with("0.9") && b.starts_with("0.9"))
}

/// The component ids `value` holds where `schema` declares child slots.
fn collect_refs(cat: &Catalog, schema: &Value, value: &Value, out: &mut Vec<String>, depth: u32) {
    if depth > 64 {
        return;
    }
    match cat.slot_kind(schema) {
        Some(SlotKind::Child) => {
            if let Value::String(s) = value {
                out.push(s.clone());
            }
            return;
        }
        Some(SlotKind::ChildList) => {
            match value {
                Value::Array(items) => {
                    out.extend(items.iter().filter_map(Value::as_str).map(str::to_owned));
                }
                Value::Object(map) => {
                    if let Some(Value::String(id)) = map.get("componentId") {
                        out.push(id.clone());
                    }
                }
                _ => {}
            }
            return;
        }
        Some(SlotKind::Action) => return,
        None => {}
    }
    match value {
        Value::Array(items) => {
            if let Some(item) = cat.resolve_item_schema(schema) {
                for v in items {
                    collect_refs(cat, item, v, out, depth + 1);
                }
            }
        }
        Value::Object(map) => {
            for (k, v) in map {
                if let Some(sub) = cat.resolve_property_schema(schema, k) {
                    collect_refs(cat, sub, v, out, depth + 1);
                }
            }
        }
        _ => {}
    }
}
