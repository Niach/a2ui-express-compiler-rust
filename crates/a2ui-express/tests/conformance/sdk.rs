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
pub fn compile_checked(_parser: &ParserConfig, _payload: &str) -> SdkResult<Vec<Value>> {
    Err(SdkError::unimplemented("compile"))
}

/// Writes messages back into Express notation.
pub fn decompile(_parser: &ParserConfig, _messages: &[Value]) -> SdkResult<String> {
    Err(SdkError::unimplemented("decompile"))
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
