//! The seam between the conformance harness and the crate.
//!
//! Every call the suites make goes through one function here, with JSON in and
//! JSON out, so the harness never depends on the crate's internal types. Each
//! function returns `unimplemented` until the piece of the crate it needs
//! exists; wiring one up is how a case turns green.
//!
//! The calls mirror the upstream `Parser`/`InferenceFormat` interface the
//! Python and TypeScript harnesses drive:
//!
//! | suite action              | function here                              |
//! |---------------------------|--------------------------------------------|
//! | `compile`                 | [`compile_checked`]                        |
//! | `decompile`               | [`decompile`] (+ [`compile_checked`])      |
//! | `unwrap`                  | [`unwrap`] (+ [`has_format_content`])      |
//! | `has_format_content`      | [`has_format_content`]                     |
//! | `wrap`                    | [`wrap`] (+ [`unwrap`])                    |
//! | `parse_response`          | [`parse_response`]                         |
//! | `parse_chunk`             | [`open_stream`] (+ [`parse_response`])     |
//! | `generate_prompt_snippet` | [`generate_prompt`], [`agent_callable_functions`] |
//! | `measure_growth`          | [`parse_response`] with `wrapped: false`   |

#![allow(dead_code)]

use a2ui_express::compile::{Category, CompileError, Compiler};
use a2ui_express::schema::{self, CatalogError};
use serde_json::Value;

/// A catalog a case registers, as the harness resolved it.
///
/// `document` is the catalog JSON with the defaults upstream's Python harness
/// applies: `protocolVersion` is `"1.0"` and `catalogId` the file's basename
/// (or `"inline"`) when the document leaves them out. In a `v0.9-lowered` run
/// it is the lowered document.
#[derive(Debug, Clone)]
pub struct Catalog {
    pub document: Value,
    /// Applied in order to the catalog before use.
    pub transformers: Vec<Transformer>,
}

#[derive(Debug, Clone)]
pub enum Transformer {
    /// Keep only the named components.
    ComponentPruning(Vec<String>),
    /// Keep only the named functions.
    FunctionPruning(Vec<String>),
}

/// What a parser is built from.
#[derive(Debug, Clone)]
pub struct ParserConfig {
    pub catalogs: Vec<Catalog>,
    /// Always `default_surface` in the suites.
    pub surface_id: String,
    /// `args.progressive_keys` (streaming); `None` means the format's default.
    pub progressive_keys: Option<Vec<String>>,
}

/// What a prompt snippet is generated from.
#[derive(Debug, Clone)]
pub struct PromptConfig {
    pub catalogs: Vec<Catalog>,
    pub surface_id: String,
    /// Prompt examples, one list of messages per turn, in the order given.
    pub examples: Vec<Vec<Value>>,
    /// `args.allowed_messages`.
    pub allowed_messages: Option<Vec<String>>,
}

/// A part of a response before its payloads are compiled.
#[derive(Debug, Clone, PartialEq)]
pub enum RawPart {
    Text { text: String, is_final: bool },
    A2ui { raw: String, is_final: bool },
}

/// A part of a parsed response.
#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    Text(String),
    /// The messages one block compiled to.
    A2ui(Vec<Value>),
    /// A block that failed, with the suite's error category.
    Error {
        category: String,
    },
}

/// An error, reduced to what the suites check.
#[derive(Debug, Clone)]
pub struct SdkError {
    /// The suite's category: `ParseError`, `ValidationError`, `CatalogError`,
    /// `CompilationError`, …; `Unimplemented` for a call not wired up yet.
    pub category: String,
    pub message: String,
    /// Parts a stream had produced before it failed (`expect_partial_results`).
    pub partial_results: Vec<Part>,
}

impl SdkError {
    pub fn new(category: &str, message: impl Into<String>) -> Self {
        Self {
            category: category.to_owned(),
            message: message.into(),
            partial_results: Vec::new(),
        }
    }

    pub fn unimplemented(what: &str) -> Self {
        Self::new("Unimplemented", format!("{what} is not implemented yet"))
    }
}

impl From<a2ui_express::Error> for SdkError {
    fn from(error: a2ui_express::Error) -> Self {
        let message = error.to_string();
        match error {
            a2ui_express::Error::Parse(_) => Self::new("ParseError", message),
            a2ui_express::Error::Validation(_) => Self::new("ValidationError", message),
        }
    }
}

impl From<CompileError> for SdkError {
    fn from(error: CompileError) -> Self {
        let category = match error.category() {
            Category::Parse => "ParseError",
            Category::Validation => "ValidationError",
        };
        Self::new(category, error.message)
    }
}

impl From<CatalogError> for SdkError {
    fn from(error: CatalogError) -> Self {
        Self::new("CatalogError", error.to_string())
    }
}

pub type SdkResult<T> = Result<T, SdkError>;

/// A response stream (`Parser.openStream`).
pub trait Stream {
    /// Feeds one chunk and returns the parts it yields since the last call.
    fn feed(&mut self, chunk: &str) -> SdkResult<Vec<Part>>;
    /// Ends the response and returns what was still held back.
    fn close(&mut self) -> SdkResult<Vec<Part>>;
    /// Whether the stream is inside an `<a2ui>` block.
    fn is_in_block(&self) -> bool;
}

/// What a stream does after a block is rejected (`args.after_rejected_block`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AfterRejectedBlock {
    End,
    Continue,
}

/// Compiles one block (no sentinel tags) and validates the messages it emits
/// against their envelope schema, the way upstream's `compile_checked` does.
/// A surface the block updates without creating is assumed to exist.
pub fn compile_checked(parser: &ParserConfig, payload: &str) -> SdkResult<Vec<Value>> {
    let compiler = compiler(&parser.catalogs)?;
    let messages = compiler.compile(payload, &parser.surface_id)?;
    validate_payload(&compiler, &messages)?;
    Ok(messages)
}

/// The part of upstream's `validate_payload` the compiler suite relies on,
/// beyond what the compiler checks itself:
///
/// - a `callRendererFunction` names a function the agent may call
///   (`allowedCallers` `agentOnly` or `rendererOrAgent`, no user activation);
/// - from v1.0, every `@call` in a component names a declared function.
///
/// Full schema validation of the messages is not ported.
fn validate_payload(compiler: &Compiler, messages: &[Value]) -> SdkResult<()> {
    let invalid = |m: String| SdkError::new("ValidationError", m);
    let set = compiler.catalogs();
    for message in messages {
        if let Some(body) = message.get("callRendererFunction") {
            let subject = format!(
                "Function call '{}'",
                body.get("functionCallId")
                    .and_then(Value::as_str)
                    .unwrap_or("")
            );
            let call = &body["callFunction"];
            let name = call.get("@call").and_then(Value::as_str).unwrap_or("");
            let catalog_id = call.get("catalogId").and_then(Value::as_str).unwrap_or("");
            let Some(catalog) = set.get(catalog_id) else {
                return Err(invalid(format!(
                    "{subject} names catalog '{catalog_id}', which isn't one of the active catalogs."
                )));
            };
            let Some(function) = catalog.function(name).filter(|f| !f.is_system()) else {
                return Err(invalid(format!(
                    "{subject} names '{name}', which catalog '{catalog_id}' does not declare."
                )));
            };
            let callers = function.allowed_callers();
            if !callers.admits_agent() {
                return Err(invalid(format!(
                    "{subject} names '{name}', which only the renderer may call (allowedCallers is {}).",
                    callers.as_str()
                )));
            }
            if function.requires_user_activation() {
                return Err(invalid(format!(
                    "{subject} names '{name}', which requires a user activation, and a call from the agent has none."
                )));
            }
        }
        if !compiler.version().at_least_v1() {
            continue;
        }
        for key in ["createSurface", "updateComponents"] {
            let components = message
                .get(key)
                .and_then(|b| b.get("components"))
                .and_then(Value::as_array);
            for component in components.into_iter().flatten() {
                let Some(map) = component.as_object() else {
                    continue;
                };
                for (k, v) in map {
                    if k != "id" && k != "component" {
                        check_calls(compiler, v)?;
                    }
                }
            }
        }
    }
    Ok(())
}

/// Every `{"@call": name, …}` in a value names a declared function.
fn check_calls(compiler: &Compiler, value: &Value) -> SdkResult<()> {
    match value {
        Value::Array(items) => items.iter().try_for_each(|v| check_calls(compiler, v)),
        Value::Object(map) => {
            if map.contains_key("@path") {
                return Ok(());
            }
            if let Some(Value::String(name)) = map.get("@call") {
                let declared = compiler
                    .catalogs()
                    .catalogs()
                    .iter()
                    .any(|c| c.function(name).is_some());
                if !declared {
                    return Err(SdkError::new(
                        "ValidationError",
                        format!("Unrecognized function '{name}'"),
                    ));
                }
            }
            map.values().try_for_each(|v| check_calls(compiler, v))
        }
        _ => Ok(()),
    }
}

/// The catalog a case registers, with its transformers applied.
pub fn load_catalog(catalog: &Catalog) -> SdkResult<schema::Catalog> {
    let mut document = catalog.document.clone();
    for transformer in &catalog.transformers {
        let (key, keep) = match transformer {
            Transformer::ComponentPruning(names) => ("components", names),
            Transformer::FunctionPruning(names) => ("functions", names),
        };
        match document.get_mut(key) {
            Some(Value::Object(map)) => map.retain(|name, _| keep.contains(name)),
            Some(Value::Array(list)) => list.retain(|f| {
                f.get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|n| keep.iter().any(|k| k == n))
            }),
            _ => {}
        }
    }
    schema::Catalog::from_json(document).map_err(SdkError::from)
}

/// A compiler for the catalogs a case registers.
pub fn compiler(catalogs: &[Catalog]) -> SdkResult<Compiler> {
    let catalogs = catalogs
        .iter()
        .map(load_catalog)
        .collect::<SdkResult<Vec<_>>>()?;
    Compiler::new(catalogs, None).map_err(SdkError::from)
}

/// Writes messages back into Express notation.
pub fn decompile(parser: &ParserConfig, messages: &[Value]) -> SdkResult<String> {
    use a2ui_express::decompile::{DecompileOptions, decompile};
    let catalogs = decompile_catalogs(&parser.catalogs)?;
    Ok(decompile(
        &catalogs,
        messages,
        &DecompileOptions::default(),
    )?)
}

/// The active catalogs of a decompile case, with their transformers applied.
fn decompile_catalogs(catalogs: &[Catalog]) -> SdkResult<a2ui_express::schema::CatalogSet> {
    use a2ui_express::schema::{Catalog as SchemaCatalog, CatalogSet};
    let catalog_error =
        |e: a2ui_express::schema::CatalogError| SdkError::new("CatalogError", e.to_string());
    let loaded = catalogs
        .iter()
        .map(|c| {
            let mut document = c.document.clone();
            for transformer in &c.transformers {
                let (key, keep) = match transformer {
                    Transformer::ComponentPruning(keep) => ("components", keep),
                    Transformer::FunctionPruning(keep) => ("functions", keep),
                };
                if let Some(Value::Object(defs)) = document.get_mut(key) {
                    defs.retain(|name, _| keep.contains(name));
                }
            }
            SchemaCatalog::from_json(document).map_err(catalog_error)
        })
        .collect::<SdkResult<Vec<_>>>()?;
    CatalogSet::new(loaded).map_err(catalog_error)
}

/// Splits a response into text and raw payload parts.
pub fn unwrap(_parser: &ParserConfig, _response: &str) -> SdkResult<Vec<RawPart>> {
    Err(SdkError::unimplemented("unwrap"))
}

/// Whether a response holds a payload (`complete`: a closed one).
pub fn has_format_content(
    _parser: &ParserConfig,
    _response: &str,
    _complete: bool,
) -> SdkResult<bool> {
    Err(SdkError::unimplemented("has_format_content"))
}

/// Writes raw parts back out as a model would have emitted them.
pub fn wrap(_parser: &ParserConfig, _parts: &[RawPart]) -> SdkResult<String> {
    Err(SdkError::unimplemented("wrap"))
}

/// Unwraps a response and compiles each block (`wrapped: false`: the whole
/// response is one block).
pub fn parse_response(
    _parser: &ParserConfig,
    _response: &str,
    _wrapped: bool,
) -> SdkResult<Vec<Part>> {
    Err(SdkError::unimplemented("parse_response"))
}

/// Opens a stream on a new parser.
pub fn open_stream(
    _parser: &ParserConfig,
    _wrapped: bool,
    _after_rejected_block: AfterRejectedBlock,
) -> SdkResult<Box<dyn Stream>> {
    Err(SdkError::unimplemented("open_stream"))
}

/// Renders the Express prompt snippet for the active catalogs.
pub fn generate_prompt(_config: &PromptConfig) -> SdkResult<String> {
    Err(SdkError::unimplemented("generate_prompt"))
}

/// The functions an agent may call standalone, across the catalogs.
pub fn agent_callable_functions(_catalogs: &[Catalog]) -> SdkResult<Vec<String>> {
    Err(SdkError::unimplemented("agent_callable_functions"))
}
