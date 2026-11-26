//! The `at_exponential_express` extension's conformance fixtures
//! (`conformance/extensions/*.json`, one file per statement), replayed here
//! and by the wasm and Python bindings.
//!
//! A fixture file names its catalog and lists cases:
//!
//! - `compile`: `input` compiles to `expect` (or fails with `expect_error`
//!   `{category, message}`, the message a substring); `decompiled` is what
//!   `decompile` (top-down) writes for `expect`, and it compiles back to
//!   `expect`, so `decompile(compile(x))` is a fixed point. The input is
//!   also streamed one character at a time: the surfaces a renderer holds
//!   afterwards equal those `expect` creates (unless `"stream": false`, a
//!   block for a surface that already exists).
//! - `decompile`: the messages `input` decompile to `expect`.
//! - `expand_theme`: the theme arguments `input` expand to `expect` (golden
//!   JSON).
//! - `patch`: `compile_patch(screen, patch)` returns `expect` and the new
//!   screen `screen_after`; applying `expect` to what `screen` creates
//!   leaves the renderer holding what `screen_after` creates.
//!
//! `EXTENSIONS_BLESS=1 cargo test --test extensions` rewrites every
//! expectation from the current output (review the diff).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use a2ui_express::compile::Compiler;
use a2ui_express::decompile::{DecompileOptions, Order, decompile};
use a2ui_express::response::{ExpressParser, Part};
use a2ui_express::schema::Catalog;
use serde_json::{Map, Value, json};

const FIXTURES: &[&str] = &["theme", "hex", "style", "motion", "patch"];

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/extensions")
}

fn compiler(catalog: &str) -> Compiler {
    let text = fs::read_to_string(dir().join(catalog)).unwrap();
    let mut doc: Value = serde_json::from_str(&text).unwrap();
    // A catalog that does not declare the extension (eUI's, vendored) gets
    // it switched on, as eUI's catalog will declare it.
    if doc
        .pointer("/metadata/extensions/at_exponential_express")
        .is_none()
    {
        doc["metadata"]["extensions"]["at_exponential_express"] = json!(true);
    }
    Compiler::new(vec![Catalog::from_json(doc).unwrap()], None).unwrap()
}

// ---- a renderer model -------------------------------------------------------

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Surface {
    components: BTreeMap<String, Value>,
    data: Value,
    theme: Value,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Model {
    surfaces: BTreeMap<String, Surface>,
}

fn set_pointer(root: &mut Value, path: &str, value: Value) {
    if path == "/" || path.is_empty() {
        *root = value;
        return;
    }
    let tokens: Vec<String> = path
        .trim_start_matches('/')
        .split('/')
        .map(|t| t.replace("~1", "/").replace("~0", "~"))
        .collect();
    let mut cur = root;
    for (i, t) in tokens.iter().enumerate() {
        if !cur.is_object() {
            *cur = Value::Object(Map::new());
        }
        let map = cur.as_object_mut().unwrap();
        if i + 1 == tokens.len() {
            if value.is_null() {
                map.remove(t);
            } else {
                map.insert(t.clone(), value);
            }
            return;
        }
        cur = map
            .entry(t.clone())
            .or_insert_with(|| Value::Object(Map::new()));
    }
}

impl Model {
    pub fn apply(&mut self, m: &Value) {
        let sid = |b: &Value| {
            b["surfaceId"]
                .as_str()
                .unwrap_or("default_surface")
                .to_owned()
        };
        if let Some(b) = m.get("createSurface") {
            let mut s = Surface {
                data: b.get("dataModel").cloned().unwrap_or(json!({})),
                theme: b
                    .pointer("/metadata/extensions/at_exponential_theme")
                    .cloned()
                    .unwrap_or(Value::Null),
                ..Surface::default()
            };
            for c in b["components"].as_array().into_iter().flatten() {
                s.components
                    .insert(c["id"].as_str().unwrap().to_owned(), c.clone());
            }
            self.surfaces.insert(sid(b), s);
        } else if let Some(b) = m.get("updateComponents") {
            let s = self.surfaces.entry(sid(b)).or_default();
            for c in b["components"].as_array().into_iter().flatten() {
                s.components
                    .insert(c["id"].as_str().unwrap().to_owned(), c.clone());
            }
        } else if let Some(b) = m.get("updateDataModel") {
            let s = self.surfaces.entry(sid(b)).or_default();
            if !s.data.is_object() {
                s.data = json!({});
            }
            set_pointer(
                &mut s.data,
                b["path"].as_str().unwrap_or("/"),
                b["value"].clone(),
            );
        } else if let Some(b) = m.get("callRendererFunction") {
            if b.pointer("/callFunction/@call") == Some(&json!("setTheme")) {
                let s = self.surfaces.entry(sid(b)).or_default();
                s.theme = b
                    .pointer("/callFunction/args/theme")
                    .cloned()
                    .unwrap_or(Value::Null);
            }
        } else if let Some(b) = m.get("deleteSurface") {
            self.surfaces.remove(&sid(b));
        }
    }

    /// What a renderer draws: per surface, the components reachable from
    /// `root`, the data model and the theme.
    pub fn drawn(&self) -> BTreeMap<String, (BTreeMap<String, Value>, Value, Value)> {
        self.surfaces
            .iter()
            .map(|(id, s)| {
                let mut seen = BTreeSet::new();
                let mut todo = vec!["root".to_owned()];
                while let Some(c) = todo.pop() {
                    let Some(comp) = s.components.get(&c) else {
                        continue;
                    };
                    if !seen.insert(c) {
                        continue;
                    }
                    let mut strings = Vec::new();
                    collect_strings(comp, &mut strings);
                    todo.extend(strings.into_iter().filter(|x| s.components.contains_key(x)));
                }
                let reachable = s
                    .components
                    .iter()
                    .filter(|(k, _)| seen.contains(*k))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                (id.clone(), (reachable, s.data.clone(), s.theme.clone()))
            })
            .collect()
    }
}

fn collect_strings(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => out.push(s.clone()),
        Value::Array(a) => a.iter().for_each(|x| collect_strings(x, out)),
        Value::Object(m) => m
            .iter()
            .filter(|(k, _)| *k != "id")
            .for_each(|(_, x)| collect_strings(x, out)),
        _ => {}
    }
}

fn model(messages: &[Value]) -> Model {
    let mut m = Model::default();
    messages.iter().for_each(|x| m.apply(x));
    m
}

// ---- the cases ----------------------------------------------------------------

fn error_matches(e: &a2ui_express::compile::CompileError, want: &Value) -> Result<(), String> {
    let category = format!("{:?}", e.category());
    let cat_ok = want["category"]
        .as_str()
        .is_none_or(|c| category.contains(c.trim_end_matches("Error")));
    let msg_ok = want["message"]
        .as_str()
        .is_none_or(|m| e.message.contains(m));
    if cat_ok && msg_ok {
        Ok(())
    } else {
        Err(format!("error {category}: {} (wanted {want})", e.message))
    }
}

fn stream(c: &Compiler, input: &str) -> Result<Vec<Value>, String> {
    let parser = ExpressParser::new(c.clone(), "main");
    let mut s = parser.open_stream();
    let text = format!("<a2ui>\n{input}\n</a2ui>");
    let mut out = Vec::new();
    let mut take = |parts: Vec<Part>| -> Result<(), String> {
        for p in parts {
            match p {
                Part::A2ui(m) => out.extend(m),
                Part::Text(_) => {}
                other => return Err(format!("stream part {other:?}")),
            }
        }
        Ok(())
    };
    for ch in text.chars() {
        take(
            s.push(&ch.to_string())
                .map_err(|e| format!("stream: {e}"))?,
        )?;
    }
    take(s.finish().map_err(|e| format!("stream finish: {e}"))?)?;
    Ok(out)
}

fn run(c: &Compiler, case: &mut Value, bless: bool) -> Result<(), String> {
    let action = case["action"].as_str().unwrap_or_default().to_owned();
    let td = DecompileOptions {
        order: Order::TopDown,
        ..Default::default()
    };
    match action.as_str() {
        "compile" => {
            let input = case["input"].as_str().unwrap().to_owned();
            if let Some(want) = case.get("expect_error").cloned() {
                return match c.compile(&input, "main") {
                    Ok(m) => Err(format!("compiled, wanted an error: {}", Value::Array(m))),
                    Err(e) => error_matches(&e, &want),
                };
            }
            let got = c
                .compile(&input, "main")
                .map_err(|e| format!("{:?}: {}", e.kind, e.message))?;
            if bless {
                case["expect"] = Value::Array(got.clone());
            }
            if case["expect"] != Value::Array(got.clone()) {
                return Err(format!("compiled to {}", Value::Array(got)));
            }
            let text = decompile(c.catalogs(), &got, &td).map_err(|e| format!("decompile: {e}"))?;
            if bless {
                case["decompiled"] = Value::String(text.clone());
            }
            if case["decompiled"].as_str() != Some(text.as_str()) {
                return Err(format!("decompiled to\n{text}"));
            }
            let again = c
                .compile(&text, "main")
                .map_err(|e| format!("recompile: {}", e.message))?;
            if again != got {
                return Err(format!(
                    "decompiled text compiles to {}",
                    Value::Array(again)
                ));
            }
            // `"stream": false`: a block for a surface that exists, which a
            // fresh stream has not seen.
            if case.get("stream") == Some(&Value::Bool(false)) {
                return Ok(());
            }
            let streamed = stream(c, &input)?;
            if model(&streamed).drawn() != model(&got).drawn() {
                return Err(format!("streamed to {}", Value::Array(streamed)));
            }
            Ok(())
        }
        "decompile" => {
            let input: Vec<Value> = serde_json::from_value(case["input"].clone()).unwrap();
            let text =
                decompile(c.catalogs(), &input, &td).map_err(|e| format!("decompile: {e}"))?;
            if bless {
                case["expect"] = Value::String(text.clone());
            }
            (case["expect"].as_str() == Some(text.as_str()))
                .then_some(())
                .ok_or_else(|| format!("decompiled to\n{text}"))
        }
        "expand_theme" => {
            let got = a2ui_express::ext::theme::expand(case["input"].as_object().unwrap());
            if let Some(want) = case.get("expect_error") {
                return match got {
                    Ok(t) => Err(format!("expanded, wanted an error: {t}")),
                    Err(e) if want["message"].as_str().is_none_or(|m| e.contains(m)) => Ok(()),
                    Err(e) => Err(e),
                };
            }
            let got = got?;
            if bless {
                case["expect"] = got.clone();
            }
            (case["expect"] == got)
                .then_some(())
                .ok_or_else(|| format!("expanded to {got}"))
        }
        "patch" => {
            let screen = case["screen"].as_str().unwrap().to_owned();
            let patch = case["patch"].as_str().unwrap().to_owned();
            if let Some(want) = case.get("expect_error").cloned() {
                return match c.compile_patch(&screen, &patch, "main") {
                    Ok(p) => Err(format!(
                        "patched, wanted an error: {}",
                        Value::Array(p.messages)
                    )),
                    Err(e) => error_matches(&e, &want),
                };
            }
            let p = c
                .compile_patch(&screen, &patch, "main")
                .map_err(|e| format!("patch: {}", e.message))?;
            if bless {
                case["expect"] = Value::Array(p.messages.clone());
                case["screen_after"] = Value::String(p.screen.clone());
            }
            if case["expect"] != Value::Array(p.messages.clone()) {
                return Err(format!("patched to {}", Value::Array(p.messages)));
            }
            if case["screen_after"].as_str() != Some(p.screen.as_str()) {
                return Err(format!("new screen\n{}", p.screen));
            }
            // apply(patch, screen) == compile(new screen)
            let mut m = model(&c.compile(&screen, "main").unwrap());
            p.messages.iter().for_each(|x| m.apply(x));
            let fresh = model(&c.compile(&p.screen, "main").map_err(|e| e.message)?);
            (m.drawn() == fresh.drawn())
                .then_some(())
                .ok_or_else(|| "applying the patch differs from compiling the new screen".into())
        }
        other => Err(format!("unknown action {other}")),
    }
}

#[test]
fn extension_fixtures() {
    let bless = std::env::var_os("EXTENSIONS_BLESS").is_some();
    let mut failures = Vec::new();
    let mut count = 0;
    for name in FIXTURES {
        let path = dir().join(format!("{name}.json"));
        let mut doc: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let c = compiler(doc["catalog"].as_str().unwrap_or("catalog.json"));
        for case in doc["cases"].as_array_mut().unwrap() {
            count += 1;
            let cname = case["name"].as_str().unwrap_or("?").to_owned();
            if let Err(e) = run(&c, case, bless) {
                failures.push(format!("{name}/{cname}: {e}"));
            }
        }
        if bless {
            fs::write(&path, serde_json::to_string_pretty(&doc).unwrap() + "\n").unwrap();
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {count} cases failed:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

/// The upstream conformance catalogs do not declare the extension, so the
/// extension's words mean nothing there: `#…` is still a comment and `s=`
/// an unknown property.
#[test]
fn without_the_extension_nothing_changes() {
    let text = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../vendor/a2ui/conformance/test_data/catalogs/simplified_catalog_v1_0.json"),
    )
    .unwrap();
    let c = Compiler::new(vec![Catalog::from_json_str(&text).unwrap()], None).unwrap();
    assert!(c.extension().is_none());
    let m = c.compile("root = Text(\"x\") #eef800", "main").unwrap();
    assert_eq!(m[0]["createSurface"]["components"][0]["text"], "x");
    assert!(c.compile("root = Text(\"x\", s=\"p16\")", "main").is_err());
    assert!(
        c.compile_patch("root = Text(\"x\")", "root = Text(\"y\")", "main")
            .is_err()
    );
}

// ---- eUI's sample surfaces ------------------------------------------------------

/// eUI's catalog (vendored) with the extension switched on, as its catalog
/// will declare it.
fn eui() -> Compiler {
    compiler("../../vendor/eui/catalog/core.schema.json")
}

fn eui_source(name: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/eui/{name}.a2ui")))
        .unwrap()
}

/// Order-insensitive for keys, numbers by value.
fn json_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| json_eq(a, b))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| json_eq(v, w)))
        }
        _ => a == b,
    }
}

/// The plain Express of eUI's surfaces decompiles with the sugar, compiles
/// back to the same messages, and the sugared text is a fixed point.
#[test]
fn eui_surfaces_round_trip_through_the_sugar() {
    let c = eui();
    let td = DecompileOptions {
        order: Order::TopDown,
        ..Default::default()
    };
    for name in ["kitchen-sink", "dashboard", "demo-surface"] {
        let plain = eui_source(name);
        let messages = c
            .compile(&plain, "main")
            .unwrap_or_else(|e| panic!("{name}: {}", e.message));
        let sugared = decompile(c.catalogs(), &messages, &td).unwrap();
        let again = c
            .compile(&sugared, "main")
            .unwrap_or_else(|e| panic!("{name}: recompile: {}\n{sugared}", e.message));
        let by_id = |m: &[Value]| -> BTreeMap<String, Value> {
            m[0]["createSurface"]["components"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| (c["id"].as_str().unwrap().to_owned(), c.clone()))
                .collect()
        };
        let (a, b) = (by_id(&messages), by_id(&again));
        for (id, comp) in &a {
            assert!(
                b.get(id).is_some_and(|x| json_eq(x, comp)),
                "{name}: {id} differs\n got {}\nwant {comp}",
                b.get(id).map_or("nothing".into(), |x| x.to_string())
            );
        }
        assert_eq!(a.len(), b.len(), "{name}: component count");
        let fixed = decompile(c.catalogs(), &again, &td).unwrap();
        assert_eq!(fixed, sugared, "{name}: not a fixed point");
    }
}

/// The sugar writes eUI's sample surfaces shorter (printed). They are
/// mostly data; on model-written screens, where style maps dominate, it
/// saves about half (docs/express-extensions.md, Measurements).
#[test]
fn the_sugar_is_shorter() {
    let c = eui();
    let plain_cat = {
        let text = fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/eui/catalog/core.schema.json"),
        )
        .unwrap();
        Compiler::new(vec![Catalog::from_json_str(&text).unwrap()], None).unwrap()
    };
    let td = DecompileOptions {
        order: Order::TopDown,
        ..Default::default()
    };
    for name in ["kitchen-sink", "dashboard", "demo-surface"] {
        let messages = c.compile(&eui_source(name), "main").unwrap();
        let plain = decompile(plain_cat.catalogs(), &messages, &td).unwrap();
        let sugared = decompile(c.catalogs(), &messages, &td).unwrap();
        let ratio = sugared.len() as f64 / plain.len() as f64;
        println!(
            "{name}: plain {} chars, sugared {} ({:.0}%)",
            plain.len(),
            sugared.len(),
            ratio * 100.0
        );
        let bound = if name == "kitchen-sink" { 0.95 } else { 1.0 };
        assert!(
            ratio < bound,
            "{name}: the sugar saves too little ({ratio})"
        );
    }
}

// ---- patches, as a property ------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn pick<T: Clone>(&mut self, v: &[T]) -> T {
        v[(self.next() % v.len() as u64) as usize].clone()
    }
}

/// apply(patch, screen) == compile(new screen), for random patches of eUI's
/// surfaces: reassigned components (a string changed), deleted list
/// children, changed data paths, and several at once.
#[test]
fn patches_apply_like_the_new_screen_compiles() {
    let c = eui();
    let td = DecompileOptions {
        order: Order::TopDown,
        ..Default::default()
    };
    let mut rng = Rng(0x9e3779b97f4a7c15);
    let (mut ok, mut tried) = (0, 0);
    for name in ["kitchen-sink", "dashboard", "demo-surface"] {
        let messages = c.compile(&eui_source(name), "main").unwrap();
        let screen = decompile(c.catalogs(), &messages, &td).unwrap();
        let lines: Vec<&str> = screen.lines().collect();
        let assigns: Vec<&str> = lines
            .iter()
            .copied()
            .filter(|l| l.contains(" = ") && l.contains("(\""))
            .collect();
        let data: Vec<&str> = lines
            .iter()
            .copied()
            .filter(|l| l.starts_with("$/"))
            .collect();
        let listed: Vec<String> = lines
            .iter()
            .flat_map(|l| {
                let inner = l
                    .split_once('[')
                    .and_then(|(_, r)| r.split_once(']'))
                    .map(|(i, _)| i)
                    .unwrap_or("");
                inner
                    .split(", ")
                    .filter(|w| a2ui_express::syntax::is_identifier(w))
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .filter(|w| lines.iter().any(|l| l.starts_with(&format!("{w} = "))))
            .collect();
        for round in 0..40 {
            let mut patch = Vec::new();
            for _ in 0..(1 + rng.next() % 3) {
                match rng.next() % 3 {
                    0 if !assigns.is_empty() => {
                        let line = rng.pick(&assigns);
                        let (head, rest) = line.split_once("(\"").unwrap();
                        let rest = rest.split_once('"').map_or(rest, |(_, r)| r);
                        patch.push(format!("{head}(\"patched {round}\"{rest}"));
                    }
                    1 if !listed.is_empty() => patch.push(format!("del({})", rng.pick(&listed))),
                    _ if !data.is_empty() => {
                        let line = rng.pick(&data);
                        let path = line.split_once(" = ").unwrap().0;
                        patch.push(format!("{path} = \"patched {round}\""));
                    }
                    _ => {}
                }
            }
            let patch = patch.join("\n");
            tried += 1;
            let Ok(p) = c.compile_patch(&screen, &patch, "main") else {
                continue;
            };
            ok += 1;
            let mut m = model(&messages);
            p.messages.iter().for_each(|x| m.apply(x));
            let fresh = model(&c.compile(&p.screen, "main").unwrap());
            assert_eq!(
                m.drawn(),
                fresh.drawn(),
                "{name}: patch\n{patch}\ndiffers from compiling the new screen"
            );
            // Untouched components are never re-sent.
            let before: BTreeMap<String, Value> =
                model(&messages).surfaces["main"].components.clone();
            for msg in &p.messages {
                for comp in msg
                    .pointer("/updateComponents/components")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    assert_ne!(
                        before.get(comp["id"].as_str().unwrap()),
                        Some(comp),
                        "{name}: re-sent an unchanged component"
                    );
                }
            }
        }
    }
    assert!(
        ok * 2 >= tried,
        "only {ok} of {tried} random patches compiled"
    );
    println!("{ok} of {tried} random patches compiled and applied");
}
