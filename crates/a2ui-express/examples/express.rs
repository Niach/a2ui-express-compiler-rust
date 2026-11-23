//! A small command line over the crate, used by `eval/prompt/`.
//!
//! ```sh
//! cargo run -q -p a2ui-express --example express -- prompt <catalog.json> [--compact]
//! cargo run -q -p a2ui-express --example express -- compile <catalog.json> < block.a2ui
//! ```
//!
//! `prompt` prints the Express prompt section for the catalog. `compile`
//! reads one Express block (a model's reply, `<a2ui>` tags allowed) from
//! stdin and prints the A2UI messages as a JSON array; on a compile error it
//! prints `{"error": …, "kind": …}` and exits with status 1.

use std::io::Read;
use std::process::ExitCode;

use a2ui_express::compile::Compiler;
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
        _ => {
            eprintln!("usage: express prompt <catalog.json> [--compact] | compile <catalog.json>");
            ExitCode::from(2)
        }
    }
}
