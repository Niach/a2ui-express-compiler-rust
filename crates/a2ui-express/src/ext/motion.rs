//! Motion sugar.
//!
//! - `enter="slide-in-up"`, `exit="fade-out"`, `stagger=60`, `delay=120`
//!   on any component: the eUI style keys `animation`, `exitAnimation`,
//!   `animationStagger` and `animationDelay` (names agreed with the eUI
//!   catalogue batch for genUI round 4, 2026-11-26).
//! - `Motion(ref="pulse-dot")`: the props of the catalog's `motions`
//!   registry entry (`metadata.extensions.at_exponential_express.motions`);
//!   a prop written on the call wins.
//! - `Motion(kf="…")`: a compact keyframe form compiled into the eui-motion
//!   dialect's `layers` (`apps/desktop/crates/eui-motion`,
//!   `packages/exponential-ui/eval/motion/guide.md`).
//!
//! # The keyframe form
//!
//! Layers are separated by `;`, a layer's parts by `|`. The first part is a
//! shape, the rest are tracks:
//!
//! ```text
//! circle CX CY R COLOR            a filled circle
//! ring CX CY R WIDTH COLOR        a stroked circle
//! rect CX CY W H RADIUS COLOR     a filled rectangle (centre, size)
//! star CX CY POINTS OUTER INNER COLOR
//!
//! scale T:V T:V …  [EASE]          percent, about the shape's centre
//! opacity T:V …    [EASE]          0–100
//! rotate T:V …     [EASE]          degrees, clockwise
//! move T:DX,DY …   [EASE]          px offset from the shape's centre
//! ```
//!
//! `T` is in ms, `COLOR` a `$color.<token>` or `#rrggbb[aa]`, `EASE` one of
//! `linear` (the default), `ease`, `ease-in`, `ease-out`, `ease-in-out`,
//! `hold`, applied to every segment. For example a pulsing dot:
//! `Motion(kf="circle 12 12 6 $color.primary | scale 0:100 600:140 1200:100 ease-in-out", size=[24, 24], duration=1200)`.
//!
//! [`kf_layers`] compiles it; [`kf_text`] writes layers back when they are
//! exactly what some keyframe text compiles to (checked by compiling the
//! text it writes), so the decompiler's `kf=` is a fixed point.

use serde_json::{Map, Value, json};

use super::{Express, invalid, json_to_expr, literal_json};
use crate::compile::CompileError;
use crate::syntax::{Arg, Call, Expr, ExprKind, Span};

/// The enter animations (`style.animation`).
pub const ENTER: &[&str] = &[
    "pulse",
    "spin",
    "fade-in",
    "slide-in-up",
    "slide-in-down",
    "slide-in-left",
    "slide-in-right",
    "shimmer",
];
/// The exit animations (`style.exitAnimation`).
pub const EXIT: &[&str] = &[
    "fade-out",
    "slide-out-up",
    "slide-out-down",
    "slide-out-left",
    "slide-out-right",
];

/// The component keyword → style key pairs.
pub const KEYS: &[(&str, &str)] = &[
    ("enter", "animation"),
    ("exit", "exitAnimation"),
    ("stagger", "animationStagger"),
    ("delay", "animationDelay"),
];

/// The style key a motion keyword sets, and its value.
pub fn sugar_key(keyword: &str, value: &Expr) -> Result<(&'static str, Value), CompileError> {
    let key = KEYS.iter().find(|(k, _)| *k == keyword).map(|(_, s)| *s).expect("a motion keyword");
    match keyword {
        "enter" | "exit" => {
            let names = if keyword == "enter" { ENTER } else { EXIT };
            match &value.kind {
                ExprKind::String(s) if names.contains(&s.as_str()) => Ok((key, Value::String(s.clone()))),
                _ => Err(invalid(format!("{keyword}= is one of {}.", names.join(", ")))),
            }
        }
        _ => match &value.kind {
            ExprKind::Number(n) if n.to_json().as_f64().is_some_and(|v| v >= 0.0) => {
                Ok((key, Value::Number(n.to_json())))
            }
            _ => Err(invalid(format!("{keyword}= is a number of ms, at least 0."))),
        },
    }
}

/// Expands `ref=` and `kf=` on a Motion call.
pub fn expand_motion(
    call: &mut Call,
    span: Span,
    ext: &Express,
    sugar: &dyn Fn(&str) -> bool,
) -> Result<(), CompileError> {
    let has = |call: &Call, k: &str| call.args.iter().any(|a| a.name.as_ref().is_some_and(|(n, _)| n == k));
    if sugar("ref") {
        if let Some(i) = call.args.iter().position(|a| a.name.as_ref().is_some_and(|(n, _)| n == "ref")) {
            let arg = call.args.remove(i);
            let ExprKind::String(name) = &arg.value.kind else {
                return Err(invalid("Motion ref= is the name of a registered animation, a string."));
            };
            let Some(Value::Object(props)) = ext.motions.get(name) else {
                let mut names: Vec<&str> = ext.motions.keys().map(String::as_str).collect();
                names.sort_unstable();
                return Err(invalid(format!(
                    "Motion ref=\"{name}\" is not a registered animation. Registered: {}.",
                    if names.is_empty() { "none".to_owned() } else { names.join(", ") }
                )));
            };
            for (k, v) in props {
                if !has(call, k) {
                    call.args.push(Arg {
                        name: Some((k.clone(), span)),
                        value: json_to_expr(v, span),
                    });
                }
            }
        }
    }
    if sugar("kf") {
        if let Some(i) = call.args.iter().position(|a| a.name.as_ref().is_some_and(|(n, _)| n == "kf")) {
            let arg = call.args.remove(i);
            let ExprKind::String(text) = &arg.value.kind else {
                return Err(invalid("Motion kf= is keyframe text in a string."));
            };
            if has(call, "layers") {
                return Err(invalid("Motion takes kf= or layers=, not both."));
            }
            let layers = kf_layers(text).map_err(|e| invalid(format!("Motion kf=: {e}")))?;
            call.args.push(Arg {
                name: Some(("layers".into(), span)),
                value: json_to_expr(&layers, span),
            });
        }
    }
    Ok(())
}

/// The registry entry `props` (a Motion component's props) are, if any:
/// every entry prop matches and the component has no Motion prop the entry
/// lacks, apart from `keep` (envelope and placement props).
pub fn registry_match<'a>(ext: &'a Express, comp: &Map<String, Value>, keep: &dyn Fn(&str) -> bool) -> Option<&'a str> {
    const MOTION_PROPS: &[&str] = &[
        "layers", "size", "duration", "fps", "assets", "markers", "lottie", "src", "autoplay", "loop", "plays",
        "direction", "speed", "segment", "marker", "poster", "fit", "stateMachine",
    ];
    let mut names: Vec<&String> = ext.motions.keys().collect();
    names.sort();
    names.into_iter().find_map(|name| {
        let props = ext.motions[name].as_object()?;
        let all_match = props.iter().all(|(k, v)| comp.get(k) == Some(v));
        let none_extra = MOTION_PROPS
            .iter()
            .all(|p| props.contains_key(*p) || !comp.contains_key(*p) || keep(p));
        (all_match && none_extra && !props.is_empty()).then_some(name.as_str())
    })
}

// ---- keyframes ---------------------------------------------------------------------

const EASES: &[&str] = &["linear", "ease", "ease-in", "ease-out", "ease-in-out", "hold"];
const TRACKS: &[&str] = &["scale", "opacity", "rotate", "move"];

fn number(word: &str) -> Result<Value, String> {
    let v = crate::syntax::parser::number_value(word).ok_or_else(|| format!("'{word}' is not a number"))?;
    if !word.bytes().all(|b| b.is_ascii_digit() || b == b'.' || b == b'-') {
        return Err(format!("'{word}' is not a plain number"));
    }
    Ok(match v {
        crate::syntax::NumberValue::Int(i) => json!(i),
        crate::syntax::NumberValue::Float(f) => json!(f),
    })
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap_or(0.0)
}

fn n(x: f64) -> Value {
    if x.fract() == 0.0 && x.abs() < 1e15 {
        json!(x as i64)
    } else {
        json!(x)
    }
}

fn color(word: &str) -> Result<Value, String> {
    let ok = word.strip_prefix("$color.").is_some_and(|t| !t.is_empty() && t.chars().all(|c| c.is_ascii_alphanumeric()))
        || super::color::Rgba::parse(word).is_some_and(|c| c.hex() == word);
    if ok {
        Ok(Value::String(word.to_owned()))
    } else {
        Err(format!("'{word}' is not a colour ($color.<token> or lowercase #rrggbb[aa])"))
    }
}

fn shape(words: &[&str]) -> Result<(Value, Value, Vec<Value>), String> {
    let nums = |ws: &[&str]| ws.iter().map(|w| number(w)).collect::<Result<Vec<_>, _>>();
    let (kind, args) = words.split_first().ok_or("an empty layer")?;
    let expect = |count: usize| {
        if args.len() == count {
            Ok(())
        } else {
            Err(format!("{kind} takes {} values, not {}", count, args.len()))
        }
    };
    match *kind {
        "circle" => {
            expect(4)?;
            let v = nums(&args[..3])?;
            let d = n(f(&v[2]) * 2.0);
            Ok((v[0].clone(), v[1].clone(), vec![
                json!({"type": "ellipse", "position": [v[0], v[1]], "size": [d, d]}),
                json!({"type": "fill", "color": color(args[3])?}),
            ]))
        }
        "ring" => {
            expect(5)?;
            let v = nums(&args[..4])?;
            let d = n(f(&v[2]) * 2.0);
            Ok((v[0].clone(), v[1].clone(), vec![
                json!({"type": "ellipse", "position": [v[0], v[1]], "size": [d, d]}),
                json!({"type": "stroke", "color": color(args[4])?, "width": v[3]}),
            ]))
        }
        "rect" => {
            expect(6)?;
            let v = nums(&args[..5])?;
            Ok((v[0].clone(), v[1].clone(), vec![
                json!({"type": "rect", "position": [v[0], v[1]], "size": [v[2], v[3]], "radius": v[4]}),
                json!({"type": "fill", "color": color(args[5])?}),
            ]))
        }
        "star" => {
            expect(6)?;
            let v = nums(&args[..5])?;
            Ok((v[0].clone(), v[1].clone(), vec![
                json!({"type": "star", "position": [v[0], v[1]], "points": v[2], "outerRadius": v[3], "innerRadius": v[4]}),
                json!({"type": "fill", "color": color(args[5])?}),
            ]))
        }
        other => Err(format!("'{other}' is not a shape (circle, ring, rect, star)")),
    }
}

/// Compiles keyframe text into dialect `layers`.
pub fn kf_layers(text: &str) -> Result<Value, String> {
    let mut layers = Vec::new();
    for layer in text.split(';') {
        let mut parts = layer.split('|');
        let words: Vec<&str> = parts.next().unwrap_or("").split_whitespace().collect();
        let (cx, cy, shapes) = shape(&words)?;
        let mut transform = Map::new();
        transform.insert("anchor".into(), json!([cx, cy]));
        transform.insert("position".into(), json!([cx, cy]));
        let mut seen = Vec::new();
        for part in parts {
            let words: Vec<&str> = part.split_whitespace().collect();
            let (prop, rest) = words.split_first().ok_or("an empty track")?;
            if !TRACKS.contains(prop) {
                return Err(format!("'{prop}' is not a track (scale, opacity, rotate, move)"));
            }
            if seen.contains(prop) {
                return Err(format!("track '{prop}' is given twice"));
            }
            seen.push(*prop);
            let (ease, keys) = match rest.last() {
                Some(e) if EASES.contains(e) => (Some(*e), &rest[..rest.len() - 1]),
                _ => (None, rest),
            };
            if keys.len() < 2 {
                return Err(format!("track '{prop}' needs at least two keys T:V"));
            }
            let mut out = Vec::new();
            let mut last_t = f64::NEG_INFINITY;
            for (i, key) in keys.iter().enumerate() {
                let (t, v) = key.split_once(':').ok_or_else(|| format!("'{key}' is not a key T:V"))?;
                let t = number(t)?;
                if f(&t) <= last_t || f(&t) < 0.0 {
                    return Err(format!("track '{prop}' keys must go forward in time"));
                }
                last_t = f(&t);
                let v = match *prop {
                    "scale" => {
                        let s = number(v)?;
                        json!([s, s])
                    }
                    "move" => {
                        let (dx, dy) = v.split_once(',').ok_or_else(|| format!("move key '{key}' is T:DX,DY"))?;
                        json!([n(f(&cx) + f(&number(dx)?)), n(f(&cy) + f(&number(dy)?))])
                    }
                    _ => number(v)?,
                };
                let mut k = Map::new();
                k.insert("t".into(), t);
                k.insert("v".into(), v);
                if let Some(e) = ease.filter(|e| *e != "linear" && i + 1 < keys.len()) {
                    k.insert("ease".into(), e.into());
                }
                out.push(Value::Object(k));
            }
            let target = match *prop {
                "scale" => "scale",
                "opacity" => "opacity",
                "rotate" => "rotation",
                _ => "position",
            };
            transform.insert(target.into(), json!({"keys": out}));
        }
        layers.push(json!({"type": "shape", "transform": Value::Object(transform), "shapes": shapes}));
    }
    Ok(Value::Array(layers))
}

fn word(v: &Value) -> Option<String> {
    match v {
        Value::Number(x) => Some(x.to_string()),
        _ => None,
    }
}

fn layer_text(layer: &Value) -> Option<String> {
    let shapes = layer.get("shapes")?.as_array()?;
    let [geom, style] = shapes.as_slice() else {
        return None;
    };
    let pos = geom.get("position")?.as_array()?;
    let (cx, cy) = (word(pos.first()?)?, word(pos.get(1)?)?);
    let col = style.get("color")?.as_str()?;
    let mut text = match (geom.get("type")?.as_str()?, style.get("type")?.as_str()?) {
        ("ellipse", "fill") => {
            let r = f(geom.get("size")?.get(0)?) / 2.0;
            format!("circle {cx} {cy} {} {col}", word(&n(r))?)
        }
        ("ellipse", "stroke") => {
            let r = f(geom.get("size")?.get(0)?) / 2.0;
            format!("ring {cx} {cy} {} {} {col}", word(&n(r))?, word(style.get("width")?)?)
        }
        ("rect", "fill") => {
            let size = geom.get("size")?;
            format!(
                "rect {cx} {cy} {} {} {} {col}",
                word(size.get(0)?)?,
                word(size.get(1)?)?,
                word(geom.get("radius")?)?
            )
        }
        ("star", "fill") => format!(
            "star {cx} {cy} {} {} {} {col}",
            word(geom.get("points")?)?,
            word(geom.get("outerRadius")?)?,
            word(geom.get("innerRadius")?)?
        ),
        _ => return None,
    };
    let transform = layer.get("transform")?.as_object()?;
    for (track, key) in [("scale", "scale"), ("opacity", "opacity"), ("rotate", "rotation"), ("move", "position")] {
        let Some(keys) = transform.get(key).and_then(|v| v.get("keys")).and_then(Value::as_array) else {
            continue;
        };
        let ease = keys.first()?.get("ease").and_then(Value::as_str);
        let mut part = format!(" | {track}");
        for k in keys {
            let t = word(k.get("t")?)?;
            let v = k.get("v")?;
            let v = match track {
                "scale" => word(v.get(0)?)?,
                "move" => format!(
                    "{},{}",
                    word(&n(f(v.get(0)?) - f(pos.first()?)))?,
                    word(&n(f(v.get(1)?) - f(pos.get(1)?)))?
                ),
                _ => word(v)?,
            };
            part.push_str(&format!(" {t}:{v}"));
        }
        if let Some(e) = ease {
            part.push(' ');
            part.push_str(e);
        }
        text.push_str(&part);
    }
    Some(text)
}

/// The keyframe text `layers` compile from, when there is one.
pub fn kf_text(layers: &Value) -> Option<String> {
    let parts: Vec<String> = layers.as_array()?.iter().map(layer_text).collect::<Option<_>>()?;
    if parts.is_empty() {
        return None;
    }
    let text = parts.join("; ");
    (kf_layers(&text).ok().as_ref() == Some(layers)).then_some(text)
}

/// A literal Motion prop as JSON (for registry entries written as Express).
pub fn prop_json(e: &Expr) -> Option<Value> {
    literal_json(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyframes_compile_and_write_back() {
        let text = "circle 12 12 6 $color.primary | scale 0:100 600:140 1200:100 ease-in-out | opacity 0:100 600:40 1200:100";
        let layers = kf_layers(text).unwrap();
        assert_eq!(layers[0]["shapes"][0], json!({"type": "ellipse", "position": [12, 12], "size": [12, 12]}));
        assert_eq!(layers[0]["transform"]["scale"]["keys"][0], json!({"t": 0, "v": [100, 100], "ease": "ease-in-out"}));
        assert_eq!(layers[0]["transform"]["scale"]["keys"][2], json!({"t": 1200, "v": [100, 100]}));
        assert_eq!(kf_text(&layers).as_deref(), Some(text));

        let two = "ring 20 20 16 3 #2563eb | rotate 0:0 1000:360; rect 20 20 8 8 2 $color.accent | move 0:0,0 500:0,-6 1000:0,0 ease";
        assert_eq!(kf_text(&kf_layers(two).unwrap()).as_deref(), Some(two));
    }

    #[test]
    fn bad_keyframes_say_why() {
        assert!(kf_layers("blob 1 2").unwrap_err().contains("not a shape"));
        assert!(kf_layers("circle 1 2 3 red").unwrap_err().contains("not a colour"));
        assert!(kf_layers("circle 1 2 3 #ffffff | scale 0:1").unwrap_err().contains("two keys"));
        assert!(kf_layers("circle 1 2 3 #ffffff | scale 5:1 1:2").unwrap_err().contains("forward"));
    }
}
