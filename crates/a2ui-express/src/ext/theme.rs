//! `theme(...)`: about 14 seeds expanded into an eUI theme.
//!
//! [`expand`] turns the seeds of one mode (the `base`, light unless given)
//! into a full eUI theme JSON (`https://ui.exponential.at/schemas/theme/v1.json`)
//! that `extends` the built-in `neutral` theme, for both modes:
//!
//! - every colour token of the shadcn set, in both modes: the base mode
//!   from the seeds, the other mode derived from them (surfaces mirrored in
//!   OKLab lightness, brand colours kept in hue and chroma);
//! - contrast-checked: every `xForeground` reaches 4.5:1 on its `x`,
//!   `mutedForeground` 4.5:1 on `background` and `muted`, `link` 4.5:1 and
//!   `ring` and the chart hues 3:1 on `background`, by moving the colour's
//!   OKLab lightness; a seed is used as given in its own mode;
//! - `radius`, `scale`, `density`, `stroke`, `font`, `display`, `shadow`
//!   and `button` set token groups, fonts and the Button/Heading recipes.
//!
//! Any full token may still be given: a colour token name (`cardForeground=
//! #…`) overrides the base mode, `light={…}` / `dark={…}` a mode, and
//! `tokens` / `fonts` / `recipes` maps merge over what the seeds produced.
//!
//! [`recover`] is the inverse the decompiler uses: the seeds (and overrides)
//! that expand to a given theme, or `None`. It only returns seeds that
//! [`expand`] maps back to exactly that theme, so a theme decompiles to
//! `theme(...)` and compiles back unchanged.
//!
//! The arithmetic is in [`super::color`], deterministic on every target, so
//! the expansion is pinned as golden JSON (`conformance/extensions/theme.json`).

use serde_json::{Map, Value};

use super::{obj, st};

use super::color::{Rgba, contrast};

/// The schema every theme names.
pub const SCHEMA: &str = "https://ui.exponential.at/schemas/theme/v1.json";

/// The seeds, in the order [`recover`] writes them.
pub const SEEDS: &[&str] = &[
    "base",
    "bg",
    "fg",
    "card",
    "primary",
    "secondary",
    "muted",
    "accent",
    "border",
    "charts",
    "radius",
    "font",
    "display",
    "scale",
    "density",
    "shadow",
    "button",
    "stroke",
];

/// Other spellings of a seed.
pub const ALIASES: &[(&str, &str)] =
    &[("mode", "base"), ("background", "bg"), ("foreground", "fg")];

/// Keys beside the seeds: the theme's identity and the full-token overrides.
pub const EXTRA: &[&str] = &[
    "id", "name", "extends", "light", "dark", "tokens", "fonts", "recipes",
];

/// The colour tokens the expander writes, in order.
pub const COLORS: &[&str] = &[
    "background",
    "foreground",
    "card",
    "cardForeground",
    "popover",
    "popoverForeground",
    "primary",
    "primaryForeground",
    "secondary",
    "secondaryForeground",
    "muted",
    "mutedForeground",
    "accent",
    "accentForeground",
    "destructive",
    "destructiveForeground",
    "success",
    "successForeground",
    "warning",
    "warningForeground",
    "info",
    "infoForeground",
    "border",
    "input",
    "ring",
    "chart1",
    "chart2",
    "chart3",
    "chart4",
    "chart5",
    "chart6",
    "chart7",
    "chart8",
    "link",
];

/// Every colour token a theme may set (eUI's `tokens.json` colour group):
/// the names an override may use.
pub const ALL_COLORS: &[&str] = &[
    "background",
    "foreground",
    "card",
    "cardForeground",
    "popover",
    "popoverForeground",
    "primary",
    "primaryForeground",
    "secondary",
    "secondaryForeground",
    "muted",
    "mutedForeground",
    "accent",
    "accentForeground",
    "destructive",
    "destructiveForeground",
    "success",
    "successForeground",
    "warning",
    "warningForeground",
    "info",
    "infoForeground",
    "border",
    "input",
    "ring",
    "chart1",
    "chart2",
    "chart3",
    "chart4",
    "chart5",
    "chart6",
    "chart7",
    "chart8",
    "link",
    "borderStrong",
    "ghostHover",
    "strokeActive",
    "destructiveHover",
    "bubbleOwn",
    "sheenFrom",
    "sheenTo",
    "match",
    "matchCurrent",
];

const MODES: [&str; 2] = ["light", "dark"];

/// `neutral`'s colours, which the expander starts from.
struct Defaults {
    bg: Rgba,
    fg: Rgba,
    status: [(&'static str, Rgba); 4],
    charts: [Rgba; 8],
}

const fn hex(v: u32) -> Rgba {
    Rgba::rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

fn defaults(mode: &str) -> Defaults {
    if mode == "dark" {
        Defaults {
            bg: hex(0x0a0a0a),
            fg: hex(0xfafafa),
            status: [
                ("destructive", hex(0xff6467)),
                ("success", hex(0x22c55e)),
                ("warning", hex(0xf59e0b)),
                ("info", hex(0x3b82f6)),
            ],
            charts: [
                hex(0x1447e6),
                hex(0x00bc7d),
                hex(0xfe9a00),
                hex(0xad46ff),
                hex(0xff2056),
                hex(0xa78bfa),
                hex(0xf472b6),
                hex(0x22d3ee),
            ],
        }
    } else {
        Defaults {
            bg: hex(0xffffff),
            fg: hex(0x0a0a0a),
            status: [
                ("destructive", hex(0xe7000b)),
                ("success", hex(0x16a34a)),
                ("warning", hex(0xd97706)),
                ("info", hex(0x2563eb)),
            ],
            charts: [
                hex(0xf54900),
                hex(0x009689),
                hex(0x104e64),
                hex(0xffb900),
                hex(0xfe9a00),
                hex(0x7c3aed),
                hex(0xdb2777),
                hex(0x0891b2),
            ],
        }
    }
}

const NEUTRAL_SHADOWS: &str = r##"{"light":{"none":[],"sm":[{"x":0,"y":1,"blur":2,"spread":0,"color":"#0000000d"}],"md":[{"x":0,"y":4,"blur":6,"spread":-1,"color":"#0000001a"},{"x":0,"y":2,"blur":4,"spread":-2,"color":"#0000001a"}],"lg":[{"x":0,"y":10,"blur":15,"spread":-3,"color":"#0000001a"},{"x":0,"y":4,"blur":6,"spread":-4,"color":"#0000001a"}],"xl":[{"x":0,"y":20,"blur":25,"spread":-5,"color":"#0000001a"},{"x":0,"y":8,"blur":10,"spread":-6,"color":"#0000001a"}],"popover":[{"x":0,"y":4,"blur":6,"spread":-1,"color":"#0000001a"},{"x":0,"y":2,"blur":4,"spread":-2,"color":"#0000001a"}]},"dark":{"none":[],"sm":[{"x":0,"y":1,"blur":2,"spread":0,"color":"#00000033"}],"md":[{"x":0,"y":4,"blur":6,"spread":-1,"color":"#00000066"},{"x":0,"y":2,"blur":4,"spread":-2,"color":"#00000066"}],"lg":[{"x":0,"y":10,"blur":15,"spread":-3,"color":"#00000066"},{"x":0,"y":4,"blur":6,"spread":-4,"color":"#00000066"}],"xl":[{"x":0,"y":20,"blur":25,"spread":-5,"color":"#00000080"},{"x":0,"y":8,"blur":10,"spread":-6,"color":"#00000080"}],"popover":[{"x":0,"y":4,"blur":6,"spread":-1,"color":"#00000066"},{"x":0,"y":2,"blur":4,"spread":-2,"color":"#00000066"}]}}"##;

const SPACING: &[(&str, f64)] = &[
    ("xxs", 2.0),
    ("xs", 4.0),
    ("sm", 8.0),
    ("md", 12.0),
    ("lg", 16.0),
    ("xl", 24.0),
    ("xl2", 32.0),
    ("xl3", 48.0),
];
const CONTROLS: &[(&str, f64)] = &[
    ("buttonSm", 32.0),
    ("buttonMd", 36.0),
    ("buttonLg", 40.0),
    ("buttonIcon", 36.0),
    ("input", 36.0),
    ("row", 40.0),
    ("rowCompact", 32.0),
    ("pill", 24.0),
    ("badge", 20.0),
    ("chip", 28.0),
    ("toggle", 36.0),
];
const TYPE_SIZE: &[(&str, f64)] = &[
    ("xs", 12.0),
    ("sm", 14.0),
    ("base", 16.0),
    ("lg", 18.0),
    ("xl", 20.0),
    ("xl2", 24.0),
    ("xl3", 30.0),
];
const LINE_HEIGHT: &[(&str, f64)] = &[
    ("xs", 16.0),
    ("sm", 20.0),
    ("base", 24.0),
    ("lg", 28.0),
    ("xl", 28.0),
    ("xl2", 32.0),
    ("xl3", 36.0),
];
const RADIUS: &[(&str, f64)] = &[
    ("sm", 0.75),
    ("md", 1.0),
    ("lg", 1.25),
    ("xl", 1.75),
    ("xl2", 2.25),
    ("xl3", 2.75),
];
const DENSITY: &[(&str, f64)] = &[("compact", 0.875), ("default", 1.0), ("comfortable", 1.125)];
const SHADOWS: &[&str] = &["none", "soft", "strong"];
const BUTTONS: &[(&str, Option<&str>)] = &[
    ("rounded", None),
    ("pill", Some("$radius.full")),
    ("square", Some("$radius.none")),
];

/// A JSON number, whole values as integers.
fn num(v: f64) -> Value {
    let r = (v * 100.0).round() / 100.0;
    if r.fract() == 0.0 && r.abs() < 1e15 {
        Value::Number((r as i64).into())
    } else {
        serde_json::Number::from_f64(r).map_or(Value::Null, Value::Number)
    }
}

fn font_entry(fallback: &str, weights: &[i64]) -> Value {
    obj([
        ("fallback", st(fallback)),
        (
            "weights",
            Value::Array(weights.iter().map(|w| Value::Number((*w).into())).collect()),
        ),
        ("source", st("host")),
    ])
}

/// `{root: [{style: {key: value}}]}`: one recipe rule.
fn recipe(key: &str, value: &str) -> Value {
    obj([(
        "root",
        Value::Array(vec![obj([("style", obj([(key, st(value))]))])]),
    )])
}

fn round_half_up(v: f64) -> f64 {
    (v + 0.5).floor()
}

// ---- contrast ---------------------------------------------------------------

/// `c` moved along OKLab lightness, away from `against`, until it reaches
/// `min` contrast on every colour of `against` (or the end of the range).
fn ensure(c: Rgba, against: &[Rgba], min: f64) -> Rgba {
    let ok = |x: Rgba| against.iter().all(|a| contrast(x, *a) >= min);
    if ok(c) {
        return c;
    }
    let darker = against.iter().map(|a| a.luminance()).sum::<f64>() / against.len() as f64 > 0.18;
    let l0 = c.lightness();
    let mut best = c;
    for step in 1..=100 {
        let l = if darker {
            l0 - step as f64 * 0.01
        } else {
            l0 + step as f64 * 0.01
        };
        if !(0.0..=1.0).contains(&l) {
            break;
        }
        best = c.with_lightness(l, 1.0);
        if ok(best) {
            return best;
        }
    }
    let end = if darker { Rgba::BLACK } else { Rgba::WHITE };
    if ok(end) { end } else { best }
}

/// The first of `prefer` reaching 4.5:1 on `bg`, else black or white,
/// whichever contrasts more.
fn on(bg: Rgba, prefer: &[Rgba]) -> Rgba {
    for &p in prefer {
        if contrast(p, bg) >= 4.5 {
            return p;
        }
    }
    if contrast(Rgba::WHITE, bg) >= contrast(Rgba::BLACK, bg) {
        Rgba::WHITE
    } else {
        Rgba::BLACK
    }
}

/// A surface colour mirrored into the other mode.
fn flip(c: Rgba) -> Rgba {
    c.with_lightness((1.16 - c.lightness()).clamp(0.1, 0.99), 1.0)
}

fn is_neutral(c: Rgba) -> bool {
    let (_, a, b) = c.oklab();
    (a * a + b * b).sqrt() < 0.03
}

/// A brand colour carried into the other mode: neutrals mirror like
/// surfaces, hues keep their lightness unless it fails 3:1 on the new
/// background.
fn carry(c: Rgba, bg: Rgba) -> Rgba {
    let c = if is_neutral(c) { flip(c) } else { c };
    ensure(c, &[bg], 3.0)
}

// ---- seeds --------------------------------------------------------------------

/// The colours one mode is built from.
#[derive(Debug, Clone, Default)]
struct ModeSeeds {
    bg: Option<Rgba>,
    fg: Option<Rgba>,
    card: Option<Rgba>,
    primary: Option<Rgba>,
    secondary: Option<Rgba>,
    muted: Option<Rgba>,
    accent: Option<Rgba>,
    border: Option<Rgba>,
    charts: Vec<Rgba>,
}

fn mode_colors(mode: &str, s: &ModeSeeds, own: bool) -> Map<String, Value> {
    let d = defaults(mode);
    let dark = mode == "dark";
    let bg = s.bg.unwrap_or(d.bg);
    let fg = match s.fg {
        Some(fg) if own => fg,
        Some(fg) => ensure(fg, &[bg], 7.0),
        None => ensure(on(bg, &[d.fg]), &[bg], 7.0),
    };
    let tint = |t: f64| bg.mix(fg, t);
    let card = s.card.unwrap_or(if dark { tint(0.05) } else { bg });
    let primary = s.primary.unwrap_or_else(|| fg.mix(bg, 0.08));
    let soft = tint(if dark { 0.12 } else { 0.06 });
    let secondary = s.secondary.unwrap_or(soft);
    let muted = s.muted.unwrap_or(soft);
    let accent = s.accent.unwrap_or(soft);
    let border = s.border.unwrap_or(tint(if dark { 0.16 } else { 0.12 }));
    let card_fg = on(card, &[fg]);
    let muted_fg = ensure(fg.mix(bg, 0.45), &[bg, muted.over(bg)], 4.5);

    let mut c = Map::new();
    let mut put = |k: &str, v: Rgba| {
        c.insert(k.to_owned(), Value::String(v.hex()));
    };
    put("background", bg);
    put("foreground", fg);
    put("card", card);
    put("cardForeground", card_fg);
    put("popover", card);
    put("popoverForeground", card_fg);
    put("primary", primary);
    put("primaryForeground", on(primary.over(bg), &[bg, fg]));
    put("secondary", secondary);
    put("secondaryForeground", on(secondary.over(bg), &[fg]));
    put("muted", muted);
    put("mutedForeground", muted_fg);
    put("accent", accent);
    put("accentForeground", on(accent.over(bg), &[fg]));
    for (name, color) in d.status {
        let color = ensure(color, &[bg], 3.0);
        put(name, color);
        put(
            &format!("{name}Foreground"),
            on(color, &[Rgba::WHITE, fg, bg]),
        );
    }
    put("border", border);
    put("input", border);
    put("ring", ensure(primary.over(bg), &[bg], 3.0));
    for i in 0..8 {
        let chart = if s.charts.is_empty() {
            d.charts[i]
        } else {
            s.charts[i % s.charts.len()]
        };
        put(
            &format!("chart{}", i + 1),
            ensure(chart.over(bg), &[bg], 3.0),
        );
    }
    put("link", ensure(primary.over(bg), &[bg], 4.5));
    c
}

/// The other mode's seeds, derived from the base mode's.
fn derive(base: &ModeSeeds, other: &str) -> ModeSeeds {
    let d = defaults(other);
    let bg = base.bg.map(flip).unwrap_or(d.bg);
    let fg = base.fg.map(flip);
    let fg_for = fg.unwrap_or(d.fg);
    let surface = |c: Option<Rgba>| {
        c.filter(|c| Some(*c) != base.bg).map(|c| {
            let flipped = flip(c);
            // Elevation stays visible: a card distinct from its background
            // stays distinct after mirroring.
            if flipped == bg {
                bg.mix(fg_for, 0.05)
            } else {
                flipped
            }
        })
    };
    ModeSeeds {
        bg: Some(bg),
        fg,
        card: surface(base.card),
        primary: base.primary.map(|p| carry(p, bg)),
        secondary: surface(base.secondary),
        muted: surface(base.muted),
        accent: base.accent.map(|a| {
            if is_neutral(a) {
                flip(a)
            } else {
                a.with_lightness(1.0 - a.lightness() * 0.6, 0.8)
            }
        }),
        border: base.border.map(|b| {
            if b.a == 255 {
                flip(b)
            } else {
                Rgba {
                    a: b.a,
                    ..flip(b.opaque())
                }
            }
        }),
        charts: base.charts.iter().map(|c| carry(*c, bg)).collect(),
    }
}

// ---- reading the call -----------------------------------------------------------

fn color_arg(args: &Map<String, Value>, key: &str) -> Result<Option<Rgba>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Rgba::parse(s).map(Some).ok_or_else(|| {
            format!("theme {key}= must be a #rrggbb or #rrggbbaa colour, not \"{s}\".")
        }),
        Some(other) => Err(format!(
            "theme {key}= must be a #rrggbb colour, not {other}."
        )),
    }
}

fn number_arg(args: &Map<String, Value>, key: &str) -> Result<Option<f64>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => Ok(n.as_f64()),
        Some(other) => Err(format!("theme {key}= must be a number, not {other}.")),
    }
}

fn string_arg<'a>(args: &'a Map<String, Value>, key: &str) -> Result<Option<&'a str>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(other) => Err(format!("theme {key}= must be a string, not {other}.")),
    }
}

fn choice<'a, T>(
    args: &Map<String, Value>,
    key: &str,
    table: &'a [(&'a str, T)],
) -> Result<Option<&'a T>, String> {
    match string_arg(args, key)? {
        None => Ok(None),
        Some(s) => table
            .iter()
            .find(|(n, _)| *n == s)
            .map(|(_, v)| Some(v))
            .ok_or_else(|| {
                let names: Vec<&str> = table.iter().map(|(n, _)| *n).collect();
                format!("theme {key}= is one of {}, not \"{s}\".", names.join(", "))
            }),
    }
}

/// The call's keyword arguments with aliases resolved, checked for unknown
/// names.
pub fn normalize_args(raw: &Map<String, Value>) -> Result<Map<String, Value>, String> {
    let mut out = Map::new();
    for (k, v) in raw {
        let key = ALIASES
            .iter()
            .find(|(a, _)| a == k)
            .map_or(k.as_str(), |(_, s)| s);
        let known = SEEDS.contains(&key) || EXTRA.contains(&key) || ALL_COLORS.contains(&key);
        if !known {
            return Err(format!(
                "Unknown theme argument '{k}'. Seeds: {}; or any colour token, light={{…}}, dark={{…}}, tokens={{…}}, fonts={{…}}, recipes={{…}}.",
                SEEDS.join(", ")
            ));
        }
        if out.insert(key.to_owned(), v.clone()).is_some() {
            return Err(format!("theme argument '{key}' is given twice."));
        }
    }
    Ok(out)
}

/// Deep-merges `over` into `into` (maps merge, anything else replaces).
pub fn merge(into: &mut Value, over: &Value) {
    match (into, over) {
        (Value::Object(a), Value::Object(b)) => {
            for (k, v) in b {
                match a.get_mut(k) {
                    Some(slot) if slot.is_object() && v.is_object() => merge(slot, v),
                    _ => {
                        a.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (slot, v) => *slot = v.clone(),
    }
}

fn check_colors(map: &Value, owner: &str) -> Result<(), String> {
    let Some(m) = map.as_object() else {
        return Err(format!("theme {owner}= must be a map of colour tokens."));
    };
    for (k, v) in m {
        if !ALL_COLORS.contains(&k.as_str()) {
            return Err(format!(
                "theme {owner}= names '{k}', which is not a colour token."
            ));
        }
        if !v.as_str().is_some_and(|s| Rgba::parse(s).is_some()) {
            return Err(format!(
                "theme {owner}= {k} must be a #rrggbb or #rrggbbaa colour."
            ));
        }
    }
    Ok(())
}

/// Expands theme arguments (seeds and overrides, aliases allowed) into an
/// eUI theme.
pub fn expand(raw: &Map<String, Value>) -> Result<Value, String> {
    let args = normalize_args(raw)?;
    let base = match string_arg(&args, "base")? {
        None | Some("light") => "light",
        Some("dark") => "dark",
        Some(other) => {
            return Err(format!(
                "theme base= is \"light\" or \"dark\", not \"{other}\"."
            ));
        }
    };
    let other = if base == "light" { "dark" } else { "light" };
    let charts = match args.get("charts") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|c| {
                c.as_str()
                    .and_then(Rgba::parse)
                    .ok_or_else(|| "theme charts= is a list of #rrggbb colours.".to_owned())
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => return Err("theme charts= is a list of #rrggbb colours.".into()),
    };
    if charts.len() > 8 {
        return Err("theme charts= takes at most 8 colours.".into());
    }
    let seeds = ModeSeeds {
        bg: color_arg(&args, "bg")?,
        fg: color_arg(&args, "fg")?,
        card: color_arg(&args, "card")?,
        primary: color_arg(&args, "primary")?,
        secondary: color_arg(&args, "secondary")?,
        muted: color_arg(&args, "muted")?,
        accent: color_arg(&args, "accent")?,
        border: color_arg(&args, "border")?,
        charts,
    };
    let base_colors = mode_colors(base, &seeds, true);
    // The other mode's charts come from the base mode's (contrast-checked)
    // charts, so the seeds read back from the base mode reach both.
    let mut carried = seeds.clone();
    if !carried.charts.is_empty() {
        carried.charts = (1..=8)
            .filter_map(|i| {
                base_colors[&format!("chart{i}")]
                    .as_str()
                    .and_then(Rgba::parse)
            })
            .collect();
    }
    let derived = derive(&carried, other);

    let mut modes = Map::new();
    for mode in MODES {
        let colors = if mode == base {
            base_colors.clone()
        } else {
            mode_colors(mode, &derived, false)
        };
        modes.insert(mode.to_owned(), obj([("color", Value::Object(colors))]));
    }

    match choice(
        &args,
        "shadow",
        &SHADOWS.iter().map(|s| (*s, ())).collect::<Vec<_>>(),
    )? {
        None => {}
        Some(()) => {
            let level = string_arg(&args, "shadow")?.unwrap_or("soft");
            if level != "soft" {
                let neutral: Value =
                    serde_json::from_str(NEUTRAL_SHADOWS).expect("built-in shadows");
                for mode in MODES {
                    let mut shadow = Map::new();
                    for (name, layers) in neutral[mode].as_object().expect("shadow map") {
                        let layers: Vec<Value> = if level == "none" {
                            Vec::new()
                        } else {
                            layers
                                .as_array()
                                .expect("layers")
                                .iter()
                                .map(|l| {
                                    let c = Rgba::parse(l["color"].as_str().unwrap_or("#00000000"))
                                        .unwrap_or(Rgba::BLACK);
                                    let a = ((c.a as u32) * 2).min(255) as u8;
                                    obj([
                                        ("x", l["x"].clone()),
                                        ("y", num(l["y"].as_f64().unwrap_or(0.0) * 1.5)),
                                        ("blur", num(l["blur"].as_f64().unwrap_or(0.0) * 1.5)),
                                        ("spread", l["spread"].clone()),
                                        ("color", Value::String(Rgba { a, ..c }.hex())),
                                    ])
                                })
                                .collect()
                        };
                        shadow.insert(name.clone(), Value::Array(layers));
                    }
                    modes[mode]["shadow"] = Value::Object(shadow);
                }
            }
        }
    }

    let mut tokens = Map::new();
    if let Some(r) = number_arg(&args, "radius")? {
        if r < 0.0 {
            return Err("theme radius= is a number of px, at least 0.".into());
        }
        let radius: Map<String, Value> = RADIUS
            .iter()
            .map(|(k, f)| ((*k).to_owned(), num(r * f)))
            .collect();
        tokens.insert("radius".into(), Value::Object(radius));
    }
    if let Some(s) = number_arg(&args, "scale")? {
        if !(0.5..=2.0).contains(&s) {
            return Err("theme scale= is a type scale between 0.5 and 2.".into());
        }
        if s != 1.0 {
            let scaled = |t: &[(&str, f64)]| -> Value {
                Value::Object(
                    t.iter()
                        .map(|(k, v)| ((*k).to_owned(), num(round_half_up(v * s))))
                        .collect(),
                )
            };
            tokens.insert(
                "type".into(),
                obj([
                    ("size", scaled(TYPE_SIZE)),
                    ("lineHeight", scaled(LINE_HEIGHT)),
                ]),
            );
        }
    }
    if let Some(&m) = choice(&args, "density", DENSITY)? {
        if m != 1.0 {
            let scaled = |t: &[(&str, f64)]| -> Value {
                Value::Object(
                    t.iter()
                        .map(|(k, v)| ((*k).to_owned(), num(round_half_up(v * m))))
                        .collect(),
                )
            };
            tokens.insert("spacing".into(), scaled(SPACING));
            tokens.insert("control".into(), scaled(CONTROLS));
        }
    }
    if let Some(w) = number_arg(&args, "stroke")? {
        if w < 0.0 {
            return Err("theme stroke= is a border width in px, at least 0.".into());
        }
        tokens.insert("border".into(), obj([("hairline", num(w))]));
    }
    let mut fonts = Map::new();
    let mut recipes = Map::new();
    let font = string_arg(&args, "font")?;
    let display = string_arg(&args, "display")?;
    if font.is_some() || display.is_some() {
        let mut family = Map::new();
        if let Some(f) = font {
            family.insert("sans".into(), f.into());
            fonts.insert(
                f.to_owned(),
                font_entry(
                    "ui-sans-serif, system-ui, sans-serif",
                    &[400, 500, 600, 700],
                ),
            );
        }
        if let Some(f) = display {
            family.insert("serif".into(), f.into());
            fonts
                .entry(f.to_owned())
                .or_insert_with(|| font_entry("ui-serif, Georgia, serif", &[400, 600, 700]));
            recipes.insert("Heading".into(), recipe("fontFamily", "$type.family.serif"));
        }
        let ty = tokens
            .entry("type")
            .or_insert_with(|| Value::Object(Map::new()));
        ty["family"] = Value::Object(family);
    }
    if let Some(Some(radius)) = choice(&args, "button", BUTTONS)? {
        recipes.insert("Button".into(), recipe("borderRadius", radius));
    }

    let mut theme = Map::new();
    theme.insert("$schema".into(), SCHEMA.into());
    theme.insert(
        "id".into(),
        string_arg(&args, "id")?.unwrap_or("express").into(),
    );
    theme.insert(
        "name".into(),
        string_arg(&args, "name")?.unwrap_or("Express").into(),
    );
    theme.insert(
        "extends".into(),
        string_arg(&args, "extends")?.unwrap_or("neutral").into(),
    );
    let mut theme = Value::Object(theme);
    theme["modes"] = Value::Object(modes);

    // Full-token overrides: the base mode's, then per mode, then groups.
    let base_over: Map<String, Value> = args
        .iter()
        .filter(|(k, _)| ALL_COLORS.contains(&k.as_str()) && !SEEDS.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    if !base_over.is_empty() {
        let v = Value::Object(base_over);
        check_colors(&v, "colour token")?;
        let lower = lowercase_colors(&v);
        merge(&mut theme["modes"][base]["color"], &lower);
    }
    for mode in MODES {
        if let Some(over) = args.get(mode).filter(|v| !v.is_null()) {
            check_colors(over, mode)?;
            merge(&mut theme["modes"][mode]["color"], &lowercase_colors(over));
        }
    }
    for (group, target) in [
        ("tokens", &mut tokens),
        ("fonts", &mut fonts),
        ("recipes", &mut recipes),
    ] {
        if let Some(over) = args.get(group).filter(|v| !v.is_null()) {
            if !over.is_object() {
                return Err(format!("theme {group}= must be a map."));
            }
            let mut v = Value::Object(std::mem::take(target));
            merge(&mut v, over);
            *target = match v {
                Value::Object(m) => m,
                _ => unreachable!(),
            };
        }
    }
    if !tokens.is_empty() {
        theme["tokens"] = Value::Object(tokens);
    }
    if !fonts.is_empty() {
        theme["fonts"] = Value::Object(fonts);
    }
    if !recipes.is_empty() {
        theme["recipes"] = Value::Object(recipes);
    }
    Ok(theme)
}

fn lowercase_colors(v: &Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        v.as_str()
                            .map_or(v.clone(), |s| Value::String(s.to_ascii_lowercase())),
                    )
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

// ---- recovery ---------------------------------------------------------------------

/// What `expand` adds that `actual` changes or adds: the leaves of `actual`
/// that differ, or `None` when `expanded` has something `actual` lacks
/// (an override cannot remove a key).
fn diff(expanded: &Value, actual: &Value) -> Option<Option<Value>> {
    match (expanded, actual) {
        (Value::Object(e), Value::Object(a)) => {
            if e.keys().any(|k| !a.contains_key(k)) {
                return None;
            }
            let mut out = Map::new();
            for (k, av) in a {
                match e.get(k) {
                    Some(ev) if ev.is_object() && av.is_object() => {
                        if let Some(d) = diff(ev, av)? {
                            out.insert(k.clone(), d);
                        }
                    }
                    Some(ev) if ev == av => {}
                    _ => {
                        out.insert(k.clone(), av.clone());
                    }
                }
            }
            Some((!out.is_empty()).then_some(Value::Object(out)))
        }
        (e, a) if e == a => Some(None),
        (_, a) => Some(Some(a.clone())),
    }
}

/// The shortest period of `charts`.
fn chart_cycle(charts: &[Value]) -> Vec<Value> {
    for p in 1..=charts.len() {
        if (0..charts.len()).all(|i| charts[i] == charts[i % p]) {
            return charts[..p].to_vec();
        }
    }
    charts.to_vec()
}

fn try_base(theme: &Value, base: &str) -> Option<Map<String, Value>> {
    let other = if base == "light" { "dark" } else { "light" };
    let colors = theme
        .pointer(&format!("/modes/{base}/color"))?
        .as_object()?;
    let mut seeds = Map::new();
    if base == "dark" {
        seeds.insert("base".into(), "dark".into());
    }
    for (seed, token) in [
        ("bg", "background"),
        ("fg", "foreground"),
        ("card", "card"),
        ("primary", "primary"),
        ("secondary", "secondary"),
        ("muted", "muted"),
        ("accent", "accent"),
        ("border", "border"),
    ] {
        if let Some(c) = colors
            .get(token)
            .filter(|c| c.as_str().is_some_and(|s| Rgba::parse(s).is_some()))
        {
            seeds.insert(seed.into(), c.clone());
        }
    }
    let charts: Vec<Value> = (1..=8)
        .filter_map(|i| colors.get(&format!("chart{i}")).cloned())
        .collect();
    if charts.len() == 8
        && charts
            .iter()
            .all(|c| c.as_str().is_some_and(|s| Rgba::parse(s).is_some()))
    {
        seeds.insert("charts".into(), Value::Array(chart_cycle(&charts)));
    }
    let tokens = theme.get("tokens");
    if let Some(r) = tokens
        .and_then(|t| t.pointer("/radius/md"))
        .filter(|v| v.is_number())
    {
        seeds.insert("radius".into(), r.clone());
    }
    if let Some(b) = tokens
        .and_then(|t| t.pointer("/type/size/base"))
        .and_then(Value::as_f64)
    {
        seeds.insert("scale".into(), num(b / 16.0));
    }
    if let Some(md) = tokens
        .and_then(|t| t.pointer("/spacing/md"))
        .and_then(Value::as_f64)
    {
        for (name, m) in DENSITY {
            if round_half_up(12.0 * m) == md && *name != "default" {
                seeds.insert("density".into(), (*name).into());
            }
        }
    }
    if let Some(f) = tokens
        .and_then(|t| t.pointer("/type/family/sans"))
        .filter(|v| v.is_string())
    {
        seeds.insert("font".into(), f.clone());
    }
    if let Some(f) = tokens
        .and_then(|t| t.pointer("/type/family/serif"))
        .filter(|v| v.is_string())
    {
        seeds.insert("display".into(), f.clone());
    }
    if let Some(w) = tokens
        .and_then(|t| t.pointer("/border/hairline"))
        .filter(|v| v.is_number())
    {
        seeds.insert("stroke".into(), w.clone());
    }
    if let Some(sm) = theme
        .pointer(&format!("/modes/{base}/shadow/sm"))
        .and_then(Value::as_array)
    {
        seeds.insert(
            "shadow".into(),
            if sm.is_empty() { "none" } else { "strong" }.into(),
        );
    }
    if let Some(r) = theme
        .pointer("/recipes/Button/root/0/style/borderRadius")
        .and_then(Value::as_str)
    {
        if let Some((name, _)) = BUTTONS.iter().find(|(_, v)| *v == Some(r)) {
            seeds.insert("button".into(), (*name).into());
        }
    }
    for (key, default) in [
        ("id", "express"),
        ("name", "Express"),
        ("extends", "neutral"),
    ] {
        match theme.get(key) {
            Some(Value::String(s)) if s == default => {}
            Some(Value::String(s)) => {
                seeds.insert(key.into(), s.clone().into());
            }
            None if key == "extends" => return None,
            _ => return None,
        }
    }
    // Drop each seed the theme does not need, in a fixed order: one whose
    // removal leaves the expansion no further from the theme.
    let distance = |seeds: &Map<String, Value>| expand(seeds).map(|e| leaves_differing(&e, theme));
    let mut current = distance(&seeds).ok()?;
    for seed in [
        "card",
        "secondary",
        "muted",
        "accent",
        "border",
        "charts",
        "fg",
        "display",
        "font",
        "shadow",
        "button",
        "stroke",
        "density",
        "scale",
        "radius",
        "primary",
        "bg",
    ] {
        if let Some(v) = seeds.remove(seed) {
            match distance(&seeds) {
                Ok(d) if d <= current => current = d,
                _ => {
                    seeds.insert(seed.to_owned(), v);
                }
            }
        }
    }
    let expanded = expand(&seeds).ok()?;
    // Overrides for whatever the seeds do not reach.
    let mut extra = Map::new();
    let ecolors = expanded.pointer(&format!("/modes/{base}/color"))?;
    if let Some(Some(Value::Object(d))) = diff(ecolors, &Value::Object(colors.clone())) {
        for (k, v) in d {
            extra.insert(k, v);
        }
    } else if diff(ecolors, &Value::Object(colors.clone())).is_none() {
        return None;
    }
    let ocolors = theme.pointer(&format!("/modes/{other}/color"))?;
    if let Some(d) = diff(expanded.pointer(&format!("/modes/{other}/color"))?, ocolors)? {
        extra.insert(other.into(), d);
    }
    for group in ["tokens", "fonts", "recipes"] {
        let e = expanded
            .get(group)
            .cloned()
            .unwrap_or_else(|| Value::Object(Map::new()));
        let a = theme
            .get(group)
            .cloned()
            .unwrap_or_else(|| Value::Object(Map::new()));
        if let Some(d) = diff(&e, &a)? {
            extra.insert(group.into(), d);
        }
    }
    for (k, v) in extra {
        seeds.insert(k, v);
    }
    let seeds = sorted(seeds);
    (expand(&seeds).ok()? == *theme).then_some(seeds)
}

/// Seeds first (in [`SEEDS`] order), then the identity, then colour tokens
/// and the override maps.
fn sorted(seeds: Map<String, Value>) -> Map<String, Value> {
    let rank = |k: &str| {
        SEEDS
            .iter()
            .position(|s| *s == k)
            .or_else(|| {
                ["id", "name", "extends"]
                    .iter()
                    .position(|s| *s == k)
                    .map(|i| 100 + i)
            })
            .or_else(|| ALL_COLORS.iter().position(|s| *s == k).map(|i| 200 + i))
            .or_else(|| {
                ["light", "dark", "tokens", "fonts", "recipes"]
                    .iter()
                    .position(|s| *s == k)
                    .map(|i| 300 + i)
            })
            .unwrap_or(400)
    };
    let mut entries: Vec<(String, Value)> = seeds.into_iter().collect();
    entries.sort_by_key(|(k, _)| rank(k));
    entries.into_iter().collect()
}

/// How many leaves of `a` and `b` differ (a leaf only one side has counts).
fn leaves_differing(a: &Value, b: &Value) -> usize {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let mut n = 0;
            for (k, v) in x {
                n += match y.get(k) {
                    Some(w) => leaves_differing(v, w),
                    None => leaf_count(v),
                };
            }
            for (k, w) in y {
                if !x.contains_key(k) {
                    n += leaf_count(w);
                }
            }
            n
        }
        (x, y) => usize::from(x != y),
    }
}

fn leaf_count(v: &Value) -> usize {
    match v {
        Value::Object(m) => m.values().map(leaf_count).sum(),
        _ => 1,
    }
}

/// The theme arguments that [`expand`] maps to exactly `theme`, shortest
/// first, or `None` when no seeds reach it (the decompiler then writes the
/// theme JSON as it is).
pub fn recover(theme: &Value) -> Option<Map<String, Value>> {
    let obj = theme.as_object()?;
    let allowed = [
        "$schema", "id", "name", "extends", "modes", "tokens", "fonts", "recipes",
    ];
    if obj.get("$schema").and_then(Value::as_str) != Some(SCHEMA)
        || obj.keys().any(|k| !allowed.contains(&k.as_str()))
    {
        return None;
    }
    let mut best: Option<Map<String, Value>> = None;
    for base in MODES {
        if let Some(seeds) = try_base(theme, base) {
            let len = Value::Object(seeds.clone()).to_string().len();
            if best
                .as_ref()
                .is_none_or(|b| len < Value::Object(b.clone()).to_string().len())
            {
                best = Some(seeds);
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn args(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    fn c(theme: &Value, mode: &str, k: &str) -> Rgba {
        Rgba::parse(theme["modes"][mode]["color"][k].as_str().unwrap()).unwrap()
    }

    #[test]
    fn the_brief_theme_expands_and_is_contrast_checked() {
        let t = expand(&args(json!({"base": "dark", "primary": "#eef800", "bg": "#0e0e0e", "card": "#161616",
            "radius": 0, "font": "Inter", "scale": 1, "density": "compact", "shadow": "none", "button": "pill"})))
        .unwrap();
        assert_eq!(t["modes"]["dark"]["color"]["primary"], "#eef800");
        assert_eq!(t["modes"]["dark"]["color"]["background"], "#0e0e0e");
        for mode in MODES {
            for (x, fg) in [
                ("primary", "primaryForeground"),
                ("card", "cardForeground"),
                ("muted", "mutedForeground"),
                ("background", "foreground"),
                ("accent", "accentForeground"),
                ("destructive", "destructiveForeground"),
            ] {
                let r = contrast(c(&t, mode, x), c(&t, mode, fg));
                assert!(r >= 4.5, "{mode} {x}/{fg}: {r}");
            }
            assert!(contrast(c(&t, mode, "link"), c(&t, mode, "background")) >= 4.5);
        }
        // Yellow on white fails 3:1, so light mode darkens it.
        assert_ne!(t["modes"]["light"]["color"]["primary"], "#eef800");
        assert_eq!(t["tokens"]["radius"]["md"], 0);
        assert_eq!(
            t["recipes"]["Button"]["root"][0]["style"]["borderRadius"],
            "$radius.full"
        );
        assert_eq!(t["modes"]["dark"]["shadow"]["sm"], json!([]));
    }

    #[test]
    fn recover_inverts_expand() {
        for a in [
            json!({}),
            json!({"primary": "#7c3aed"}),
            json!({"base": "dark", "primary": "#eef800", "bg": "#0e0e0e", "card": "#161616", "radius": 0,
                   "font": "Inter", "density": "compact", "shadow": "none", "button": "pill"}),
            json!({"primary": "#0f766e", "charts": ["#0f766e", "#f59e0b"], "radius": 12, "scale": 1.125,
                   "display": "Fraunces", "stroke": 2, "cardForeground": "#111111", "dark": {"ring": "#ff00ff"}}),
        ] {
            let t = expand(&args(a.clone())).unwrap();
            let seeds = recover(&t).unwrap_or_else(|| panic!("{a}"));
            assert_eq!(
                expand(&seeds).unwrap(),
                t,
                "{a} -> {}",
                Value::Object(seeds.clone())
            );
        }
    }

    #[test]
    fn unknown_arguments_are_refused() {
        assert!(
            expand(&args(json!({"primry": "#000000"})))
                .unwrap_err()
                .contains("Unknown theme argument")
        );
        assert!(expand(&args(json!({"bg": "red"}))).is_err());
    }
}
