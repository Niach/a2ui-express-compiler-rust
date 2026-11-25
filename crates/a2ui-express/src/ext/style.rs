//! The style shorthand: `s="col g12 p16 bg-card r-lg sh-sm"`.
//!
//! One word per style key over eUI's `Box` style whitelist
//! (`packages/exponential-ui/catalog/style.json`), expanded at compile time
//! into the plain `style` map; the wire never sees a shorthand.
//!
//! A word is one of:
//!
//! - a **bare word** from [`BARE`]: `col` is `flexDirection: "column"`,
//!   `grow` is `flexGrow: 1`;
//! - **prefix + value**, the prefix being the word's leading ASCII letters,
//!   looked up in [`KEYS`]:
//!   - a number: `p16` (`16`), `op0.5`, `mt-8` (`-8`); with a unit it is
//!     the string the whitelist takes: `w50%`, `p16px`;
//!   - `-auto`: the string `"auto"` (lengths only);
//!   - `-<name>`: the token `$<group>.<name>` in the key's default group
//!     (`bg-card` = `$color.card`, `r-lg` = `$radius.lg`), or
//!     `-<group>.<name>` for another group (`h-control.input`);
//!   - `-<value>`: an enum value, or its short form (`justify-between`);
//! - a state prefix `hover:`, `focus:` or `pressed:` before any of the
//!   above: the nested `:hover` / `:focus-visible` / `:pressed` map;
//! - the name of a class defined with `style(name, "…")`, which expands to
//!   its words.
//!
//! Later words win. [`encode`] writes a style map back: every entry it can
//! spell becomes a word, in [`KEYS`] order, and the rest stays a map. It only
//! writes a word that [`decode`] reads back as exactly that entry, so
//! `compile(decompile(m)) == m` holds by construction, and with one canonical
//! spelling per entry `decompile(compile(x))` is a fixed point.

use serde_json::{Map, Number, Value};

/// How a key's value is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A length: a number, `N%`, `Npx`, `auto`, or a token.
    Length,
    /// A plain number, or a token.
    Number,
    /// A token only (colours: `$color.*`).
    Token,
    /// One of the listed values.
    Enum(&'static [EnumValue]),
}

/// One value of an enum key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnumValue {
    /// The JSON value.
    pub value: &'static str,
    /// The word after `<prefix>-`, or the whole word when `bare`.
    pub short: &'static str,
    /// Written without the prefix.
    pub bare: bool,
}

const fn v(value: &'static str, short: &'static str) -> EnumValue {
    EnumValue {
        value,
        short,
        bare: false,
    }
}

const fn b(value: &'static str, short: &'static str) -> EnumValue {
    EnumValue {
        value,
        short,
        bare: true,
    }
}

/// One style key of the shorthand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    /// The style key.
    pub key: &'static str,
    /// The word prefix (ASCII letters).
    pub prefix: &'static str,
    /// What the value is.
    pub kind: Kind,
    /// The token group a `-name` value reads from; `None` = a token needs
    /// its group written out.
    pub group: Option<&'static str>,
}

const fn k(key: &'static str, prefix: &'static str, kind: Kind, group: Option<&'static str>) -> Key {
    Key {
        key,
        prefix,
        kind,
        group,
    }
}

const SP: Option<&str> = Some("spacing");
const JUSTIFY: &[EnumValue] = &[
    v("flex-start", "flex-start"),
    v("flex-end", "flex-end"),
    v("start", "start"),
    v("end", "end"),
    v("center", "center"),
    v("stretch", "stretch"),
    v("space-between", "between"),
    v("space-around", "around"),
    v("space-evenly", "evenly"),
];
const ALIGN: &[EnumValue] = &[
    v("flex-start", "flex-start"),
    v("flex-end", "flex-end"),
    v("start", "start"),
    v("end", "end"),
    v("center", "center"),
    v("stretch", "stretch"),
    v("baseline", "baseline"),
];
const OVERFLOW: &[EnumValue] = &[
    v("visible", "visible"),
    v("hidden", "hidden"),
    v("clip", "clip"),
    v("scroll", "scroll"),
    v("auto", "auto"),
];

/// The keys, in the order [`encode`] writes them: layout, then paint.
pub const KEYS: &[Key] = &[
    k(
        "display",
        "d",
        Kind::Enum(&[b("flex", "flex"), b("grid", "grid"), b("block", "block"), v("none", "none")]),
        None,
    ),
    k(
        "flexDirection",
        "fd",
        Kind::Enum(&[
            b("row", "row"),
            b("column", "col"),
            v("row-reverse", "row-reverse"),
            v("column-reverse", "col-reverse"),
        ]),
        None,
    ),
    k(
        "flexWrap",
        "flexwrap",
        Kind::Enum(&[b("wrap", "wrap"), b("nowrap", "nowrap"), b("wrap-reverse", "wrap-reverse")]),
        None,
    ),
    k("justifyContent", "justify", Kind::Enum(JUSTIFY), None),
    k("alignItems", "items", Kind::Enum(ALIGN), None),
    k("alignContent", "content", Kind::Enum(JUSTIFY), None),
    k("alignSelf", "self", Kind::Enum(ALIGN), None),
    k("justifySelf", "jself", Kind::Enum(ALIGN), None),
    k("flexGrow", "grow", Kind::Number, None),
    k("flexShrink", "shrink", Kind::Number, None),
    k("flexBasis", "basis", Kind::Length, SP),
    k("gap", "g", Kind::Length, SP),
    k("rowGap", "gy", Kind::Length, SP),
    k("columnGap", "gx", Kind::Length, SP),
    k("direction", "dir", Kind::Enum(&[v("ltr", "ltr"), v("rtl", "rtl")]), None),
    k("overflow", "overflow", Kind::Enum(OVERFLOW), None),
    k("overflowX", "overflowx", Kind::Enum(OVERFLOW), None),
    k("overflowY", "overflowy", Kind::Enum(OVERFLOW), None),
    k("width", "w", Kind::Length, SP),
    k("height", "h", Kind::Length, SP),
    k("minWidth", "minw", Kind::Length, SP),
    k("minHeight", "minh", Kind::Length, SP),
    k("maxWidth", "maxw", Kind::Length, SP),
    k("maxHeight", "maxh", Kind::Length, SP),
    k("aspectRatio", "aspect", Kind::Number, None),
    k(
        "position",
        "pos",
        Kind::Enum(&[b("relative", "relative"), b("absolute", "absolute"), b("sticky", "sticky")]),
        None,
    ),
    k("top", "top", Kind::Length, SP),
    k("right", "right", Kind::Length, SP),
    k("bottom", "bottom", Kind::Length, SP),
    k("left", "left", Kind::Length, SP),
    k("inset", "inset", Kind::Length, SP),
    k("insetInlineStart", "insets", Kind::Length, SP),
    k("insetInlineEnd", "insete", Kind::Length, SP),
    k("insetBlockStart", "insett", Kind::Length, SP),
    k("insetBlockEnd", "insetb", Kind::Length, SP),
    k("padding", "p", Kind::Length, SP),
    k("paddingTop", "pt", Kind::Length, SP),
    k("paddingRight", "pr", Kind::Length, SP),
    k("paddingBottom", "pb", Kind::Length, SP),
    k("paddingLeft", "pl", Kind::Length, SP),
    k("paddingHorizontal", "px", Kind::Length, SP),
    k("paddingVertical", "py", Kind::Length, SP),
    k("paddingInlineStart", "ps", Kind::Length, SP),
    k("paddingInlineEnd", "pe", Kind::Length, SP),
    k("margin", "m", Kind::Length, SP),
    k("marginTop", "mt", Kind::Length, SP),
    k("marginRight", "mr", Kind::Length, SP),
    k("marginBottom", "mb", Kind::Length, SP),
    k("marginLeft", "ml", Kind::Length, SP),
    k("marginHorizontal", "mx", Kind::Length, SP),
    k("marginVertical", "my", Kind::Length, SP),
    k("marginInlineStart", "ms", Kind::Length, SP),
    k("marginInlineEnd", "me", Kind::Length, SP),
    k("backgroundColor", "bg", Kind::Token, Some("color")),
    k("color", "fg", Kind::Token, Some("color")),
    k("borderWidth", "b", Kind::Number, Some("border")),
    k("borderTopWidth", "bt", Kind::Number, Some("border")),
    k("borderRightWidth", "br", Kind::Number, Some("border")),
    k("borderBottomWidth", "bb", Kind::Number, Some("border")),
    k("borderLeftWidth", "bl", Kind::Number, Some("border")),
    k("borderColor", "bc", Kind::Token, Some("color")),
    k(
        "borderStyle",
        "border",
        Kind::Enum(&[b("solid", "solid"), b("dashed", "dashed"), b("dotted", "dotted")]),
        None,
    ),
    k("borderRadius", "r", Kind::Length, Some("radius")),
    k("borderTopLeftRadius", "rtl", Kind::Length, Some("radius")),
    k("borderTopRightRadius", "rtr", Kind::Length, Some("radius")),
    k("borderBottomRightRadius", "rbr", Kind::Length, Some("radius")),
    k("borderBottomLeftRadius", "rbl", Kind::Length, Some("radius")),
    k("opacity", "op", Kind::Number, Some("opacity")),
    k("boxShadow", "sh", Kind::Token, Some("shadow")),
    k("fontSize", "fs", Kind::Length, Some("type.size")),
    k("fontWeight", "fw", Kind::Number, Some("type.weight")),
    k("lineHeight", "lh", Kind::Length, Some("type.lineHeight")),
    k("letterSpacing", "ls", Kind::Length, None),
    k(
        "textAlign",
        "text",
        Kind::Enum(&[
            v("left", "left"),
            v("right", "right"),
            v("center", "center"),
            v("start", "start"),
            v("end", "end"),
            v("justify", "justify"),
        ]),
        None,
    ),
    k(
        "textDecoration",
        "decoration",
        Kind::Enum(&[
            b("none", "no-underline"),
            b("underline", "underline"),
            b("line-through", "line-through"),
        ]),
        None,
    ),
    k(
        "textTransform",
        "case",
        Kind::Enum(&[
            b("none", "normal-case"),
            b("uppercase", "uppercase"),
            b("lowercase", "lowercase"),
            b("capitalize", "capitalize"),
        ]),
        None,
    ),
    k(
        "fontStyle",
        "fontstyle",
        Kind::Enum(&[b("normal", "not-italic"), b("italic", "italic")]),
        None,
    ),
    k(
        "fontVariantNumeric",
        "numeric",
        Kind::Enum(&[b("tabular-nums", "tabular-nums")]),
        None,
    ),
    k("fontFamily", "ff", Kind::Token, Some("type.family")),
    k("transition", "tr", Kind::Token, Some("motion")),
    k("transitionEasing", "ease", Kind::Token, Some("ease")),
    k(
        "visibility",
        "visibility",
        Kind::Enum(&[b("visible", "visible"), b("hidden", "invisible")]),
        None,
    ),
    k(
        "pointerEvents",
        "events",
        Kind::Enum(&[v("auto", "auto"), v("none", "none")]),
        None,
    ),
    k(
        "userSelect",
        "select",
        Kind::Enum(&[v("auto", "auto"), v("none", "none"), v("text", "text")]),
        None,
    ),
    k(
        "cursor",
        "cursor",
        Kind::Enum(&[
            v("auto", "auto"),
            v("default", "default"),
            v("pointer", "pointer"),
            v("text", "text"),
            v("not-allowed", "not-allowed"),
            v("grab", "grab"),
            v("grabbing", "grabbing"),
            v("move", "move"),
            v("col-resize", "col-resize"),
            v("row-resize", "row-resize"),
        ]),
        None,
    ),
    k("backdropBlur", "blur", Kind::Token, Some("blur")),
    k(
        "tail",
        "tail",
        Kind::Enum(&[v("none", "none"), v("start", "start"), v("end", "end"), v("down", "down")]),
        None,
    ),
    k("animationDuration", "dur", Kind::Token, Some("motion")),
];

/// Bare words that stand for a number: `grow` = `flexGrow: 1`.
pub const BARE_NUMBERS: &[(&str, &str, i64)] = &[("grow", "flexGrow", 1), ("shrink", "flexShrink", 1)];

/// The state prefixes and the nested map each writes into.
pub const STATES: &[(&str, &str)] = &[
    ("hover", ":hover"),
    ("focus", ":focus-visible"),
    ("pressed", ":pressed"),
];

/// The shorthand key for a style key.
pub fn key(name: &str) -> Option<&'static Key> {
    KEYS.iter().find(|k| k.key == name)
}

fn by_prefix(prefix: &str) -> Option<&'static Key> {
    KEYS.iter().find(|k| k.prefix == prefix)
}

/// The bare words, from the enum keys and [`BARE_NUMBERS`]: word → (key,
/// value).
pub fn bare_words() -> impl Iterator<Item = (&'static str, &'static str, Value)> {
    let enums = KEYS.iter().flat_map(|k| match k.kind {
        Kind::Enum(values) => values
            .iter()
            .filter(|v| v.bare)
            .map(|v| (v.short, k.key, Value::String(v.value.to_owned())))
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    });
    enums.chain(
        BARE_NUMBERS
            .iter()
            .map(|(w, key, n)| (*w, *key, Value::Number((*n).into()))),
    )
}

fn bare(word: &str) -> Option<(&'static str, Value)> {
    bare_words().find(|(w, _, _)| *w == word).map(|(_, k, v)| (k, v))
}

/// Whether `word` reads as a shorthand word on its own (so it cannot name a
/// class).
pub fn is_word(word: &str) -> bool {
    decode_word(word).is_ok()
}

fn is_number_text(s: &str) -> bool {
    let s = s.strip_prefix('-').unwrap_or(s);
    let (int, frac) = match s.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (s, None),
    };
    !int.is_empty()
        && int.bytes().all(|b| b.is_ascii_digit())
        && frac.is_none_or(|f| !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit()))
}

fn number(text: &str) -> Option<Value> {
    if !is_number_text(text) {
        return None;
    }
    match crate::syntax::parser::number_value(text)? {
        crate::syntax::NumberValue::Int(i) => Some(Value::Number(i.into())),
        crate::syntax::NumberValue::Float(f) => Number::from_f64(f).map(Value::Number),
    }
}

fn is_token_name(s: &str) -> bool {
    !s.is_empty()
        && s.split('.').all(|part| {
            part.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                && part.chars().all(|c| c.is_ascii_alphanumeric())
        })
}

/// Reads one word (without a state prefix) as `(key, value)`.
pub fn decode_word(word: &str) -> Result<(&'static str, Value), String> {
    if let Some((key, value)) = bare(word) {
        return Ok((key, value));
    }
    let split = word
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(word.len());
    let (prefix, rest) = word.split_at(split);
    let Some(key) = by_prefix(prefix) else {
        return Err(format!("'{word}' is not a style shorthand word"));
    };
    let bad = || format!("'{word}' is not a value of {} ({})", key.key, key.prefix);
    match key.kind {
        Kind::Enum(values) => {
            let short = rest.strip_prefix('-').ok_or_else(bad)?;
            values
                .iter()
                .find(|v| v.short == short || v.value == short)
                .map(|v| (key.key, Value::String(v.value.to_owned())))
                .ok_or_else(bad)
        }
        Kind::Length | Kind::Number | Kind::Token => {
            if key.kind != Kind::Token {
                if let Some(n) = number(rest) {
                    return Ok((key.key, n));
                }
            }
            if key.kind == Kind::Length {
                for unit in ["%", "px"] {
                    if let Some(n) = rest.strip_suffix(unit) {
                        if is_number_text(n) {
                            return Ok((key.key, Value::String(rest.to_owned())));
                        }
                    }
                }
                if rest == "-auto" {
                    return Ok((key.key, Value::String("auto".into())));
                }
            }
            let name = rest.strip_prefix('-').filter(|n| is_token_name(n)).ok_or_else(bad)?;
            let token = match (name.contains('.'), key.group) {
                (true, _) => format!("${name}"),
                (false, Some(group)) => format!("${group}.{name}"),
                (false, None) => return Err(bad()),
            };
            Ok((key.key, Value::String(token)))
        }
    }
}

/// Reads a shorthand string into `out`; `class` resolves a class name to
/// its words. Later words win.
pub fn decode_into(
    text: &str,
    out: &mut Map<String, Value>,
    class: &dyn Fn(&str) -> Option<String>,
    depth: usize,
) -> Result<(), String> {
    if depth > 8 {
        return Err("style classes nest too deep".into());
    }
    for word in text.split_whitespace() {
        if let Some(words) = class(word) {
            decode_into(&words, out, class, depth + 1)?;
            continue;
        }
        let (state, word) = match word.split_once(':') {
            Some((s, rest)) => match STATES.iter().find(|(name, _)| *name == s) {
                Some((_, key)) => (Some(*key), rest),
                None => return Err(format!("'{s}:' is not a style state (hover:, focus:, pressed:)")),
            },
            None => (None, word),
        };
        let (key, value) = decode_word(word)?;
        match state {
            None => {
                out.insert(key.to_owned(), value);
            }
            Some(state) => {
                let nested = out
                    .entry(state.to_owned())
                    .or_insert_with(|| Value::Object(Map::new()));
                if !nested.is_object() {
                    *nested = Value::Object(Map::new());
                }
                nested
                    .as_object_mut()
                    .expect("object")
                    .insert(key.to_owned(), value);
            }
        }
    }
    Ok(())
}

/// The canonical word for one entry, if the shorthand can spell it.
pub fn encode_entry(name: &str, value: &Value) -> Option<String> {
    let key = key(name)?;
    let mut candidates: Vec<String> = Vec::new();
    for (w, k, v) in bare_words() {
        if k == name && v == *value {
            candidates.push(w.to_owned());
        }
    }
    match (key.kind, value) {
        (Kind::Enum(values), Value::String(s)) => {
            if let Some(v) = values.iter().find(|v| v.value == s) {
                candidates.push(if v.bare {
                    v.short.to_owned()
                } else {
                    format!("{}-{}", key.prefix, v.short)
                });
            }
        }
        (Kind::Length | Kind::Number, Value::Number(n)) => {
            candidates.push(format!("{}{}", key.prefix, n));
        }
        (_, Value::String(s)) => {
            if let Some(token) = s.strip_prefix('$') {
                if let Some(group) = key.group {
                    if let Some(name) = token.strip_prefix(group).and_then(|r| r.strip_prefix('.')) {
                        candidates.push(format!("{}-{name}", key.prefix));
                    }
                }
                candidates.push(format!("{}-{token}", key.prefix));
            } else if s == "auto" {
                candidates.push(format!("{}-auto", key.prefix));
            } else {
                candidates.push(format!("{}{s}", key.prefix));
            }
        }
        _ => {}
    }
    candidates
        .into_iter()
        .find(|w| !w.contains(char::is_whitespace) && decode_word(w).is_ok_and(|(k, v)| k == name && v == *value))
}

/// A style map split into shorthand words and what the shorthand cannot
/// spell. `skip` keys are left out of both (the motion keywords).
pub fn encode(style: &Map<String, Value>, skip: &[&str]) -> (Vec<String>, Map<String, Value>) {
    let mut words: Vec<(usize, usize, String)> = Vec::new();
    let mut rest = Map::new();
    for (name, value) in style {
        if skip.contains(&name.as_str()) {
            continue;
        }
        if let Some(state) = STATES.iter().position(|(_, key)| key == name) {
            let encoded = value.as_object().and_then(|m| {
                if m.is_empty() {
                    return None;
                }
                m.iter()
                    .map(|(k, v)| {
                        let order = KEYS.iter().position(|x| x.key == k)?;
                        encode_entry(k, v).map(|w| (order, w))
                    })
                    .collect::<Option<Vec<_>>>()
            });
            match encoded {
                Some(ws) => {
                    for (order, w) in ws {
                        words.push((1 + state, order, format!("{}:{w}", STATES[state].0)));
                    }
                }
                None => {
                    rest.insert(name.clone(), value.clone());
                }
            }
            continue;
        }
        match (KEYS.iter().position(|k| k.key == name), encode_entry(name, value)) {
            (Some(order), Some(w)) => words.push((0, order, w)),
            _ => {
                rest.insert(name.clone(), value.clone());
            }
        }
    }
    words.sort();
    (words.into_iter().map(|(_, _, w)| w).collect(), rest)
}

/// The shorthand table as JSON, for the grammar generators
/// (`a2ui-gen-eui`), which build their style-word rules from it.
pub fn table_json() -> Value {
    let keys: Vec<Value> = KEYS
        .iter()
        .map(|k| {
            let (kind, values) = match k.kind {
                Kind::Length => ("length", Value::Null),
                Kind::Number => ("number", Value::Null),
                Kind::Token => ("token", Value::Null),
                Kind::Enum(vs) => (
                    "enum",
                    Value::Array(
                        vs.iter()
                            .map(|v| {
                                serde_json::json!({"value": v.value, "word": v.short, "bare": v.bare})
                            })
                            .collect(),
                    ),
                ),
            };
            let mut o = Map::new();
            o.insert("key".into(), k.key.into());
            o.insert("prefix".into(), k.prefix.into());
            o.insert("kind".into(), kind.into());
            if let Some(g) = k.group {
                o.insert("group".into(), g.into());
            }
            if !values.is_null() {
                o.insert("values".into(), values);
            }
            Value::Object(o)
        })
        .collect();
    serde_json::json!({
        "keys": keys,
        "bareNumbers": BARE_NUMBERS.iter().map(|(w, k, n)| serde_json::json!({"word": w, "key": k, "value": n})).collect::<Vec<_>>(),
        "states": STATES.iter().map(|(p, k)| serde_json::json!({"prefix": p, "key": k})).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn read(text: &str) -> Value {
        let mut m = Map::new();
        decode_into(text, &mut m, &|_| None, 0).unwrap();
        Value::Object(m)
    }

    #[test]
    fn reads_the_brief_example() {
        assert_eq!(
            read("col g12 p16 bg-card r-lg sh-sm"),
            json!({"flexDirection": "column", "gap": 12, "padding": 16,
                   "backgroundColor": "$color.card", "borderRadius": "$radius.lg",
                   "boxShadow": "$shadow.sm"})
        );
    }

    #[test]
    fn reads_units_tokens_states_and_negatives() {
        assert_eq!(
            read("w50% p16px mt-8 h-control.input maxw-auto op0.5 justify-between hover:bg-accent fw600 fs-sm grow"),
            json!({"width": "50%", "padding": "16px", "marginTop": -8,
                   "height": "$control.input", "maxWidth": "auto", "opacity": 0.5,
                   "justifyContent": "space-between", ":hover": {"backgroundColor": "$color.accent"},
                   "fontWeight": 600, "fontSize": "$type.size.sm", "flexGrow": 1})
        );
    }

    #[test]
    fn prefixes_and_bare_words_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for k in KEYS {
            assert!(seen.insert(k.prefix), "prefix {} twice", k.prefix);
            assert!(k.prefix.bytes().all(|b| b.is_ascii_lowercase()));
        }
        let mut words = std::collections::HashSet::new();
        for (w, _, _) in bare_words() {
            assert!(words.insert(w), "bare word {w} twice");
        }
    }

    #[test]
    fn every_enum_value_round_trips() {
        for k in KEYS {
            if let Kind::Enum(values) = k.kind {
                for v in values {
                    let value = Value::String(v.value.into());
                    let w = encode_entry(k.key, &value).unwrap_or_else(|| panic!("{} {}", k.key, v.value));
                    assert_eq!(decode_word(&w).unwrap(), (k.key, value));
                }
            }
        }
    }

    #[test]
    fn encode_keeps_what_it_cannot_spell() {
        let style = json!({"padding": 16, "backgroundColor": "#ff0000", "gap": 16.0,
                           "transform": "scale(2)", ":hover": {"opacity": 0.5},
                           ":pressed": {"transform": "scale(0.9)"}, "animation": "fade-in"});
        let (words, rest) = encode(style.as_object().unwrap(), &["animation"]);
        assert_eq!(words, ["g16.0", "p16", "hover:op0.5"]);
        assert_eq!(
            Value::Object(rest),
            json!({"backgroundColor": "#ff0000", "transform": "scale(2)", ":pressed": {"transform": "scale(0.9)"}})
        );
    }
}
