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
use crate::syntax::{Call, Expr, ExprKind, MapEntry, Number, NumberValue, Span, Statement, StatementKind};

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
        let _ = ext;
        match &stmt.kind {
            StatementKind::Assign { value, .. } | StatementKind::Expr(value) => self.check_uses(value, cat)?,
        }
        Ok(Desugared::Statement(stmt.clone()))
    }

    /// A class is defined before its first use: the `s=` of the components
    /// in `e` may only name classes defined so far (as a stream sees it).
    fn check_uses(&self, e: &Expr, cat: &dyn Components) -> Result<(), CompileError> {
        match &e.kind {
            ExprKind::Call(call) => {
                if cat.is_component(&call.name) && !cat.declares(&call.name, "s") {
                    for (k, v) in call.keywords() {
                        match (&v.kind, k) {
                            (ExprKind::Variable(name), "s") if !self.classes.contains_key(name) => {
                                return Err(invalid(format!(
                                    "s={name} names no style class; define it first with style({name}, \"…\"), or write s=\"words\"."
                                )));
                            }
                            (ExprKind::String(words), "s") => {
                                decode_words(words, &self.classes)?;
                            }
                            _ => {}
                        }
                    }
                }
                call.args.iter().try_for_each(|a| self.check_uses(&a.value, cat))
            }
            ExprKind::List(items) => items.iter().try_for_each(|i| self.check_uses(i, cat)),
            ExprKind::Map(entries) => entries.iter().try_for_each(|m| self.check_uses(&m.value, cat)),
            _ => Ok(()),
        }
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
        if self.classes.contains_key(&name) {
            return Err(invalid(format!(
                "style class '{name}' is defined twice in the block; a class is defined once, before its first use."
            )));
        }
        // Check the words now, so a bad class fails where it is written.
        decode_words(words, &self.classes)?;
        self.classes.insert(name, words.clone());
        Ok(())
    }

}

/// The extension's keywords of one component, as `(prop, expr)` pairs
/// the compile pass reads: `s=`, `enter=`, `exit=`, `stagger=`, `delay=`
/// (merged into `style`), Motion `ref=` (the registry entry's props) and
/// `kf=` (`layers`). A keyword the component declares itself is left
/// alone. `pairs` loses what is replaced; the pairs returned are added.
pub(crate) fn component_pairs(
    name: &str,
    pairs: &mut Vec<(&str, &Expr)>,
    ext: &Express,
    classes: &HashMap<String, String>,
    declares: &dyn Fn(&str) -> bool,
    span: Span,
) -> Result<Vec<(String, Expr)>, CompileError> {
    let sugar = |k: &str| !declares(k);
    let given = |pairs: &Vec<(&str, &Expr)>, k: &str| {
        pairs.iter().any(|(p, e)| *p == k && !matches!(e.kind, ExprKind::Skipped))
    };
    let take = |pairs: &mut Vec<(&str, &Expr)>, k: &str| -> Option<Expr> {
        let i = pairs.iter().position(|(p, _)| *p == k)?;
        Some(pairs.remove(i).1.clone())
    };
    let mut out: Vec<(String, Expr)> = Vec::new();

    if name == "Motion" {
        if sugar("ref") {
            if let Some(r) = take(pairs, "ref") {
                let ExprKind::String(reference) = &r.kind else {
                    return Err(invalid("Motion ref= is the name of a registered animation, a string."));
                };
                let Some(Value::Object(props)) = ext.motions.get(reference) else {
                    let mut names: Vec<&str> = ext.motions.keys().map(String::as_str).collect();
                    names.sort_unstable();
                    return Err(invalid(format!(
                        "Motion ref=\"{reference}\" is not a registered animation. Registered: {}.",
                        if names.is_empty() { "none".to_owned() } else { names.join(", ") }
                    )));
                };
                for (k, v) in props {
                    if !given(pairs, k) {
                        take(pairs, k);
                        out.push((k.clone(), json_to_expr(v, span)));
                    }
                }
            }
        }
        if sugar("kf") {
            if let Some(k) = take(pairs, "kf") {
                let ExprKind::String(text) = &k.kind else {
                    return Err(invalid("Motion kf= is keyframe text in a string."));
                };
                if given(pairs, "layers") || out.iter().any(|(p, _)| p == "layers") {
                    return Err(invalid("Motion takes kf= or layers=, not both."));
                }
                take(pairs, "layers");
                let layers = motion::kf_layers(text).map_err(|e| invalid(format!("Motion kf=: {e}")))?;
                out.push(("layers".into(), json_to_expr(&layers, span)));
            }
        }
    }

    let mut style: Option<Map<String, Value>> = None;
    if sugar("s") {
        if let Some(s) = take(pairs, "s") {
            let text = match &s.kind {
                ExprKind::String(text) => text.clone(),
                ExprKind::Variable(v) => {
                    if !classes.contains_key(v) {
                        return Err(invalid(format!(
                            "s={v} names no style class; define it first with style({v}, \"…\"), or write s=\"words\"."
                        )));
                    }
                    v.clone()
                }
                _ => return Err(invalid("s= takes style words in a string, or a class name.")),
            };
            style = Some(decode_words(&text, classes)?);
        }
    }
    let mut motion_keys = Vec::new();
    for (kw, _) in motion::KEYS {
        if sugar(kw) {
            if let Some(v) = take(pairs, kw) {
                motion_keys.push(motion::sugar_key(kw, &v)?);
            }
        }
    }
    if style.is_none() && motion_keys.is_empty() {
        return Ok(out);
    }
    let style = style.unwrap_or_default();
    let mut entries: Vec<MapEntry> = style
        .iter()
        .map(|(k, v)| MapEntry { key: k.clone(), key_span: span, value: json_to_expr(v, span) })
        .collect();
    // An explicit style= map wins over the words.
    if let Some(explicit) = take(pairs, "style") {
        match explicit.kind {
            ExprKind::Skipped => {}
            ExprKind::Map(more) => {
                entries.retain(|e| !more.iter().any(|m| m.key == e.key));
                entries.extend(more);
            }
            _ => return Err(invalid("style= must be a map literal when s=, enter=, exit=, stagger= or delay= is also given.")),
        }
    }
    for (k, v) in motion_keys {
        entries.retain(|e| e.key != k);
        entries.push(MapEntry { key: k.to_owned(), key_span: span, value: json_to_expr(&v, span) });
    }
    out.push(("style".into(), Expr { kind: ExprKind::Map(entries), span }));
    Ok(out)
}

fn decode_words(words: &str, classes: &HashMap<String, String>) -> Result<Map<String, Value>, CompileError> {
    let mut out = Map::new();
    style::decode_into(words, &mut out, &|w| classes.get(w).cloned(), 0).map_err(|e| {
        invalid(format!("{e}. Style words: col, row, g12, p16, px-md, bg-card, fg-mutedForeground, r-lg, sh-sm, w100%, items-center, justify-between, hover:bg-accent, …"))
    })?;
    Ok(out)
}

/// Desugars a whole program: its statements, and the classes it defines.
pub fn desugar(
    statements: &[Statement],
    ext: &Express,
    cat: &dyn Components,
) -> Result<(Vec<Statement>, HashMap<String, String>), CompileError> {
    let mut d = Desugarer::default();
    let mut out = Vec::with_capacity(statements.len());
    for s in statements {
        if let Desugared::Statement(s) = d.statement(s, ext, cat)? {
            out.push(s);
        }
    }
    Ok((out, d.classes))
}
