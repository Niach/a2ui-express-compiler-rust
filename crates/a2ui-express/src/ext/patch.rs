//! `compile_patch`: a block of changes against a surface that exists.
//!
//! The *screen* is the Express of the surface as it stands (what compiled
//! to its `createSurface`, or `decompile` of its messages). The *patch* is a
//! block of statements:
//!
//! - `name = …` reassigns a variable, or adds one;
//! - `$/path = …` sets a data path;
//! - `style(name, "…")` redefines or adds a class;
//! - `theme(...)` replaces the theme;
//! - `del(name)` removes a component (and its entry in every child list
//!   that names it); `del($/path)` removes a data path.
//!
//! The patch is applied to the screen statement by statement (a statement
//! replaces the one with the same target, in place), giving the new screen.
//! Both screens are compiled, and the messages are the difference:
//! `setTheme` when the theme changed, one `updateComponents` with exactly
//! the components that are new or changed (untouched ones are never sent),
//! and one `updateDataModel` per data path that changed (`null` for a path
//! that is gone). Applied to the surface, they leave the renderer holding
//! what compiling the new screen would create
//! (`tests/extensions.rs` checks it).
//!
//! A component that is no longer reachable from `root` is not sent: A2UI 1.0
//! has no message that deletes one, and a renderer draws only the tree
//! under `root`.

use serde_json::{Map, Value, json};

use super::THEME_EXTENSION;
use crate::compile::{CompileError, Compiler, strip_sentinel_tags};
use crate::syntax::{self, Expr, ExprKind, Statement, StatementKind, Target};

/// What [`compile_patch`] returns.
#[derive(Debug, Clone, PartialEq)]
pub struct Patch {
    /// The messages that take the surface from the old screen to the new.
    pub messages: Vec<Value>,
    /// The new screen: the old one with the patch applied, the state to
    /// patch next.
    pub screen: String,
}

fn invalid(message: impl Into<String>) -> CompileError {
    CompileError::validation(message)
}

/// The identity of a statement: what a patch statement replaces.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Key {
    Var(String),
    Path(String),
    Class(String),
    Theme,
    Surface,
}

fn key_of(stmt: &Statement) -> Result<Key, CompileError> {
    Ok(match &stmt.kind {
        StatementKind::Assign {
            target: Target::Variable { name, .. },
            ..
        } => Key::Var(name.clone()),
        StatementKind::Assign {
            target: Target::Path { path, .. },
            ..
        } => Key::Path(path.clone()),
        StatementKind::Expr(Expr {
            kind: ExprKind::Call(call),
            ..
        }) => match call.name.as_str() {
            "theme" => Key::Theme,
            "surface" => Key::Surface,
            "style" => match call.positional().next().map(|e| &e.kind) {
                Some(ExprKind::Variable(n)) | Some(ExprKind::String(n)) => Key::Class(n.clone()),
                _ => return Err(invalid("style() defines a class: style(name, \"words\").")),
            },
            other => {
                return Err(invalid(format!(
                    "A patch holds assignments, style(), theme() and del(); {other}(...) is none of them."
                )));
            }
        },
        StatementKind::Expr(_) => {
            return Err(invalid(
                "A patch holds assignments, style(), theme() and del().",
            ));
        }
    })
}

/// A parsed block and the (normalized) text its spans index.
struct Block {
    text: String,
    statements: Vec<Statement>,
}

impl Block {
    fn parse(compiler: &Compiler, source: &str) -> Result<Block, CompileError> {
        let body = strip_sentinel_tags(source);
        let hex = compiler.dialect().hex_colors;
        let text = syntax::normalize::normalize_dialect(&body, &Default::default(), hex);
        let parsed =
            syntax::parser::parse_tokens(syntax::lexer::lex_with(&text, syntax::Pos::START, hex));
        let program = parsed.into_result()?;
        Ok(Block {
            text,
            statements: program.statements,
        })
    }

    fn source(&self, stmt: &Statement) -> String {
        self.text[stmt.span.start.offset..stmt.span.end.offset].to_owned()
    }
}

/// The byte ranges of `name` as an item of a list literal in `e`, each with
/// the separator next to it, and whether `name` is read anywhere else.
fn list_items(e: &Expr, name: &str, ranges: &mut Vec<(usize, usize)>, elsewhere: &mut bool) {
    match &e.kind {
        ExprKind::List(items) => {
            for (i, item) in items.iter().enumerate() {
                if matches!(&item.kind, ExprKind::Variable(v) if v == name) {
                    let range = match (items.get(i + 1), i.checked_sub(1).map(|j| &items[j])) {
                        (Some(next), _) => (item.span.start.offset, next.span.start.offset),
                        (None, Some(prev)) => (prev.span.end.offset, item.span.end.offset),
                        (None, None) => (item.span.start.offset, item.span.end.offset),
                    };
                    ranges.push(range);
                } else {
                    list_items(item, name, ranges, elsewhere);
                }
            }
        }
        ExprKind::Variable(v) if v == name => *elsewhere = true,
        ExprKind::Map(entries) => entries
            .iter()
            .for_each(|m| list_items(&m.value, name, ranges, elsewhere)),
        ExprKind::Check { args, .. } => args
            .iter()
            .for_each(|a| list_items(a, name, ranges, elsewhere)),
        ExprKind::Call(call) => call
            .args
            .iter()
            .for_each(|a| list_items(&a.value, name, ranges, elsewhere)),
        _ => {}
    }
}

/// `text` with the byte ranges (relative to `base`) cut out.
fn cut(text: &str, base: usize, mut ranges: Vec<(usize, usize)>) -> String {
    ranges.sort();
    let mut out = String::new();
    let mut at = 0;
    for (s, e) in ranges {
        let (s, e) = (s - base, e - base);
        if s >= at {
            out.push_str(&text[at..s]);
            at = e;
        }
    }
    out.push_str(&text[at..]);
    out
}

/// One entry of the new screen.
struct Line {
    key: Key,
    text: String,
    /// The parsed statement, when the text is still the one it was parsed
    /// from (for `del`'s reference search).
    stmt: Option<(Statement, usize)>,
}

/// Applies `patch` to `screen` and returns the new screen's source.
pub fn apply(compiler: &Compiler, screen: &str, patch: &str) -> Result<String, CompileError> {
    let old = Block::parse(compiler, screen)?;
    let new = Block::parse(compiler, patch)?;
    let mut lines: Vec<Line> = Vec::new();
    for s in &old.statements {
        let key = key_of(s).map_err(|e| invalid(format!("The screen: {}", e.message)))?;
        if key == Key::Surface && lines.iter().any(|l| l.key == Key::Surface) {
            return Err(invalid(
                "compile_patch takes the screen of one surface; this one has two surface(...) lines.",
            ));
        }
        lines.push(Line {
            key,
            text: old.source(s),
            stmt: Some((s.clone(), s.span.start.offset)),
        });
    }
    for s in &new.statements {
        if let StatementKind::Expr(Expr {
            kind: ExprKind::Call(call),
            ..
        }) = &s.kind
        {
            if call.name == "del" {
                delete(&mut lines, call)?;
                continue;
            }
            if call.name == "surface" {
                return Err(invalid(
                    "A patch changes the surface its screen creates; leave out surface(...).",
                ));
            }
        }
        let key = key_of(s)?;
        let text = new.source(s);
        match lines.iter_mut().find(|l| l.key == key) {
            Some(line) => {
                line.text = text;
                line.stmt = None;
            }
            None => {
                // A theme or class goes before the components that use it.
                let at = match key {
                    Key::Theme | Key::Class(_) => lines
                        .iter()
                        .position(|l| !matches!(l.key, Key::Surface | Key::Theme | Key::Class(_)))
                        .unwrap_or(lines.len()),
                    _ => lines.len(),
                };
                lines.insert(
                    at,
                    Line {
                        key,
                        text,
                        stmt: None,
                    },
                );
            }
        }
    }
    Ok(lines
        .into_iter()
        .map(|l| l.text)
        .collect::<Vec<_>>()
        .join("\n"))
}

fn delete(lines: &mut Vec<Line>, call: &syntax::Call) -> Result<(), CompileError> {
    let usage =
        || invalid("del() takes the component to remove, del(name), or a data path, del($/path).");
    let mut pos = call.positional();
    let (Some(target), None) = (pos.next(), pos.next()) else {
        return Err(usage());
    };
    let key = match &target.kind {
        ExprKind::Variable(name) => Key::Var(name.clone()),
        ExprKind::String(name) => Key::Var(name.clone()),
        ExprKind::Path(path) => Key::Path(path.clone()),
        _ => return Err(usage()),
    };
    let Some(i) = lines.iter().position(|l| l.key == key) else {
        return Err(invalid(match &key {
            Key::Var(n) => format!("del({n}): the screen assigns no variable {n}."),
            Key::Path(p) => format!("del(${p}): the screen assigns no data path ${p}."),
            _ => unreachable!(),
        }));
    };
    lines.remove(i);
    let Key::Var(name) = key else {
        return Ok(());
    };
    for line in lines.iter_mut() {
        let mut ranges = Vec::new();
        let mut elsewhere = false;
        match &line.stmt {
            Some((stmt, base)) => {
                let value = match &stmt.kind {
                    StatementKind::Assign { value, .. } | StatementKind::Expr(value) => value,
                };
                list_items(value, &name, &mut ranges, &mut elsewhere);
                if elsewhere {
                    return Err(invalid(format!(
                        "del({name}): {} still names it outside a child list; reassign it in the patch.",
                        match &line.key {
                            Key::Var(v) => v.clone(),
                            _ => "a statement".into(),
                        }
                    )));
                }
                if !ranges.is_empty() {
                    line.text = cut(&line.text, *base, ranges);
                    let reparsed = syntax::parser::parse_tokens(syntax::lexer::lex_with(
                        &line.text,
                        syntax::Pos::START,
                        true,
                    ));
                    line.stmt = reparsed
                        .program
                        .statements
                        .into_iter()
                        .next()
                        .map(|s| (s, 0));
                }
            }
            None => {
                // A statement the patch wrote: it must not name what it deletes.
                if line
                    .text
                    .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                    .any(|w| w == name)
                {
                    let probe = syntax::parser::parse_tokens(syntax::lexer::lex_with(
                        &line.text,
                        syntax::Pos::START,
                        true,
                    ));
                    for s in &probe.program.statements {
                        let value = match &s.kind {
                            StatementKind::Assign { value, .. } | StatementKind::Expr(value) => {
                                value
                            }
                        };
                        let mut r = Vec::new();
                        let mut e = false;
                        list_items(value, &name, &mut r, &mut e);
                        if e || !r.is_empty() {
                            return Err(invalid(format!(
                                "del({name}): the patch still names {name}."
                            )));
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// The `createSurface` of a screen's messages.
fn created(messages: &[Value], which: &str) -> Result<Map<String, Value>, CompileError> {
    match messages {
        [m] => m
            .get("createSurface")
            .and_then(Value::as_object)
            .cloned()
            .ok_or_else(|| {
                invalid(format!(
                    "The {which} screen must create one surface (assign root)."
                ))
            }),
        _ => Err(invalid(format!(
            "The {which} screen must compile to one createSurface, not {} messages.",
            messages.len()
        ))),
    }
}

/// The data paths of `old` → `new` that changed: `(pointer, value)`, a
/// removed path as `null`.
fn data_delta(old: &Value, new: &Value, path: String, out: &mut Vec<(String, Value)>) {
    match (old, new) {
        (a, b) if a == b => {}
        (Value::Object(a), Value::Object(b)) => {
            for (k, v) in b {
                let p = format!("{path}/{}", k.replace('~', "~0").replace('/', "~1"));
                match a.get(k) {
                    Some(w) => data_delta(w, v, p, out),
                    None => out.push((p, v.clone())),
                }
            }
            for k in a.keys() {
                if !b.contains_key(k) {
                    out.push((
                        format!("{path}/{}", k.replace('~', "~0").replace('/', "~1")),
                        Value::Null,
                    ));
                }
            }
        }
        (_, b) => out.push((if path.is_empty() { "/".into() } else { path }, b.clone())),
    }
}

fn theme_of(cs: &Map<String, Value>) -> Option<&Value> {
    cs.get("metadata")?.get("extensions")?.get(THEME_EXTENSION)
}

/// Compiles `patch` against `screen` (see the module docs).
pub fn compile_patch(
    compiler: &Compiler,
    screen: &str,
    patch: &str,
    surface_id: &str,
) -> Result<Patch, CompileError> {
    if compiler.extension().is_none() {
        return Err(invalid(format!(
            "compile_patch needs a catalog that declares the {} extension.",
            super::EXTENSION
        )));
    }
    let new_screen = apply(compiler, screen, patch)?;
    let before = created(&compiler.compile(screen, surface_id)?, "old")?;
    let after_messages = compiler.compile(&new_screen, surface_id)?;
    let after = created(&after_messages, "new")?;
    let sid = after
        .get("surfaceId")
        .cloned()
        .unwrap_or_else(|| surface_id.into());
    if before.get("surfaceId") != after.get("surfaceId") {
        return Err(invalid(
            "A patch cannot move the screen to another surface.",
        ));
    }
    let version = compiler.version().wire();
    let mut messages = Vec::new();

    if theme_of(&before) != theme_of(&after) {
        let theme = theme_of(&after)
            .cloned()
            .unwrap_or_else(|| Value::String("neutral".into()));
        let mut ctx = crate::compile::pass_ctx();
        messages.push(compiler.theme_call(sid.as_str().unwrap_or(surface_id), &theme, &mut ctx)?);
    }

    let list = |cs: &Map<String, Value>| -> Vec<Map<String, Value>> {
        cs.get("components")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|c| c.as_object().cloned()).collect())
            .unwrap_or_default()
    };
    let old_components = list(&before);
    let changed: Vec<Value> = list(&after)
        .into_iter()
        .filter(|c| !old_components.iter().any(|o| o == c))
        .map(Value::Object)
        .collect();
    if !changed.is_empty() {
        messages.push(json!({"version": version, "updateComponents": {"surfaceId": sid, "components": changed}}));
    }

    let empty = Value::Object(Map::new());
    let mut data = Vec::new();
    data_delta(
        before.get("dataModel").unwrap_or(&empty),
        after.get("dataModel").unwrap_or(&empty),
        String::new(),
        &mut data,
    );
    for (path, value) in data {
        messages.push(json!({"version": version, "updateDataModel": {"surfaceId": sid, "path": path, "value": value}}));
    }
    crate::compile::validate(compiler.catalogs(), &messages)?;
    Ok(Patch {
        messages,
        screen: new_screen,
    })
}
