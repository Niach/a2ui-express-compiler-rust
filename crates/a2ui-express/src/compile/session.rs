//! Incremental compilation of one `<a2ui>` block as it streams in.
//!
//! A port of upstream's `ExpressScopeSession` (`compiler.py`, mirrored by
//! `compiler.ts`). The response stream hands the session the text of the
//! block that is complete so far, again and again as it grows; each
//! [`ScopeSession::compile_step`] returns only what is new since the last
//! step:
//!
//! - a new surface's `createSurface` once its `root` compiles, holding
//!   everything that came before `root` until then;
//! - afterwards, `updateComponents` with the components that are new or
//!   changed, and one `updateDataModel` per new or changed data path;
//! - a component that names a child not assigned yet (a forward reference
//!   in a `ComponentId`/`ChildList` slot) is emitted at once with the
//!   variable name as the child's id, and re-emitted if the child turns out
//!   to carry an explicit `id=`;
//! - a component whose value reads a variable not assigned yet waits for it.
//!
//! Each step reuses the statements of the text it has seen (only the new
//! text is parsed) and the components that cannot have changed (a variable
//! is recompiled only when its value or a variable it reads changed), so a
//! step costs about what the new statement costs, not the whole surface.
//!
//! A step is atomic: if it fails, the session is left as it was.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde_json::{Map, Value, json};

use super::data::{set_nested_path, to_json_pointer};
use super::pass::{Ctx, Pass};
use super::{CompIds, CompileError, Compiler, Scope, Unit};
use crate::syntax::{self, Expr, Statement, StatementKind, Target};

/// What a component depends on in another variable.
#[derive(Debug, Clone, PartialEq)]
enum Dep {
    /// Not assigned.
    Missing,
    /// A component, read through its id.
    Component(String),
    /// A value, read whole.
    Value(Expr),
}

/// A compiled component, shared between the cache and what was emitted.
type Comp = Arc<Map<String, Value>>;

#[derive(Debug, Clone)]
struct CompCache {
    /// The statement the variable's value came from; held, so a value at
    /// the same address is the same value.
    stmt: Arc<Statement>,
    components: Vec<Comp>,
    deps: Vec<(String, Dep)>,
    inline_start: usize,
    inline_count: usize,
    generated: Vec<String>,
}

#[derive(Debug, Clone)]
struct DataCache {
    stmt: Arc<Statement>,
    value: Value,
    deps: Vec<(String, Dep)>,
}

/// What one surface scope of the block has emitted.
#[derive(Debug, Clone, Default)]
struct ScopeState {
    surface_id: String,
    created: bool,
    emitted_components: HashMap<String, Comp>,
    emitted_data: HashMap<String, Value>,
    var_ids: HashMap<String, String>,
    cache: HashMap<String, CompCache>,
    data_cache: HashMap<String, DataCache>,
}

/// One change to the session, recorded so a failed step can be undone.
enum Undo {
    NewScope(usize),
    SurfaceId(usize, String),
    Created(usize),
    Component(usize, String, Option<Comp>),
    Data(usize, String, Option<Value>),
    VarId(usize, String, Option<String>),
    EmittedUnit(usize),
}

/// A step's messages, and how to undo it.
pub(crate) struct Step {
    /// The messages that are new since the last step.
    pub messages: Vec<Value>,
    undo: Vec<Undo>,
    has_emitted: bool,
    cache: Option<CacheUndo>,
}

/// The statement cache before a step replaced it.
struct CacheUndo {
    prefix: String,
    statements: Vec<Arc<Statement>>,
    added_known: Vec<String>,
}

/// The incremental compiler of one block.
#[derive(Debug, Default)]
pub(crate) struct ScopeSession {
    surface_id: String,
    scopes: HashMap<usize, ScopeState>,
    emitted_units: HashSet<usize>,
    /// Whether a step has emitted anything.
    pub has_emitted: bool,
    cached_prefix: String,
    cached: Vec<Arc<Statement>>,
    known: HashSet<String>,
    /// Components (surface, id) that a later step re-emitted under another
    /// id, and that no other scope holds: the caller drops them from its
    /// state. Set by each step.
    pub superseded: Vec<(String, String)>,
}

fn restore<V>(map: &mut HashMap<String, V>, key: String, old: Option<V>) {
    match old {
        Some(v) => {
            map.insert(key, v);
        }
        None => {
            map.remove(&key);
        }
    }
}

impl ScopeSession {
    /// A session for a block whose statements before any `surface(...)`
    /// line target `surface_id`.
    pub fn new(surface_id: &str) -> Self {
        ScopeSession {
            surface_id: surface_id.to_owned(),
            ..Self::default()
        }
    }

    /// Compiles what is new in `text`, the block so far.
    ///
    /// `is_final` is the last step, at the close tag: a variable still not
    /// assigned is then an error. `existing` holds the surfaces the renderer
    /// had when the block began. `cache_prefix` keeps `text`'s statements
    /// for the next step; a trial step (a healed fragment) passes false.
    pub fn compile_step(
        &mut self,
        compiler: &Compiler,
        text: &str,
        is_final: bool,
        existing: &HashSet<String>,
        cache_prefix: bool,
    ) -> Result<Step, CompileError> {
        self.superseded.clear();
        let has_emitted = self.has_emitted;
        let mut undo = Vec::new();
        match self.step(compiler, text, is_final, existing, cache_prefix, &mut undo) {
            Ok((messages, cache)) => {
                let step = Step {
                    messages,
                    undo,
                    has_emitted,
                    cache,
                };
                // What the step emits is checked like a whole block's
                // messages; a step that fails it is undone.
                match super::validate(compiler.catalogs(), &step.messages) {
                    Ok(()) => Ok(step),
                    Err(e) => {
                        self.undo(step);
                        Err(e)
                    }
                }
            }
            Err(e) => {
                self.undo(Step {
                    messages: Vec::new(),
                    undo,
                    has_emitted,
                    cache: None,
                });
                Err(e)
            }
        }
    }

    /// Undoes a step whose messages the caller could not apply.
    pub fn undo(&mut self, step: Step) {
        self.has_emitted = step.has_emitted;
        self.superseded.clear();
        for u in step.undo.into_iter().rev() {
            self.revert(u);
        }
        if let Some(c) = step.cache {
            self.cached_prefix = c.prefix;
            self.cached = c.statements;
            for name in c.added_known {
                self.known.remove(&name);
            }
        }
    }

    fn revert(&mut self, undo: Undo) {
        match undo {
            Undo::NewScope(i) => {
                self.scopes.remove(&i);
            }
            Undo::SurfaceId(i, old) => {
                if let Some(s) = self.scopes.get_mut(&i) {
                    s.surface_id = old;
                }
            }
            Undo::Created(i) => {
                if let Some(s) = self.scopes.get_mut(&i) {
                    s.created = false;
                }
            }
            Undo::Component(i, id, old) => {
                if let Some(s) = self.scopes.get_mut(&i) {
                    restore(&mut s.emitted_components, id, old);
                }
            }
            Undo::Data(i, path, old) => {
                if let Some(s) = self.scopes.get_mut(&i) {
                    restore(&mut s.emitted_data, path, old);
                }
            }
            Undo::VarId(i, var, old) => {
                if let Some(s) = self.scopes.get_mut(&i) {
                    restore(&mut s.var_ids, var, old);
                }
            }
            Undo::EmittedUnit(i) => {
                self.emitted_units.remove(&i);
            }
        }
    }

    /// The statements of `text`, parsing only what follows the text the
    /// last cached step saw.
    fn statements(&self, text: &str) -> Result<Vec<Arc<Statement>>, CompileError> {
        if !self.cached_prefix.is_empty()
            && text.starts_with(&self.cached_prefix)
            && is_prefix_boundary(text, self.cached_prefix.len())
        {
            let delta = &text[self.cached_prefix.len()..];
            let mut out = self.cached.clone();
            if !delta.trim().is_empty() {
                let parsed = syntax::parse_at(delta, syntax::Pos::START, &self.known);
                let program = parsed.into_result()?;
                out.extend(program.statements.into_iter().map(Arc::new));
            }
            return Ok(out);
        }
        let program = syntax::parse(text).into_result()?;
        Ok(program.statements.into_iter().map(Arc::new).collect())
    }

    fn step(
        &mut self,
        compiler: &Compiler,
        text: &str,
        is_final: bool,
        existing: &HashSet<String>,
        cache_prefix: bool,
        undo: &mut Vec<Undo>,
    ) -> Result<(Vec<Value>, Option<CacheUndo>), CompileError> {
        let statements = self.statements(text)?;
        let (scopes, units) =
            compiler.build_units(statements.iter().map(|s| &**s), &self.surface_id)?;
        if units.is_empty() {
            if is_final {
                return Err(CompileError::undefined_root("root"));
            }
            return Ok((Vec::new(), None));
        }
        let mut ctx = Ctx::new(compiler.version());
        ctx.allow_pending = !is_final;
        let mut active: HashSet<String> = existing.clone();
        let mut out = Vec::new();
        for (idx, unit) in units.iter().enumerate() {
            match unit {
                Unit::Delete(surface_id) => {
                    active.remove(surface_id);
                    if self.emitted_units.insert(idx) {
                        undo.push(Undo::EmittedUnit(idx));
                        self.has_emitted = true;
                        out.push(json!({
                            "version": compiler.version().wire(),
                            "deleteSurface": {"surfaceId": surface_id},
                        }));
                    }
                }
                Unit::Call(call, scope) => {
                    if self.emitted_units.contains(&idx) {
                        continue;
                    }
                    match compiler.compile_standalone_call(
                        call,
                        scope.map(|i| &scopes[i]),
                        &mut ctx,
                    ) {
                        Ok(message) => {
                            self.emitted_units.insert(idx);
                            undo.push(Undo::EmittedUnit(idx));
                            self.has_emitted = true;
                            out.push(message);
                        }
                        Err(e) if e.is_pending() => {}
                        Err(e) => return Err(e),
                    }
                }
                Unit::Scope(i) => {
                    let scope = &scopes[*i];
                    if let std::collections::hash_map::Entry::Vacant(e) = self.scopes.entry(idx) {
                        e.insert(ScopeState {
                            surface_id: scope.surface_id.clone(),
                            ..ScopeState::default()
                        });
                        undo.push(Undo::NewScope(idx));
                    }
                    let state = self.scopes.get_mut(&idx).expect("inserted above");
                    if state.surface_id != scope.surface_id {
                        let old =
                            std::mem::replace(&mut state.surface_id, scope.surface_id.clone());
                        undo.push(Undo::SurfaceId(idx, old));
                    }
                    let exists = active.contains(&scope.surface_id);
                    let messages = self.scope_delta(
                        compiler,
                        &statements,
                        idx,
                        scope,
                        &mut ctx,
                        is_final,
                        exists,
                        undo,
                    )?;
                    out.extend(messages);
                    if self.scopes[&idx].created || exists {
                        active.insert(scope.surface_id.clone());
                    }
                }
            }
        }
        if !cache_prefix {
            return Ok((out, None));
        }
        let mut added_known = Vec::new();
        for s in &statements[self.cached.len().min(statements.len())..] {
            if let StatementKind::Assign {
                target: Target::Variable { name, .. },
                ..
            } = &s.kind
            {
                if self.known.insert(name.clone()) {
                    added_known.push(name.clone());
                }
            }
        }
        let cache = CacheUndo {
            prefix: std::mem::replace(&mut self.cached_prefix, text.to_owned()),
            statements: std::mem::replace(&mut self.cached, statements),
            added_known,
        };
        Ok((out, Some(cache)))
    }

    #[allow(clippy::too_many_arguments)]
    fn scope_delta(
        &mut self,
        compiler: &Compiler,
        statements: &[Arc<Statement>],
        idx: usize,
        scope: &Scope<'_>,
        ctx: &mut Ctx,
        is_final: bool,
        exists: bool,
        undo: &mut Vec<Undo>,
    ) -> Result<Vec<Value>, CompileError> {
        ctx.generated_ids.clear();
        ctx.generated_log.clear();
        let ids = compiler.collect_component_ids(scope)?;
        let (components, has_pending) =
            self.ready_components(compiler, statements, idx, scope, &ids, ctx, undo)?;
        let data = self.ready_data(compiler, statements, idx, scope, &ids, ctx)?;
        let state = &self.scopes[&idx];
        let has_root = components.iter().any(|c| c["id"] == "root");
        let needs_create = !scope.update
            && !state.created
            && (!exists || has_root || scope.raw_extensions.is_some());
        if needs_create {
            if !has_root {
                if !is_final {
                    return Ok(Vec::new());
                }
                if scope.raw_extensions.is_some() {
                    return Err(CompileError::validation(
                        "surface() extensions require a surface-creating scope with a root component.",
                    ));
                }
                if !components.is_empty() || !data.is_empty() {
                    return self.updates(
                        compiler, idx, scope, components, data, true, false, false, undo,
                    );
                }
                return Err(CompileError::undefined_root("root"));
            }
            return self.create(
                compiler, idx, scope, &ids, components, data, ctx, is_final, undo,
            );
        }
        self.updates(
            compiler,
            idx,
            scope,
            components,
            data,
            is_final,
            has_pending,
            exists,
            undo,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn create(
        &mut self,
        compiler: &Compiler,
        idx: usize,
        scope: &Scope<'_>,
        ids: &CompIds,
        components: Vec<Comp>,
        data: Vec<(String, Value)>,
        ctx: &mut Ctx,
        is_final: bool,
        undo: &mut Vec<Undo>,
    ) -> Result<Vec<Value>, CompileError> {
        let mut data_model = Map::new();
        for (path, value) in &data {
            set_nested_path(&mut data_model, path, value.clone())?;
        }
        let metadata = match scope.raw_extensions {
            Some(raw) => {
                let mut pass = Pass::new(compiler, &scope.syms, ids, ctx);
                match pass.extensions_map(raw, &format!("surface '{}'", scope.surface_id)) {
                    Ok(ext) => Some(json!({ "extensions": ext })),
                    Err(e) if e.is_pending() && !is_final => return Ok(Vec::new()),
                    Err(e) => return Err(e),
                }
            }
            None => None,
        };
        let messages = compiler.create_messages(
            scope,
            components
                .iter()
                .map(|c| Value::Object((**c).clone()))
                .collect(),
            data_model,
            metadata,
        );
        let state = self.scopes.get_mut(&idx).expect("scope state");
        for (path, value) in data {
            let old = state.emitted_data.insert(path.clone(), value);
            undo.push(Undo::Data(idx, path, old));
        }
        for c in components {
            let id = component_id(&c);
            let old = state.emitted_components.insert(id.clone(), c);
            undo.push(Undo::Component(idx, id, old));
        }
        state.created = true;
        undo.push(Undo::Created(idx));
        self.has_emitted = true;
        Ok(messages)
    }

    #[allow(clippy::too_many_arguments)]
    fn updates(
        &mut self,
        compiler: &Compiler,
        idx: usize,
        scope: &Scope<'_>,
        components: Vec<Comp>,
        data: Vec<(String, Value)>,
        is_final: bool,
        has_pending: bool,
        exists: bool,
        undo: &mut Vec<Undo>,
    ) -> Result<Vec<Value>, CompileError> {
        let version = compiler.version().wire();
        let state = self.scopes.get_mut(&idx).expect("scope state");
        let mut delta = Vec::new();
        for c in components {
            let id = component_id(&c);
            if state
                .emitted_components
                .get(&id)
                .is_some_and(|e| Arc::ptr_eq(e, &c) || **e == *c)
            {
                continue;
            }
            delta.push(Value::Object((*c).clone()));
            let old = state.emitted_components.insert(id.clone(), c);
            undo.push(Undo::Component(idx, id, old));
        }
        let mut data_updates = Vec::new();
        for (path, value) in data {
            if state.emitted_data.get(&path) == Some(&value) {
                continue;
            }
            let pointer = to_json_pointer(&path);
            if pointer == "/" && !value.is_object() {
                return Err(CompileError::validation(
                    "Cannot assign a non-map value to the root data path '$/'.",
                ));
            }
            let old = state.emitted_data.insert(path.clone(), value.clone());
            undo.push(Undo::Data(idx, path, old));
            data_updates.push(json!({
                "version": version,
                "updateDataModel": {
                    "surfaceId": scope.surface_id,
                    "path": pointer,
                    "value": value,
                },
            }));
        }
        if is_final
            && state.emitted_components.is_empty()
            && state.emitted_data.is_empty()
            && !has_pending
        {
            return Err(CompileError::undefined_root("root"));
        }
        let mut out = Vec::new();
        if !delta.is_empty() {
            if !is_final && !scope.directive && !state.created && !exists {
                return Err(CompileError::undefined_root("root"));
            }
            out.push(json!({
                "version": version,
                "updateComponents": {
                    "surfaceId": scope.surface_id,
                    "components": delta,
                },
            }));
        }
        out.extend(data_updates);
        if !out.is_empty() {
            self.has_emitted = true;
        }
        Ok(out)
    }

    /// What a variable is to the variables that read it.
    fn dep(scope: &Scope<'_>, ids: &CompIds, name: &str) -> Dep {
        match (ids.get(name), scope.syms.get(name)) {
            (Some(id), _) => Dep::Component(id.to_owned()),
            (None, Some(value)) => Dep::Value(value.clone()),
            (None, None) => Dep::Missing,
        }
    }

    fn dep_matches(scope: &Scope<'_>, ids: &CompIds, name: &str, dep: &Dep) -> bool {
        match (dep, ids.get(name), scope.syms.get(name)) {
            (Dep::Component(old), Some(id), _) => old == id,
            (Dep::Value(old), None, Some(value)) => old == value,
            (Dep::Missing, None, None) => true,
            _ => false,
        }
    }

    fn deps(scope: &Scope<'_>, ids: &CompIds, referenced: &mut Vec<String>) -> Vec<(String, Dep)> {
        referenced.sort_unstable();
        referenced.dedup();
        referenced
            .drain(..)
            .map(|name| {
                let dep = Self::dep(scope, ids, &name);
                (name, dep)
            })
            .collect()
    }

    /// Every component of the scope that compiles now, in statement order
    /// (each variable's component followed by its inline children), and
    /// whether a variable is waiting for another.
    #[allow(clippy::too_many_arguments)]
    fn ready_components(
        &mut self,
        compiler: &Compiler,
        statements: &[Arc<Statement>],
        idx: usize,
        scope: &Scope<'_>,
        ids: &CompIds,
        ctx: &mut Ctx,
        undo: &mut Vec<Undo>,
    ) -> Result<(Vec<Comp>, bool), CompileError> {
        let mut compiled: Vec<Comp> = Vec::new();
        let mut has_pending = false;
        let mut superseded: Vec<String> = Vec::new();
        for (var, ast) in scope.syms.iter() {
            let state = self.scopes.get_mut(&idx).expect("scope state");
            let hit = state.cache.get(var).filter(|c| {
                same_value(&c.stmt, ast)
                    && (c.inline_count == 0 || c.inline_start == ctx.inline_counter)
                    && c.generated.iter().all(|g| {
                        !ctx.generated_ids.contains(g) && !ids.has_id(g) && !scope.syms.contains(g)
                    })
                    && c.deps
                        .iter()
                        .all(|(n, d)| Self::dep_matches(scope, ids, n, d))
            });
            let components = match hit {
                Some(c) => {
                    ctx.inline_counter += c.inline_count;
                    for g in &c.generated {
                        ctx.generated_ids.insert(g.clone());
                        ctx.generated_log.push(g.clone());
                    }
                    c.components.clone()
                }
                None => {
                    let inline_start = ctx.inline_counter;
                    let log_start = ctx.generated_log.len();
                    ctx.referenced.clear();
                    let result = Pass::new(compiler, &scope.syms, ids, ctx).ast_node(var, ast);
                    let extras = std::mem::take(&mut ctx.extra_components);
                    let component = match result {
                        Ok(c) => c,
                        Err(e) if e.is_pending() => {
                            ctx.inline_counter = inline_start;
                            for g in ctx.generated_log.drain(log_start..) {
                                ctx.generated_ids.remove(&g);
                            }
                            has_pending = true;
                            continue;
                        }
                        Err(e) => return Err(e),
                    };
                    let components: Vec<Comp> = match component {
                        Some(c) => std::iter::once(c).chain(extras).map(Arc::new).collect(),
                        None => Vec::new(),
                    };
                    let deps = Self::deps(scope, ids, &mut ctx.referenced);
                    state.cache.insert(
                        var.to_owned(),
                        CompCache {
                            stmt: statement_of(statements, ast),
                            components: components.clone(),
                            deps,
                            inline_start,
                            inline_count: ctx.inline_counter - inline_start,
                            generated: ctx.generated_log[log_start..].to_vec(),
                        },
                    );
                    components
                }
            };
            if let Some(first) = components.first() {
                let new_id = component_id(first);
                let old = state.var_ids.insert(var.to_owned(), new_id.clone());
                if old.as_deref() != Some(new_id.as_str()) {
                    if let Some(old_id) = &old {
                        superseded.push(old_id.clone());
                    }
                    undo.push(Undo::VarId(idx, var.to_owned(), old));
                }
                compiled.extend(components);
            }
        }
        if !superseded.is_empty() {
            let active: HashSet<String> = compiled.iter().map(|c| component_id(c)).collect();
            for old_id in superseded {
                if active.contains(&old_id) {
                    continue;
                }
                let state = self.scopes.get_mut(&idx).expect("scope state");
                if let Some(old) = state.emitted_components.remove(&old_id) {
                    undo.push(Undo::Component(idx, old_id.clone(), Some(old)));
                }
                match self.earlier_scope_component(idx, &scope.surface_id, &old_id) {
                    Some(earlier) => compiled.push(earlier),
                    None => self.superseded.push((scope.surface_id.clone(), old_id)),
                }
            }
        }
        Ok((compiled, has_pending))
    }

    /// The component an earlier scope of the block emitted on the surface
    /// under `id`, the latest of them.
    fn earlier_scope_component(&self, current: usize, surface_id: &str, id: &str) -> Option<Comp> {
        let mut earlier: Vec<(&usize, &ScopeState)> =
            self.scopes.iter().filter(|(i, _)| **i < current).collect();
        earlier.sort_by_key(|(i, _)| **i);
        earlier
            .into_iter()
            .rev()
            .find(|(_, s)| s.surface_id == surface_id && s.emitted_components.contains_key(id))
            .map(|(_, s)| s.emitted_components[id].clone())
    }

    /// Every data path of the scope whose value compiles now.
    fn ready_data(
        &mut self,
        compiler: &Compiler,
        statements: &[Arc<Statement>],
        idx: usize,
        scope: &Scope<'_>,
        ids: &CompIds,
        ctx: &mut Ctx,
    ) -> Result<Vec<(String, Value)>, CompileError> {
        let state = self.scopes.get_mut(&idx).expect("scope state");
        let mut out = Vec::new();
        for (path, ast) in &scope.data {
            if let Some(c) = state.data_cache.get(path).filter(|c| {
                same_value(&c.stmt, ast)
                    && c.deps
                        .iter()
                        .all(|(n, d)| Self::dep_matches(scope, ids, n, d))
            }) {
                out.push((path.clone(), c.value.clone()));
                continue;
            }
            ctx.referenced.clear();
            let value = match Pass::new(compiler, &scope.syms, ids, ctx).plain_value(ast) {
                Ok(v) => v,
                Err(e) if e.is_pending() => continue,
                Err(e) => return Err(e),
            };
            let deps = Self::deps(scope, ids, &mut ctx.referenced);
            state.data_cache.insert(
                path.clone(),
                DataCache {
                    stmt: statement_of(statements, ast),
                    value: value.clone(),
                    deps,
                },
            );
            out.push((path.clone(), value));
        }
        Ok(out)
    }
}

/// The assigned value of an assignment.
fn value_of(stmt: &Statement) -> Option<&Expr> {
    match &stmt.kind {
        StatementKind::Assign { value, .. } => Some(value),
        StatementKind::Expr(_) => None,
    }
}

/// Whether `stmt` assigns `ast`: the very same value (the cache holds the
/// statement, so its address can't be reused), or an equal one.
fn same_value(stmt: &Statement, ast: &Expr) -> bool {
    value_of(stmt).is_some_and(|v| std::ptr::eq(v, ast) || v == ast)
}

/// The statement assigning `ast`, from the step's statements.
fn statement_of(statements: &[Arc<Statement>], ast: &Expr) -> Arc<Statement> {
    statements
        .iter()
        .rev()
        .find(|s| value_of(s).is_some_and(|v| std::ptr::eq(v, ast)))
        .cloned()
        .unwrap_or_else(|| {
            Arc::new(Statement {
                kind: StatementKind::Assign {
                    target: Target::Variable {
                        name: String::new(),
                        span: ast.span,
                    },
                    value: ast.clone(),
                },
                span: ast.span,
            })
        })
}

fn component_id(c: &Map<String, Value>) -> String {
    c.get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// Whether `text[..len]` ends between statements or tokens, so the
/// statements parsed from it are still statements of `text`.
fn is_prefix_boundary(text: &str, len: usize) -> bool {
    let b = text.as_bytes();
    if b.len() == len {
        return true;
    }
    if len > 0 && matches!(b[len - 1], b'\n' | b';') {
        return true;
    }
    matches!(b[len], b' ' | b'\t' | b'\r' | b'\n' | b';')
}
