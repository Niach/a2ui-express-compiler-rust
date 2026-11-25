//! The `at_exponential_express` extension: theme, style shorthand, motion
//! sugar and patches on top of A2UI Express.
//!
//! Everything here is off unless a catalog declares the extension in its
//! `metadata.extensions.at_exponential_express` (an object, or `true`). With
//! no catalog declaring it, the compiler, decompiler and response reader are
//! upstream Express, byte for byte. With it, a block may also use:
//!
//! - `theme(base="dark", primary=#eef800, …)`: seeds expanded into an eUI
//!   theme ([`theme`]); compiled into the `createSurface`'s
//!   `metadata.extensions.at_exponential_theme`, or on a surface that
//!   already exists into a `callRendererFunction` of the catalog's
//!   `setTheme` function;
//! - `#rrggbb` / `#rrggbbaa` colour literals, inside `theme(...)` only;
//! - `style(name, "col g12 p16")` classes and `s=name` / `s="…"` on any
//!   component ([`style`]);
//! - `enter=`, `exit=`, `stagger=`, `delay=` on any component, and
//!   `Motion(ref="name")` / `Motion(kf="…")` ([`motion`]);
//! - [`patch`]: `compile_patch`, a block against a surface that exists.
//!
//! All of it is expanded at compile time ([`Desugarer`]): the messages are
//! plain A2UI 1.0. The decompiler writes the sugar back
//! ([`crate::decompile`]), canonically, so `decompile(compile(x))` is a fixed
//! point. `docs/express-extensions.md` is the reference.

pub mod color;
pub mod motion;
pub mod patch;
pub mod style;
pub mod theme;

use std::collections::HashMap;

use serde_json::{Map, Value};

use crate::compile::CompileError;
use crate::schema::CatalogSet;
use crate::syntax::{Arg, Call, Expr, ExprKind, MapEntry, Number, NumberValue, Span, Statement, StatementKind};

/// The catalog extension key that turns the extension on.
pub const EXTENSION: &str = "at_exponential_express";
/// The `createSurface.metadata.extensions` key a theme goes under (#VAPP-289).
pub const THEME_EXTENSION: &str = "at_exponential_theme";
/// The renderer function that changes a live surface's theme (agreed with
/// #VAPP-289, 2026-11-26).
pub const DEFAULT_THEME_FUNCTION: &str = "setTheme";

/// The component keywords the extension reads, when the component does not
/// declare a property of that name itself.
pub const SUGAR: &[&str] = &["s", "enter", "exit", "stagger", "delay"];

/// The extension as a catalog declares it.
#[derive(Debug, Clone, PartialEq)]
pub struct Express {
    /// `motions`: the `Motion(ref=…)` registry, name → Motion props.
    pub motions: Map<String, Value>,
    /// `themeFunction`: the renderer function a live theme change calls.
    pub theme_function: String,
    /// The id of the catalog that declares it.
    pub catalog_id: String,
}

impl Express {
    /// The extension declared by the first of `catalogs` that declares it.
    pub fn from_catalogs(catalogs: &CatalogSet) -> Option<Express> {
        catalogs.catalogs().iter().find_map(|c| {
            let decl = c.document().pointer(&format!("/metadata/extensions/{EXTENSION}"))?;
            let obj = match decl {
                Value::Bool(true) => Map::new(),
                Value::Object(o) => o.clone(),
                _ => return None,
            };
            Some(Express {
                motions: obj.get("motions").and_then(Value::as_object).cloned().unwrap_or_default(),
                theme_function: obj
                    .get("themeFunction")
                    .and_then(Value::as_str)
                    .unwrap_or(DEFAULT_THEME_FUNCTION)
                    .to_owned(),
                catalog_id: c.id().to_owned(),
            })
        })
    }
}

// ---- values -------------------------------------------------------------------

/// A JSON value as an Express literal.
pub fn json_to_expr(v: &Value, span: Span) -> Expr {
    let kind = match v {
        Value::Null => ExprKind::Null,
        Value::Bool(b) => ExprKind::Bool(*b),
        Value::Number(n) => {
            let value = match n.as_i64() {
                Some(i) => NumberValue::Int(i),
                None => NumberValue::Float(n.as_f64().unwrap_or(0.0)),
            };
            ExprKind::Number(Number {
                text: n.to_string(),
                value,
            })
        }
        Value::String(s) => ExprKind::String(s.clone()),
        Value::Array(items) => ExprKind::List(items.iter().map(|i| json_to_expr(i, span)).collect()),
        Value::Object(m) => ExprKind::Map(
            m.iter()
                .map(|(k, v)| MapEntry {
                    key: k.clone(),
                    key_span: span,
                    value: json_to_expr(v, span),
                })
                .collect(),
        ),
    };
    Expr { kind, span }
}

/// A literal Express value as JSON (colour literals as their `#hex`
/// string); `None` for anything that is not a literal.
pub fn literal_json(e: &Expr) -> Option<Value> {
    Some(match &e.kind {
        ExprKind::Null => Value::Null,
        ExprKind::Bool(b) => Value::Bool(*b),
        ExprKind::Number(n) => Value::Number(n.to_json()),
        ExprKind::String(s) | ExprKind::Color(s) => Value::String(s.clone()),
        ExprKind::List(items) => Value::Array(items.iter().map(literal_json).collect::<Option<_>>()?),
        ExprKind::Map(entries) => {
            let mut m = Map::new();
            for e in entries {
                m.insert(e.key.clone(), literal_json(&e.value)?);
            }
            Value::Object(m)
        }
        _ => return None,
    })
}

fn invalid(message: impl Into<String>) -> CompileError {
    CompileError::validation(message)
}

/// The keyword arguments of an extension statement as JSON.
pub fn call_kwargs(call: &Call, what: &str) -> Result<Map<String, Value>, CompileError> {
    let mut out = Map::new();
    for arg in &call.args {
        let Some((name, _)) = &arg.name else {
            continue;
        };
        let v = literal_json(&arg.value).ok_or_else(|| {
            invalid(format!("{what} argument '{name}' must be a literal value (a colour, number, string, list or map)."))
        })?;
        out.insert(name.clone(), v);
    }
    Ok(out)
}

/// What a `theme(...)` statement sets: a built-in theme id
/// (`theme("orange")`) or the expanded theme.
pub fn theme_value(call: &Call) -> Result<Value, CompileError> {
    let positional: Vec<&Expr> = call.positional().collect();
    let kwargs = call_kwargs(call, "theme()")?;
    match positional.as_slice() {
        [] => theme::expand(&kwargs).map_err(invalid),
        [id] => match (&id.kind, kwargs.is_empty()) {
            (ExprKind::String(s), true) if !s.is_empty() => Ok(Value::String(s.clone())),
            (ExprKind::String(_), false) => Err(invalid(
                "theme(\"id\") names a built-in theme and takes no other arguments; to start from one, write theme(extends=\"id\", …).",
            )),
            _ => Err(invalid("theme() takes keyword seeds, or one string: a built-in theme id.")),
        },
        _ => Err(invalid("theme() takes keyword seeds, or one string: a built-in theme id.")),
    }
}

// ---- desugaring -----------------------------------------------------------------

/// Expands the extension's statements and keywords into plain Express, one
/// statement at a time (so a stream can desugar each statement as it
/// arrives). Holds the style classes defined so far.
#[derive(Debug, Clone, Default)]
pub struct Desugarer {
    classes: HashMap<String, String>,
}

/// What one statement desugars to.
pub enum Desugared {
    /// The statement in plain Express (or a `theme(...)` call, which the
    /// compiler reads itself).
    Statement(Statement),
    /// A `style(...)` class definition: nothing to compile.
    Class,
}

/// What the desugarer needs to know about the catalogs.
pub trait Components {
    /// Whether `name` is a component.
    fn is_component(&self, name: &str) -> bool;
    /// Whether component `name` declares property `prop`.
    fn declares(&self, name: &str, prop: &str) -> bool;
}

impl Desugarer {
    /// The classes defined so far: name → shorthand words.
    pub fn classes(&self) -> &HashMap<String, String> {
        &self.classes
    }

    /// Desugars one statement.
    pub fn statement(
        &mut self,
        stmt: &Statement,
        ext: &Express,
        cat: &dyn Components,
    ) -> Result<Desugared, CompileError> {
        if let StatementKind::Expr(Expr {
            kind: ExprKind::Call(call),
            ..
        }) = &stmt.kind
        {
            if call.name == "style" {
                self.define_class(call)?;
                return Ok(Desugared::Class);
            }
            if call.name == "theme" {
                theme_value(call)?;
                return Ok(Desugared::Statement(stmt.clone()));
            }
        }
        let mut out = stmt.clone();
        match &mut out.kind {
            StatementKind::Assign { value, .. } => self.expr(value, ext, cat)?,
            StatementKind::Expr(value) => self.expr(value, ext, cat)?,
        }
        Ok(Desugared::Statement(out))
    }

    fn define_class(&mut self, call: &Call) -> Result<(), CompileError> {
        let pos: Vec<&Expr> = call.positional().collect();
        let usage = || invalid("style() defines a class: style(name, \"words\"), e.g. style(card, \"col g12 p16 bg-card r-lg\").");
        if pos.len() != 2 || call.keywords().next().is_some() {
            return Err(usage());
        }
        let name = match &pos[0].kind {
            ExprKind::Variable(n) => n.clone(),
            ExprKind::String(n) if crate::syntax::is_identifier(n) => n.clone(),
            _ => return Err(usage()),
        };
        let ExprKind::String(words) = &pos[1].kind else {
            return Err(usage());
        };
        if style::is_word(&name) || style::STATES.iter().any(|(s, _)| *s == name) {
            return Err(invalid(format!(
                "style class '{name}' reads as a shorthand word; name it something else."
            )));
        }
        // Check the words now, so a bad class fails where it is written.
        self.decode(words)?;
        self.classes.insert(name, words.clone());
        Ok(())
    }

    fn decode(&self, words: &str) -> Result<Map<String, Value>, CompileError> {
        let mut out = Map::new();
        let classes = &self.classes;
        style::decode_into(words, &mut out, &|w| classes.get(w).cloned(), 0).map_err(|e| {
            invalid(format!("{e}. Style words: col, row, g12, p16, px-md, bg-card, fg-mutedForeground, r-lg, sh-sm, w100%, items-center, justify-between, hover:bg-accent, …"))
        })?;
        Ok(out)
    }

    fn expr(&self, e: &mut Expr, ext: &Express, cat: &dyn Components) -> Result<(), CompileError> {
        match &mut e.kind {
            ExprKind::Call(call) => {
                for arg in &mut call.args {
                    self.expr(&mut arg.value, ext, cat)?;
                }
                if call.name.starts_with('@') || !cat.is_component(&call.name) {
                    return Ok(());
                }
                self.component(call, e.span, ext, cat)
            }
            ExprKind::List(items) => items.iter_mut().try_for_each(|i| self.expr(i, ext, cat)),
            ExprKind::Map(entries) => entries.iter_mut().try_for_each(|m| self.expr(&mut m.value, ext, cat)),
            ExprKind::Check { args, .. } => args.iter_mut().try_for_each(|a| self.expr(a, ext, cat)),
            _ => Ok(()),
        }
    }

    fn component(&self, call: &mut Call, span: Span, ext: &Express, cat: &dyn Components) -> Result<(), CompileError> {
        let name = call.name.clone();
        let sugar = |k: &str| !cat.declares(&name, k);
        if name == "Motion" {
            motion::expand_motion(call, span, ext, &|k| !cat.declares("Motion", k))?;
        }
        let mut words: Option<Map<String, Value>> = None;
        let mut motion_keys: Vec<(&'static str, Value)> = Vec::new();
        let mut kept = Vec::with_capacity(call.args.len());
        for arg in std::mem::take(&mut call.args) {
            let key = arg.name.as_ref().map(|(k, _)| k.as_str());
            match key {
                Some("s") if sugar("s") => {
                    let text = match &arg.value.kind {
                        ExprKind::String(s) => s.clone(),
                        ExprKind::Variable(v) => {
                            if !self.classes.contains_key(v) {
                                return Err(invalid(format!(
                                    "s={v} names no style class; define it first with style({v}, \"…\"), or write s=\"words\"."
                                )));
                            }
                            v.clone()
                        }
                        _ => return Err(invalid("s= takes style words in a string, or a class name.")),
                    };
                    let decoded = self.decode(&text)?;
                    let into = words.get_or_insert_with(Map::new);
                    for (k, v) in decoded {
                        into.insert(k, v);
                    }
                }
                Some(k @ ("enter" | "exit" | "stagger" | "delay")) if sugar(k) => {
                    motion_keys.push(motion::sugar_key(k, &arg.value)?);
                }
                _ => kept.push(arg),
            }
        }
        call.args = kept;
        if words.is_none() && motion_keys.is_empty() {
            return Ok(());
        }
        let mut style = words.unwrap_or_default();
        // An explicit style= map wins over the words.
        let explicit = call
            .args
            .iter()
            .position(|a| a.name.as_ref().is_some_and(|(k, _)| k == "style"));
        if let Some(i) = explicit {
            let arg = call.args.remove(i);
            let ExprKind::Map(entries) = arg.value.kind else {
                return Err(invalid("style= must be a map literal when s=, enter= or exit= is also given."));
            };
            let mut map_expr: Vec<MapEntry> = style
                .iter()
                .filter(|(k, _)| !entries.iter().any(|e| e.key == **k))
                .map(|(k, v)| MapEntry {
                    key: k.clone(),
                    key_span: span,
                    value: json_to_expr(v, span),
                })
                .collect();
            map_expr.extend(entries);
            for (k, v) in &motion_keys {
                map_expr.retain(|e| e.key != *k);
                map_expr.push(MapEntry {
                    key: (*k).to_owned(),
                    key_span: span,
                    value: json_to_expr(v, span),
                });
            }
            call.args.push(Arg {
                name: Some(("style".into(), span)),
                value: Expr {
                    kind: ExprKind::Map(map_expr),
                    span,
                },
            });
            return Ok(());
        }
        for (k, v) in motion_keys {
            style.insert(k.to_owned(), v);
        }
        call.args.push(Arg {
            name: Some(("style".into(), span)),
            value: json_to_expr(&Value::Object(style), span),
        });
        Ok(())
    }
}

/// Desugars a whole program.
pub fn desugar(
    statements: &[Statement],
    ext: &Express,
    cat: &dyn Components,
) -> Result<Vec<Statement>, CompileError> {
    let mut d = Desugarer::default();
    let mut out = Vec::with_capacity(statements.len());
    for s in statements {
        if let Desugared::Statement(s) = d.statement(s, ext, cat)? {
            out.push(s);
        }
    }
    Ok(out)
}
