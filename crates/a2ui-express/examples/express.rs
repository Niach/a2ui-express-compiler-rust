//! A small command line over the crate, used by `eval/prompt/`.
//!
//! ```sh
//! cargo run -q -p a2ui-express --example express -- prompt <catalog.json> [--compact]
//! cargo run -q -p a2ui-express --example express -- compile <catalog.json> < block.a2ui
//! cargo run -q -p a2ui-express --example express -- decompile <catalog.json> < messages.json
//! cargo run -q -p a2ui-express --example express -- sugar <catalog.json> < message-lists.json
//! ```
//!
//! `prompt` prints the Express prompt section for the catalog. `compile`
//! reads one Express block (a model's reply, `<a2ui>` tags allowed) from
//! stdin and prints the A2UI messages as a JSON array; on a compile error it
//! prints `{"error": …, "kind": …}` and exits with status 1.

use std::io::Read;
use std::process::ExitCode;

use a2ui_express::compile::Compiler;
use a2ui_express::decompile::{DecompileOptions, Order, decompile};
use a2ui_express::prompt::{PromptOptions, PromptStyle, generate_prompt};
use a2ui_express::schema::{Catalog, CatalogSet};

fn load(path: &str) -> Catalog {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    Catalog::from_json_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["prompt", catalog, rest @ ..] => {
            let options = PromptOptions {
                style: if rest.contains(&"--compact") {
                    PromptStyle::Compact
                } else {
                    PromptStyle::Upstream
                },
                ..Default::default()
            };
            let catalogs = CatalogSet::new(vec![load(catalog)]).expect("catalog set");
            print!("{}", generate_prompt(&catalogs, &options).expect("prompt"));
            ExitCode::SUCCESS
        }
        ["compile", catalog] => {
            let mut source = String::new();
            std::io::stdin().read_to_string(&mut source).expect("stdin");
            let compiler = Compiler::new(vec![load(catalog)], None).expect("compiler");
            match compiler.compile(&source, "main") {
                Ok(messages) => {
                    println!("{}", serde_json::to_string_pretty(&messages).unwrap());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    println!(
                        "{}",
                        serde_json::json!({"error": e.message, "kind": format!("{:?}", e.kind)})
                    );
                    ExitCode::FAILURE
                }
            }
        }
        ["decompile", catalog] => {
            let mut source = String::new();
            std::io::stdin().read_to_string(&mut source).expect("stdin");
            let messages: Vec<serde_json::Value> =
                serde_json::from_str(&source).expect("a JSON array of messages");
            let catalogs = CatalogSet::new(vec![load(catalog)]).expect("catalog set");
            let options = DecompileOptions {
                order: Order::TopDown,
                ..Default::default()
            };
            match decompile(&catalogs, &messages, &options) {
                Ok(text) => {
                    println!("{text}");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
        ["sugar", catalog] => {
            // stdin: a JSON list of message lists; stdout: a JSON list of
            // {plain, sugared} (or {error}): each decompiled without and with
            // the at_exponential_express extension.
            let mut source = String::new();
            std::io::stdin().read_to_string(&mut source).expect("stdin");
            let lists: Vec<Vec<serde_json::Value>> =
                serde_json::from_str(&source).expect("a JSON list of message lists");
            let text = std::fs::read_to_string(catalog).expect("catalog");
            let mut doc: serde_json::Value = serde_json::from_str(&text).expect("catalog JSON");
            doc["metadata"]["extensions"]
                .as_object_mut()
                .map(|m| m.remove("at_exponential_express"));
            let plain =
                CatalogSet::new(vec![Catalog::from_json(doc.clone()).expect("catalog")]).unwrap();
            doc["metadata"]["extensions"]["at_exponential_express"] = serde_json::Value::Bool(true);
            let sugared = CatalogSet::new(vec![Catalog::from_json(doc).expect("catalog")]).unwrap();
            let options = DecompileOptions {
                order: Order::TopDown,
                ..Default::default()
            };
            let out: Vec<serde_json::Value> = lists
                .iter()
                .map(|m| {
                    match (
                        decompile(&plain, m, &options),
                        decompile(&sugared, m, &options),
                    ) {
                        (Ok(p), Ok(s)) => serde_json::json!({"plain": p, "sugared": s}),
                        (Err(e), _) | (_, Err(e)) => serde_json::json!({"error": e.to_string()}),
                    }
                })
                .collect();
            println!("{}", serde_json::Value::Array(out));
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!(
                "usage: express prompt <catalog.json> [--compact] | compile <catalog.json> | decompile <catalog.json>"
            );
            ExitCode::from(2)
        }
    }
}
