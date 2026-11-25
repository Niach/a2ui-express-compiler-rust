//! Running one case, per action, the way upstream's Python harness
//! (`python/a2ui_agent/tests/conformance/test_conformance.py`) runs it.

use std::time::Instant;

use serde_json::{Value, json};

use crate::cases::{Case, read_json, resolve_catalogs};
use crate::check::*;
use crate::sdk::{self, AfterRejectedBlock, ParserConfig, Part, PromptConfig};

pub const SURFACE_ID: &str = "default_surface";

type Outcome = Result<(), String>;

struct Run<'a> {
    case: &'a Case,
}

impl Run<'_> {
    fn get(&self, key: &str) -> Option<&Value> {
        self.case.get(key)
    }

    fn arg(&self, key: &str) -> Option<&Value> {
        self.case.args.get(key)
    }

    fn str_field(&self, key: &str) -> Result<&str, String> {
        self.get(key)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("case has no string {key:?}"))
    }

    fn parser(&self) -> Result<ParserConfig, String> {
        let progressive_keys = self
            .arg("progressive_keys")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            });
        Ok(ParserConfig {
            catalogs: resolve_catalogs(&self.case.args)?,
            surface_id: SURFACE_ID.to_owned(),
            progressive_keys,
        })
    }

    fn parts_to_json(&self, parts: &[Part]) -> Vec<Value> {
        parts.iter().map(part_to_json).collect()
    }

    fn compile(&self) -> Outcome {
        let parser = self.parser()?;
        let input = self.str_field("input")?;
        let result = sdk::compile_checked(&parser, input);
        if let Some(expected) = self.get("expect_error") {
            return expect_failure(result, expected).map(drop);
        }
        let mut compiled = Value::Array(ok("compile", result)?);
        for pointer in self
            .get("expect_present")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let pointer = pointer
                .as_str()
                .ok_or("expect_present holds a non-string")?;
            match resolve_pointer(&compiled, pointer) {
                None | Some(Value::Null) => return Err(format!("{pointer} is missing")),
                Some(Value::String(s)) if s.is_empty() => {
                    return Err(format!("{pointer} is empty"));
                }
                Some(_) => delete_pointer(&mut compiled, pointer),
            }
        }
        expect_json(
            "compiled messages",
            &compiled,
            self.get("expect").unwrap_or(&Value::Null),
        )
    }

    fn decompile(&self) -> Outcome {
        let parser = self.parser()?;
        let original = self
            .get("messages")
            .and_then(Value::as_array)
            .ok_or("case has no messages")?;
        let result = sdk::decompile(&parser, original);
        if let Some(expected) = self.get("expect_error") {
            return expect_failure(result, expected).map(drop);
        }
        let notation = ok("decompile", result)?;
        expect_fragments(
            &notation,
            self.get("expect_contains"),
            self.get("expect_absent"),
        )?;
        let recompile = || -> Result<Value, String> {
            let messages = ok("recompile", sdk::compile_checked(&parser, &notation))
                .map_err(|e| format!("{e}\n--- notation\n{notation}"))?;
            Ok(Value::Array(messages))
        };
        if self.get("expect_round_trip") == Some(&json!(true)) {
            expect_json("round trip", &recompile()?, &Value::Array(original.clone()))?;
        }
        if let Some(expected) = self.get("expect_recompiled") {
            expect_json("recompiled messages", &recompile()?, expected)?;
        }
        Ok(())
    }

    fn unwrap(&self) -> Outcome {
        let parser = self.parser()?;
        let input = self.str_field("input")?;
        let expected = self.get("expect").cloned().unwrap_or(json!([]));
        let parts = ok("unwrap", sdk::unwrap(&parser, input))?;
        let actual = Value::Array(parts.iter().map(raw_part_to_json).collect());
        expect_json("raw parts", &actual, &without_final_flags(&expected))?;
        let expected_parts = expected.as_array().cloned().unwrap_or_default();
        let has_any = expected_parts.iter().any(|p| p.get("a2ui_raw").is_some());
        let has_complete = expected_parts.iter().any(|p| {
            p.get("a2ui_raw").is_some()
                && p.get("is_final").and_then(Value::as_bool).unwrap_or(true)
        });
        let any = ok(
            "has_format_content",
            sdk::has_format_content(&parser, input, false),
        )?;
        expect_bool("has_format_content", any, has_any)?;
        let complete = ok(
            "has_format_content",
            sdk::has_format_content(&parser, input, true),
        )?;
        expect_bool("has_format_content(complete)", complete, has_complete)
    }

    fn has_format_content(&self) -> Outcome {
        let parser = self.parser()?;
        let complete = self
            .arg("complete")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let expected = self
            .get("expect")
            .and_then(Value::as_bool)
            .ok_or("expect is not a bool")?;
        let input = self.str_field("input")?;
        let actual = ok(
            "has_format_content",
            sdk::has_format_content(&parser, input, complete),
        )?;
        expect_bool("has_format_content", actual, expected)
    }

    fn wrap(&self) -> Outcome {
        let parser = self.parser()?;
        let parts = self.get("parts").ok_or("case has no parts")?;
        let output = ok("wrap", sdk::wrap(&parser, &to_raw_parts(parts)?))?;
        if let Some(expected) = self.get("expect_output") {
            expect_text(
                "wrapped output",
                &output,
                expected.as_str().unwrap_or_default(),
            )?;
        }
        expect_fragments(&output, self.get("expect_contains"), None)?;
        if self.get("expect_round_trip") == Some(&json!(true)) {
            let unwrapped = ok("unwrap", sdk::unwrap(&parser, &output))?;
            let actual = Value::Array(unwrapped.iter().map(raw_part_to_json).collect());
            expect_json("round trip", &actual, &without_final_flags(parts))?;
        }
        Ok(())
    }

    fn parse_response(&self) -> Outcome {
        let parser = self.parser()?;
        let wrapped = self.arg("wrapped").and_then(Value::as_bool).unwrap_or(true);
        let result = sdk::parse_response(&parser, self.str_field("input")?, wrapped);
        if let Some(expected) = self.get("expect_error") {
            return expect_failure(result, expected).map(drop);
        }
        let parts = self.parts_to_json(&ok("parse_response", result)?);
        expect_json(
            "parts",
            &Value::Array(parts),
            self.get("expect").unwrap_or(&Value::Null),
        )
    }

    fn parse_chunk(&self) -> Outcome {
        let parser = self.parser()?;
        let wrapped = self.arg("wrapped").and_then(Value::as_bool).unwrap_or(true);
        let after = match self.arg("after_rejected_block").and_then(Value::as_str) {
            None | Some("end") => AfterRejectedBlock::End,
            Some("continue") => AfterRejectedBlock::Continue,
            Some(other) => return Err(format!("unknown after_rejected_block {other:?}")),
        };
        let mut stream = ok("open_stream", sdk::open_stream(&parser, wrapped, after))?;
        let steps = self
            .get("steps")
            .and_then(Value::as_array)
            .ok_or("case has no steps")?;
        let mut chunks = String::new();
        let mut yielded: Vec<Value> = Vec::new();

        for (index, step) in steps.iter().enumerate() {
            let input = step
                .get("input")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("step {index} has no input"))?;
            chunks.push_str(input);
            let result = stream.feed(input);
            let check_in_block = |in_block: bool| match step.get("expect_in_block") {
                Some(expected) => expect_bool(
                    &format!("step {index}: in block"),
                    in_block,
                    expected.as_bool().unwrap_or_default(),
                ),
                None => Ok(()),
            };
            if let Some(expected) = step.get("expect_error") {
                let error = expect_failure(result, expected)
                    .map_err(|e| format!("step {index} ({input:?}): {e}"))?;
                if let Some(partial) = step.get("expect_partial_results") {
                    let actual = Value::Array(self.parts_to_json(&error.partial_results));
                    expect_json(&format!("step {index}: partial results"), &actual, partial)?;
                }
                return check_in_block(stream.is_in_block());
            }
            let parts = self.parts_to_json(&ok(&format!("step {index}: feed"), result)?);
            expect_json(
                &format!("step {index} ({input:?})"),
                &Value::Array(parts.clone()),
                step.get("expect").unwrap_or(&json!([])),
            )?;
            check_in_block(stream.is_in_block())?;
            yielded.extend(parts);
        }

        if let Some(expected) = self.get("expect_error") {
            return expect_failure(stream.close(), expected).map(drop);
        }
        let closed = self.parts_to_json(&ok("close", stream.close())?);
        if let Some(expected) = self.get("expect_close") {
            expect_json("close", &Value::Array(closed.clone()), expected)?;
        }
        yielded.extend(closed);

        if self.get("expect_matches_single_shot") == Some(&json!(true)) {
            let fresh = self.parser()?;
            let single = ok(
                "parse_response",
                sdk::parse_response(&fresh, &chunks, wrapped),
            )?;
            expect_json(
                "streamed parts against single-shot parse_response",
                &Value::Array(merge_text_parts(&yielded)),
                &Value::Array(merge_text_parts(&self.parts_to_json(&single))),
            )?;
        }
        Ok(())
    }

    fn generate_prompt_snippet(&self) -> Outcome {
        let catalogs = resolve_catalogs_for_prompt(&self.case.args)?;
        let mut examples = Vec::new();
        for path in self
            .arg("examples")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let path = path.as_str().ok_or("examples holds a non-string")?;
            let messages = match read_json(path)? {
                Value::Array(items) => items,
                single => vec![single],
            };
            examples.push(messages);
        }
        let allowed_messages = self
            .arg("allowed_messages")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            });
        let config = PromptConfig {
            catalogs,
            surface_id: SURFACE_ID.to_owned(),
            examples,
            allowed_messages,
        };
        let result = sdk::generate_prompt(&config);
        if let Some(expected) = self.get("expect_error") {
            return expect_failure(result, expected).map(drop);
        }
        let snippet = ok("generate_prompt", result)?;
        if let Some(path) = self.get("expect_file").and_then(Value::as_str) {
            let file = crate::cases::conformance_root().join(path);
            let expected =
                std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            expect_text("snippet", &snippet, &expected)?;
        }
        expect_fragments(
            &snippet,
            self.get("expect_contains"),
            self.get("expect_absent"),
        )?;
        if let Some(expected) = self.get("expect_callable_functions") {
            let actual = ok(
                "agent_callable_functions",
                sdk::agent_callable_functions(&config.catalogs),
            )?;
            expect_json("agent-callable functions", &json!(actual), expected)?;
            expect_fragments(&snippet, Some(expected), None)?;
        }
        if self.get("expect_deterministic") == Some(&json!(true)) {
            let again = ok("generate_prompt", sdk::generate_prompt(&config))?;
            expect_text("second snippet", &again, &snippet)?;
        }
        Ok(())
    }

    fn measure_growth(&self) -> Outcome {
        let operation = self.str_field("operation")?;
        if operation != "compile" {
            return Err(format!("unsupported operation {operation:?}"));
        }
        let parser = self.parser()?;
        let template = self.get("payload").ok_or("case has no payload")?;
        let sizes: Vec<usize> = self
            .get("sizes")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_u64().map(|n| n as usize))
                    .collect()
            })
            .unwrap_or_default();
        let [small, large] = sizes[..] else {
            return Err(format!("expected two sizes, got {sizes:?}"));
        };
        let max_ratio = self
            .get("max_ratio")
            .and_then(Value::as_f64)
            .ok_or("no max_ratio")?;
        let payloads = [
            write_payload(template, small)?,
            write_payload(template, large)?,
        ];

        // `compile` is `parse_response(payload, wrapped=False)`, as upstream
        // times it. Warm each size once, then time each at least five times
        // and compare the medians.
        let time = |payload: &str| -> Result<f64, String> {
            let start = Instant::now();
            ok("compile", sdk::parse_response(&parser, payload, false))?;
            Ok(start.elapsed().as_secs_f64())
        };
        for payload in &payloads {
            time(payload)?;
        }
        let mut samples = [Vec::new(), Vec::new()];
        for _ in 0..7 {
            for (i, payload) in payloads.iter().enumerate() {
                samples[i].push(time(payload)?);
            }
        }
        let median = |s: &mut Vec<f64>| {
            s.sort_by(f64::total_cmp);
            s[s.len() / 2]
        };
        let (t_small, t_large) = (median(&mut samples[0]), median(&mut samples[1]));
        let ratio = t_large / t_small.max(1e-9);
        if ratio > max_ratio {
            return Err(format!(
                "compile at n={large} took {ratio:.1}x as long as at n={small} (limit {max_ratio})"
            ));
        }
        Ok(())
    }
}

/// A prompt case lists `args.catalogs`; one without any registers none, which
/// the format has to reject (`CatalogError`).
fn resolve_catalogs_for_prompt(args: &Value) -> Result<Vec<sdk::Catalog>, String> {
    if args.get("catalogs").is_none() && args.get("catalog").is_none() {
        return Ok(Vec::new());
    }
    resolve_catalogs(args)
}

/// Writes a `PayloadTemplate` out at size `n`: strings as they are, and each
/// `repeat` `n` times with `{i}` counting from 0, joined by `separator`.
pub fn write_payload(template: &Value, n: usize) -> Result<String, String> {
    let mut out = String::new();
    for segment in template.as_array().ok_or("payload is not a list")? {
        match segment {
            Value::String(s) => out.push_str(s),
            Value::Object(map) => {
                let repeat = map
                    .get("repeat")
                    .and_then(Value::as_str)
                    .ok_or("repeat segment without repeat")?;
                let separator = map.get("separator").and_then(Value::as_str).unwrap_or("");
                let copies: Vec<String> = (0..n)
                    .map(|i| repeat.replace("{i}", &i.to_string()))
                    .collect();
                out.push_str(&copies.join(separator));
            }
            other => return Err(format!("invalid payload segment {other}")),
        }
    }
    Ok(out)
}

/// Runs one case.
pub fn run_case(case: &Case) -> Outcome {
    let run = Run { case };
    match case.action.as_str() {
        "compile" => run.compile(),
        "decompile" => run.decompile(),
        "unwrap" => run.unwrap(),
        "has_format_content" => run.has_format_content(),
        "wrap" => run.wrap(),
        "parse_response" => run.parse_response(),
        "parse_chunk" => run.parse_chunk(),
        "generate_prompt_snippet" => run.generate_prompt_snippet(),
        "measure_growth" => run.measure_growth(),
        other => Err(format!("unknown action {other:?}")),
    }
}
