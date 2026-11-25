//! Streaming equals batch: every conformance input, fed in small chunks.
//!
//! For each case of the compiler, response parser and response streaming
//! suites, the input is read once whole (`parse_response`) and again through
//! a stream, one character at a time and in chunks of 3, 7 and 64 bytes. The
//! outcomes must agree:
//!
//! - both fail, with the same error category; or
//! - both succeed, the text is the same, and the surfaces a renderer holds
//!   after applying the streamed messages equal those after the batch
//!   messages. A component the stream emitted under a provisional id (a
//!   healed statement whose `id=` came later) may remain, as it remains in a
//!   renderer, but nothing may reference it.
//!
//! Every streamed message is also checked when it is emitted: the stream
//! applies it to its renderer state, which rejects an update to a surface
//! that does not exist or a component the catalog does not declare.

#[allow(dead_code)]
#[path = "conformance/cases.rs"]
mod cases;
#[allow(dead_code)]
#[path = "conformance/check.rs"]
mod check;
#[path = "conformance/sdk.rs"]
mod sdk;

use std::collections::BTreeMap;

use a2ui_express::response::{AfterRejectedBlock, ExpressParser, Part};
use serde_json::{Map, Value};

const SURFACE: &str = "default_surface";

#[derive(Debug, Default, PartialEq)]
struct Model {
    surfaces: BTreeMap<String, (BTreeMap<String, Value>, Value)>,
}

impl Model {
    fn apply(&mut self, m: &Value) {
        let sid = |b: &Value| b["surfaceId"].as_str().unwrap_or_default().to_owned();
        if let Some(b) = m.get("createSurface") {
            let mut comps = BTreeMap::new();
            for c in b
                .get("components")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                comps.insert(c["id"].as_str().unwrap().to_owned(), c.clone());
            }
            let data = b
                .get("dataModel")
                .cloned()
                .unwrap_or(Value::Object(Map::new()));
            self.surfaces.insert(sid(b), (comps, data));
        } else if let Some(b) = m.get("updateComponents") {
            let s = self.surfaces.entry(sid(b)).or_default();
            for c in b["components"].as_array().into_iter().flatten() {
                s.0.insert(c["id"].as_str().unwrap().to_owned(), c.clone());
            }
        } else if let Some(b) = m.get("updateDataModel") {
            let s = self.surfaces.entry(sid(b)).or_default();
            let path = b["path"].as_str().unwrap_or("/");
            let value = b["value"].clone();
            if path == "/" || path.is_empty() {
                s.1 = value;
            } else {
                if !s.1.is_object() {
                    s.1 = Value::Object(Map::new());
                }
                let mut cur = &mut s.1;
                let tokens: Vec<&str> = path.trim_start_matches('/').split('/').collect();
                for (i, t) in tokens.iter().enumerate() {
                    let t = t.replace("~1", "/").replace("~0", "~");
                    if !cur.is_object() {
                        *cur = Value::Object(Map::new());
                    }
                    let map = cur.as_object_mut().unwrap();
                    if i == tokens.len() - 1 {
                        map.insert(t, value.clone());
                        break;
                    }
                    cur = map.entry(t).or_insert_with(|| Value::Object(Map::new()));
                }
            }
        } else if let Some(b) = m.get("deleteSurface") {
            self.surfaces.remove(&sid(b));
        }
    }
}

#[derive(Debug)]
enum Outcome {
    Ok { text: String, model: Model },
    Err(String),
}

fn category(e: &a2ui_express::Error) -> String {
    match e {
        a2ui_express::Error::Parse(_) => "ParseError".into(),
        a2ui_express::Error::Validation(_) => "ValidationError".into(),
    }
}

fn collect(parts: Vec<Part>, text: &mut String, model: &mut Model) {
    for p in parts {
        match p {
            Part::Text(t) => text.push_str(&t),
            Part::A2ui(messages) => messages.iter().for_each(|m| model.apply(m)),
            Part::Error(e) => panic!("unexpected error part {e}"),
        }
    }
}

fn batch(parser: &ExpressParser, input: &str, wrapped: bool) -> Outcome {
    match parser.parse_response(input, wrapped) {
        Ok(parts) => {
            let (mut text, mut model) = (String::new(), Model::default());
            collect(parts, &mut text, &mut model);
            Outcome::Ok { text, model }
        }
        Err(e) => Outcome::Err(category(&e.error)),
    }
}

fn streamed(parser: &ExpressParser, chunks: &[&str], wrapped: bool) -> Outcome {
    let mut stream = parser.open_stream_with(wrapped, AfterRejectedBlock::End, parser.new_state());
    let (mut text, mut model) = (String::new(), Model::default());
    for chunk in chunks {
        match stream.push(chunk) {
            Ok(parts) => collect(parts, &mut text, &mut model),
            Err(e) => return Outcome::Err(category(&e.error)),
        }
    }
    match stream.finish() {
        Ok(parts) => collect(parts, &mut text, &mut model),
        Err(e) => return Outcome::Err(category(&e.error)),
    }
    Outcome::Ok { text, model }
}

fn chunks(input: &str, size: usize) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    while start < input.len() {
        let mut end = (start + size).min(input.len());
        while !input.is_char_boundary(end) {
            end += 1;
        }
        out.push(&input[start..end]);
        start = end;
    }
    out
}

fn mentions(value: &Value, id: &str) -> bool {
    match value {
        Value::String(s) => s == id,
        Value::Array(a) => a.iter().any(|v| mentions(v, id)),
        Value::Object(m) => m.iter().any(|(k, v)| k != "id" && mentions(v, id)),
        _ => false,
    }
}

fn agree(batch: &Outcome, streamed: &Outcome) -> Result<(), String> {
    match (batch, streamed) {
        (Outcome::Err(a), Outcome::Err(b)) if a == b || a == "*" => Ok(()),
        (
            Outcome::Ok {
                text: t1,
                model: m1,
            },
            Outcome::Ok {
                text: t2,
                model: m2,
            },
        ) => {
            if t1.split_whitespace().ne(t2.split_whitespace()) {
                return Err(format!("text differs:\n batch:  {t1:?}\n stream: {t2:?}"));
            }
            if m1.surfaces.keys().ne(m2.surfaces.keys()) {
                return Err(format!(
                    "surfaces differ: {:?} vs {:?}",
                    m1.surfaces.keys(),
                    m2.surfaces.keys()
                ));
            }
            for (sid, (c1, d1)) in &m1.surfaces {
                let (c2, d2) = &m2.surfaces[sid];
                if d1 != d2 {
                    return Err(format!("{sid}: data model {d1} vs {d2}"));
                }
                for (id, c) in c1 {
                    if c2.get(id) != Some(c) {
                        return Err(format!("{sid}/{id}: {c} vs {:?}", c2.get(id)));
                    }
                }
                for id in c2.keys().filter(|id| !c1.contains_key(*id)) {
                    if c1.values().any(|c| mentions(c, id)) {
                        return Err(format!("{sid}: stray component {id} is referenced"));
                    }
                }
            }
            Ok(())
        }
        (a, b) => Err(format!("batch {a:?}\n stream {b:?}")),
    }
}

/// The inputs a suite gives a parser: (case, input, wrapped, progressive keys).
fn inputs() -> Vec<(String, Value, String, bool)> {
    let mut out = Vec::new();
    for suite in ["compiler", "response_parser", "response_streaming"] {
        for case in cases::load_suite(suite).expect("suite loads") {
            let wrapped = case
                .args
                .get("wrapped")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let input = match case.action.as_str() {
                "compile" => {
                    let Some(i) = case.get("input").and_then(Value::as_str) else {
                        continue;
                    };
                    out.push((case.name.clone(), case.args.clone(), i.to_owned(), false));
                    continue;
                }
                "parse_response" => case.get("input").and_then(Value::as_str).map(str::to_owned),
                "parse_chunk" => {
                    if case
                        .args
                        .get("after_rejected_block")
                        .and_then(Value::as_str)
                        == Some("continue")
                    {
                        continue;
                    }
                    Some(
                        case.get("steps")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .filter_map(|s| s.get("input").and_then(Value::as_str))
                            .collect::<String>(),
                    )
                }
                _ => None,
            };
            if let Some(input) = input {
                out.push((case.name.clone(), case.args.clone(), input, wrapped));
            }
        }
    }
    out
}

/// A case whose catalogs cannot form a compiler (the suites test those
/// errors elsewhere) is skipped.
fn parser_for(args: &Value) -> Option<ExpressParser> {
    let catalogs = cases::resolve_catalogs(args).ok()?;
    let compiler = sdk::compiler(&catalogs).ok()?;
    let p = ExpressParser::new(compiler, SURFACE);
    Some(
        match args.get("progressive_keys").and_then(Value::as_array) {
            Some(keys) => p.with_progressive_keys(keys.iter().filter_map(Value::as_str)),
            None => p,
        },
    )
}

/// Where upstream's stream and its single-shot parser part ways, by design or
/// by an upstream quirk this port keeps.
const DIVERGES: &[(&str, &str)] = &[
    (
        "test_express_stream_raw_string_ending_in_backslash_before_close_tag",
        "single-shot `unwrap` reads `\\\"` in a raw string as an escape, so the block never closes there; the stream reads raw strings as Express does",
    ),
    (
        "test_express_stream_update_surface_heals_reassigned_custom_id_without_dropping_baseline",
        "the stream healed `hdr` live before its `id=` arrived; upstream restores the agent's state but sends the renderer nothing, so the renderer keeps the healed text",
    ),
];

/// A wrapped response that ends inside a block: the stream rejects it
/// (`ParseError`), while single-shot parsing compiles the block as far as it
/// reads.
fn ends_inside_a_block(input: &str, wrapped: bool) -> bool {
    wrapped
        && matches!(
            a2ui_express::response::unwrap(input).last(),
            Some(a2ui_express::response::RawPart::A2ui {
                is_final: false,
                ..
            })
        )
}

#[test]
fn streaming_in_small_chunks_equals_batch() {
    let mut failures = Vec::new();
    let mut checked = 0;
    for (name, args, input, wrapped) in inputs() {
        let Some(parser) = parser_for(&args) else {
            continue;
        };
        if DIVERGES.iter().any(|(n, _)| *n == name) {
            continue;
        }
        let whole = if ends_inside_a_block(&input, wrapped) {
            // Any error: the stream rejects the block at the latest when it
            // ends, or earlier at a statement that fails.
            Outcome::Err("*".into())
        } else {
            batch(&parser, &input, wrapped)
        };
        for size in [1, 3, 7, 64] {
            let got = streamed(&parser, &chunks(&input, size), wrapped);
            if let Err(e) = agree(&whole, &got) {
                failures.push(format!("{name} (chunks of {size}): {e}"));
                break;
            }
        }
        checked += 1;
    }
    assert!(checked > 150, "only {checked} inputs checked");
    assert!(
        failures.is_empty(),
        "{} of {checked} inputs stream differently from batch:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

fn eui_parser() -> ExpressParser {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let catalog =
        std::fs::read_to_string(dir.join("../../vendor/eui/catalog/core.schema.json")).unwrap();
    let catalog = a2ui_express::schema::Catalog::from_json_str(&catalog).unwrap();
    let compiler = a2ui_express::compile::Compiler::new(vec![catalog], None).unwrap();
    ExpressParser::new(compiler, "main")
}

/// eui's sample surfaces (kitchen-sink: 251 components), typed in one
/// character at a time, render the same surface as compiled whole, and
/// render their root as soon as its statement is complete.
#[test]
fn eui_fixtures_stream_character_by_character() {
    let parser = eui_parser();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/eui");
    for name in ["demo-surface", "dashboard", "kitchen-sink"] {
        let source = std::fs::read_to_string(dir.join(format!("{name}.a2ui"))).unwrap();
        let start = std::time::Instant::now();
        let whole = batch(&parser, &source, false);
        let batch_time = start.elapsed();

        let mut stream =
            parser.open_stream_with(false, AfterRejectedBlock::End, parser.new_state());
        let first_line = source.find('\n').unwrap() + 1;
        let (mut text, mut model) = (String::new(), Model::default());
        let start = std::time::Instant::now();
        let mut rendered_at = None;
        for (i, c) in source.char_indices() {
            let parts = stream
                .push(&source[i..i + c.len_utf8()])
                .unwrap_or_else(|e| panic!("{name} at byte {i}: {e}"));
            if rendered_at.is_none() && parts.iter().any(|p| matches!(p, Part::A2ui(_))) {
                rendered_at = Some(i + 1);
            }
            collect(parts, &mut text, &mut model);
        }
        collect(stream.finish().unwrap(), &mut text, &mut model);
        let stream_time = start.elapsed();
        eprintln!(
            "{name}: {} bytes, batch {batch_time:?}, char-by-char stream {stream_time:?}, first render at byte {rendered_at:?}",
            source.len()
        );
        agree(&whole, &Outcome::Ok { text, model }).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(
            rendered_at.is_some_and(|at| at <= first_line + 1),
            "{name}: root rendered at {rendered_at:?}, its line ends at {first_line}"
        );
    }
}

/// Training data written with `Order::TopDown` streams root first: a
/// surface whose messages list its leaves first, decompiled top-down, renders
/// its root as soon as the first statement is complete, and every later
/// statement adds to what is on screen.
#[test]
fn top_down_samples_render_their_root_first() {
    use a2ui_express::decompile::{DecompileOptions, Order, decompile};
    let parser = eui_parser();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/eui");
    for name in ["demo-surface", "dashboard", "kitchen-sink"] {
        let expected: Value = serde_json::from_str(
            &std::fs::read_to_string(dir.join(format!("{name}.expected.json"))).unwrap(),
        )
        .unwrap();
        // The worst case for streaming: the components listed leaves first.
        let mut messages = expected.as_array().unwrap().clone();
        if let Some(list) = messages[0]["createSurface"]["components"].as_array_mut() {
            list.reverse();
        }
        let options = DecompileOptions {
            keyword_args: true,
            order: Order::TopDown,
        };
        let express = decompile(parser.compiler().catalogs(), &messages, &options).unwrap();
        // `root` is the first component statement (after the `surface(...)`
        // line and the data model, which wait for it).
        let mut body = 0;
        for line in express.split_inclusive('\n') {
            if !(line.starts_with("surface(") || line.starts_with('$')) {
                break;
            }
            body += line.len();
        }
        assert!(
            express[body..].starts_with("root = "),
            "{name}: {express:.120}"
        );

        let mut stream =
            parser.open_stream_with(false, AfterRejectedBlock::End, parser.new_state());
        let first_line = body + express[body..].find('\n').unwrap() + 1;
        let mut rendered_at = None;
        let mut updates = 0;
        for (i, c) in express.char_indices() {
            let parts = stream.push(&express[i..i + c.len_utf8()]).unwrap();
            if parts.iter().any(|p| matches!(p, Part::A2ui(_))) {
                rendered_at.get_or_insert(i + 1);
                updates += 1;
            }
        }
        stream.finish().unwrap();
        assert!(
            rendered_at.is_some_and(|at| at <= first_line + 1),
            "{name}: root rendered at {rendered_at:?}"
        );
        assert!(updates > 1, "{name}: rendered in {updates} update(s)");
    }
}
