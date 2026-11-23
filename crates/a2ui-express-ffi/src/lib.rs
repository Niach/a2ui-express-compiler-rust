//! The API the `a2ui-express` language bindings share, in JSON.
//!
//! The wasm (`a2ui-express-wasm`) and Python (`a2ui-express-py`) bindings
//! are thin shells over this crate: every call takes and returns
//! [`serde_json::Value`]s in one shape, and every error is an [`FfiError`]
//! with a suite category, so the two cannot drift apart in what they accept
//! or return.
//!
//! Shapes:
//!
//! - catalogs: one catalog document, or a list of them;
//! - a parsed part: `{"text": …}`, `{"a2ui": [messages]}` or
//!   `{"error": {"category", "message"}}`;
//! - prompt options: `{surfaceId?, examples?: [[messages]], allowedMessages?}`;
//! - a raw part ([`unwrap`], [`wrap`]): `{"text": …}` or
//!   `{"a2ui_raw": …}`, with `"is_final": false` on a block whose close tag
//!   never came.

use a2ui_express::compile::{Category as CompileCategory, CompileError, Compiler};
use a2ui_express::decompile::{self as dec, DecompileOptions, Order};
use a2ui_express::prompt;
use a2ui_express::response::{self, AfterRejectedBlock, ExpressParser, Part, ResponseError};
use a2ui_express::schema::{Catalog, CatalogError, CatalogSet, CatalogVersion};
use std::sync::Mutex;

use serde_json::{Map, Value, json};

/// The surface a block without a `surface(...)` line targets by default.
pub const DEFAULT_SURFACE_ID: &str = "default_surface";

/// What kind of failure an [`FfiError`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// The payload is not valid Express.
    Parse,
    /// The payload reads but does not match the catalog.
    Validation,
    /// A catalog document cannot be loaded, or catalogs not used together.
    Catalog,
    /// An argument of the wrong shape (an unknown option, a non-list).
    Argument,
}

impl Category {
    /// The category's name, as the conformance suites write it.
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Parse => "ParseError",
            Category::Validation => "ValidationError",
            Category::Catalog => "CatalogError",
            Category::Argument => "ArgumentError",
        }
    }
}

/// A failed call: its category, message, and the parts a response produced
/// before the failure (empty for calls that are not response reads).
#[derive(Debug, Clone, PartialEq)]
pub struct FfiError {
    /// What failed.
    pub category: Category,
    /// The message.
    pub message: String,
    /// Parsed parts (JSON) made ready before the failure.
    pub partial: Vec<Value>,
}

impl FfiError {
    /// An error with no partial parts.
    pub fn new(category: Category, message: impl Into<String>) -> Self {
        FfiError {
            category,
            message: message.into(),
            partial: Vec::new(),
        }
    }

    fn argument(message: impl Into<String>) -> Self {
        FfiError::new(Category::Argument, message)
    }
}

impl std::fmt::Display for FfiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for FfiError {}

impl From<a2ui_express::Error> for FfiError {
    fn from(e: a2ui_express::Error) -> Self {
        match e {
            a2ui_express::Error::Parse(m) => FfiError::new(Category::Parse, m),
            a2ui_express::Error::Validation(m) => FfiError::new(Category::Validation, m),
        }
    }
}

impl From<CompileError> for FfiError {
    fn from(e: CompileError) -> Self {
        let category = match e.category() {
            CompileCategory::Parse => Category::Parse,
            CompileCategory::Validation => Category::Validation,
        };
        FfiError::new(category, e.message)
    }
}

impl From<CatalogError> for FfiError {
    fn from(e: CatalogError) -> Self {
        FfiError::new(Category::Catalog, e.to_string())
    }
}

impl From<ResponseError> for FfiError {
    fn from(e: ResponseError) -> Self {
        let mut error = FfiError::from(e.error);
        error.partial = e.partial.into_iter().map(part_to_json).collect();
        error
    }
}

/// Convenience alias.
pub type Result<T, E = FfiError> = std::result::Result<T, E>;

/// A parsed part as JSON.
pub fn part_to_json(part: Part) -> Value {
    match part {
        Part::Text(text) => json!({ "text": text }),
        Part::A2ui(messages) => json!({ "a2ui": messages }),
        Part::Error(error) => {
            let error = FfiError::from(error);
            json!({ "error": { "category": error.category.as_str(), "message": error.message } })
        }
    }
}

fn parts_to_json(parts: Vec<Part>) -> Vec<Value> {
    parts.into_iter().map(part_to_json).collect()
}

/// Loads catalog documents: one document, or a list of them.
pub fn load_catalogs(catalogs: Value) -> Result<Vec<Catalog>> {
    let docs = match catalogs {
        Value::Array(docs) => docs,
        doc @ Value::Object(_) => vec![doc],
        _ => {
            return Err(FfiError::argument(
                "catalogs must be a catalog document or a list of them",
            ));
        }
    };
    docs.into_iter()
        .map(|d| Catalog::from_json(d).map_err(FfiError::from))
        .collect()
}

/// Parses a protocol version option (`"1.0"`, `"0.9"`, …).
pub fn parse_version(version: Option<&str>) -> Result<Option<CatalogVersion>> {
    version
        .map(|v| {
            CatalogVersion::parse(v)
                .ok_or_else(|| FfiError::argument(format!("unsupported protocol version {v:?}")))
        })
        .transpose()
}

/// A compiler for `catalogs`, at `version` (default: the catalogs').
pub fn compiler(catalogs: Value, version: Option<&str>) -> Result<Compiler> {
    let version = parse_version(version)?;
    Ok(Compiler::new(load_catalogs(catalogs)?, version)?)
}

/// Compiles one Express block (sentinel tags optional) to A2UI messages.
pub fn compile(
    catalogs: Value,
    source: &str,
    surface_id: Option<&str>,
    version: Option<&str>,
) -> Result<Vec<Value>> {
    Catalogs::new(catalogs, version)?.compile(source, surface_id)
}

/// Parses a decompile order: `"upstream"` (default) or `"top-down"`.
pub fn parse_order(order: Option<&str>) -> Result<Order> {
    match order {
        None | Some("upstream") => Ok(Order::Upstream),
        Some("top-down" | "topDown" | "top_down") => Ok(Order::TopDown),
        Some(other) => Err(FfiError::argument(format!(
            "unknown order {other:?}; expected \"upstream\" or \"top-down\""
        ))),
    }
}

/// Writes A2UI messages back as one Express block (without tags).
pub fn decompile(
    catalogs: Value,
    messages: Value,
    keyword_args: bool,
    order: Option<&str>,
) -> Result<String> {
    let set = CatalogSet::new(load_catalogs(catalogs)?)?;
    decompile_with(&set, messages, keyword_args, order)
}

fn decompile_with(
    set: &CatalogSet,
    messages: Value,
    keyword_args: bool,
    order: Option<&str>,
) -> Result<String> {
    let Value::Array(messages) = messages else {
        return Err(FfiError::argument("messages must be a list"));
    };
    let options = DecompileOptions {
        keyword_args,
        order: parse_order(order)?,
    };
    Ok(dec::decompile(set, &messages, &options)?)
}

/// What a prompt is generated from, besides the catalogs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PromptOptions {
    /// The surface the agent writes to (default `default_surface`).
    pub surface_id: Option<String>,
    /// Examples: one list of A2UI messages per turn, in order.
    pub examples: Vec<Vec<Value>>,
    /// The message types the agent may write; `None` allows all.
    pub allowed_messages: Option<Vec<String>>,
    /// How the signatures are written (default upstream's format).
    pub style: prompt::PromptStyle,
}

/// Reads a prompt style: `"upstream"` (the default) or `"compact"`.
pub fn prompt_style(style: Option<&str>) -> Result<prompt::PromptStyle> {
    match style {
        None | Some("upstream") => Ok(prompt::PromptStyle::Upstream),
        Some("compact") => Ok(prompt::PromptStyle::Compact),
        Some(other) => Err(FfiError::argument(format!(
            "unknown prompt style {other:?}; expected \"upstream\" or \"compact\""
        ))),
    }
}

fn string_list(value: &Value, what: &str) -> Result<Vec<String>> {
    let bad = || FfiError::argument(format!("{what} must be a list of strings"));
    value
        .as_array()
        .ok_or_else(bad)?
        .iter()
        .map(|v| v.as_str().map(str::to_owned).ok_or_else(bad))
        .collect()
}

/// Reads prompt examples: a list of turns, each a list of messages.
pub fn prompt_examples(value: Value) -> Result<Vec<Vec<Value>>> {
    let bad = || FfiError::argument("examples must be a list of lists of A2UI messages");
    match value {
        Value::Null => Ok(Vec::new()),
        Value::Array(turns) => turns
            .into_iter()
            .map(|t| match t {
                Value::Array(messages) => Ok(messages),
                _ => Err(bad()),
            })
            .collect(),
        _ => Err(bad()),
    }
}

impl PromptOptions {
    /// Reads `{surfaceId?, examples?, allowedMessages?, style?}`.
    pub fn from_json(options: &Value) -> Result<Self> {
        let known = ["surfaceId", "examples", "allowedMessages", "style"];
        let Some(map) = options_map(options, &known, "prompt")? else {
            return Ok(PromptOptions::default());
        };
        Ok(PromptOptions {
            surface_id: string_option(map, "surfaceId")?.map(str::to_owned),
            examples: prompt_examples(map.get("examples").cloned().unwrap_or(Value::Null))?,
            allowed_messages: match map.get("allowedMessages") {
                None | Some(Value::Null) => None,
                Some(v) => Some(string_list(v, "allowedMessages")?),
            },
            style: prompt_style(string_option(map, "style")?)?,
        })
    }
}

/// Generates the system-prompt section that teaches a model Express for
/// `catalogs`.
pub fn generate_prompt(catalogs: Value, options: &PromptOptions) -> Result<String> {
    let set = CatalogSet::new(load_catalogs(catalogs)?)?;
    generate_prompt_with(&set, options)
}

fn generate_prompt_with(set: &CatalogSet, options: &PromptOptions) -> Result<String> {
    let options = prompt::PromptOptions {
        surface_id: options
            .surface_id
            .clone()
            .unwrap_or_else(|| DEFAULT_SURFACE_ID.to_owned()),
        examples: options.examples.clone(),
        allowed_messages: options.allowed_messages.clone(),
        style: options.style,
    };
    Ok(prompt::generate_prompt(set, &options)?)
}

/// The functions an agent may call standalone, across the catalogs.
pub fn agent_callable_functions(catalogs: Value) -> Result<Vec<String>> {
    let set = CatalogSet::new(load_catalogs(catalogs)?)?;
    Ok(prompt::agent_callable_functions(&set))
}

/// Splits a response into raw text and block parts.
pub fn unwrap(content: &str) -> Vec<Value> {
    response::unwrap(content)
        .into_iter()
        .map(|p| match p {
            response::RawPart::Text(text) => json!({ "text": text }),
            response::RawPart::A2ui {
                raw,
                is_final: true,
            } => json!({ "a2ui_raw": raw }),
            response::RawPart::A2ui { raw, is_final } => {
                json!({ "a2ui_raw": raw, "is_final": is_final })
            }
        })
        .collect()
}

/// Writes raw parts back out as a model would have emitted them.
pub fn wrap(parts: &Value) -> Result<String> {
    let bad = || FfiError::argument("parts must be a list of {text} or {a2ui_raw, is_final?}");
    let parts = parts.as_array().ok_or_else(bad)?;
    let raw = parts
        .iter()
        .map(|p| {
            if let Some(text) = p.get("text").and_then(Value::as_str) {
                return Ok(response::RawPart::Text(text.to_owned()));
            }
            let raw = p.get("a2ui_raw").and_then(Value::as_str).ok_or_else(bad)?;
            let is_final = match p.get("is_final") {
                None | Some(Value::Null) => true,
                Some(v) => v.as_bool().ok_or_else(bad)?,
            };
            Ok(response::RawPart::A2ui {
                raw: raw.to_owned(),
                is_final,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(response::wrap(&raw))
}

/// Whether a response holds a block (`complete`: a closed one).
pub fn has_format_content(content: &str, complete: bool) -> bool {
    response::has_format_content(content, complete)
}

/// How a response is read: [`parse_response`] and [`Stream`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseOptions {
    /// The surface blocks without a `surface(...)` line target.
    pub surface_id: String,
    /// `false` reads the whole response as one block without tags.
    pub wrapped: bool,
    /// The keys whose open strings heal while streaming; `None` is the
    /// default (`["text"]`), empty turns healing off.
    pub progressive_keys: Option<Vec<String>>,
    /// What a stream does after a rejected block.
    pub after_rejected_block: AfterRejectedBlock,
    /// The protocol version (default: the catalogs').
    pub version: Option<String>,
}

impl Default for ResponseOptions {
    fn default() -> Self {
        ResponseOptions {
            surface_id: DEFAULT_SURFACE_ID.to_owned(),
            wrapped: true,
            progressive_keys: None,
            after_rejected_block: AfterRejectedBlock::End,
            version: None,
        }
    }
}

/// Parses an `afterRejectedBlock` option: `"end"` (default) or `"continue"`.
pub fn parse_after_rejected_block(value: Option<&str>) -> Result<AfterRejectedBlock> {
    match value {
        None | Some("end") => Ok(AfterRejectedBlock::End),
        Some("continue") => Ok(AfterRejectedBlock::Continue),
        Some(other) => Err(FfiError::argument(format!(
            "unknown afterRejectedBlock {other:?}; expected \"end\" or \"continue\""
        ))),
    }
}

fn string_option<'a>(map: &'a Map<String, Value>, key: &str) -> Result<Option<&'a str>> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(FfiError::argument(format!("{key} must be a string"))),
    }
}

fn bool_option(map: &Map<String, Value>, key: &str) -> Result<Option<bool>> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(FfiError::argument(format!("{key} must be a boolean"))),
    }
}

/// Reads an options object (JS style, camelCase keys); `null` is the
/// defaults. Unknown keys are refused, so a typo does not pass silently.
pub fn options_map<'a>(
    options: &'a Value,
    known: &[&str],
    what: &str,
) -> Result<Option<&'a Map<String, Value>>> {
    let map = match options {
        Value::Null => return Ok(None),
        Value::Object(map) => map,
        _ => {
            return Err(FfiError::argument(format!(
                "{what} options must be an object"
            )));
        }
    };
    if let Some(key) = map.keys().find(|k| !known.contains(&k.as_str())) {
        return Err(FfiError::argument(format!(
            "unknown {what} option {key:?}; expected one of {}",
            known.join(", ")
        )));
    }
    Ok(Some(map))
}

/// Compile options from JSON: `{surfaceId?, version?}`.
pub fn compile_options(options: &Value) -> Result<(Option<String>, Option<String>)> {
    let Some(map) = options_map(options, &["surfaceId", "version"], "compile")? else {
        return Ok((None, None));
    };
    Ok((
        string_option(map, "surfaceId")?.map(str::to_owned),
        string_option(map, "version")?.map(str::to_owned),
    ))
}

/// Decompile options from JSON: `{keywordArgs?, order?}`.
pub fn decompile_options(options: &Value) -> Result<(bool, Option<String>)> {
    let Some(map) = options_map(options, &["keywordArgs", "order"], "decompile")? else {
        return Ok((false, None));
    };
    Ok((
        bool_option(map, "keywordArgs")?.unwrap_or(false),
        string_option(map, "order")?.map(str::to_owned),
    ))
}

impl ResponseOptions {
    /// Reads `{surfaceId?, wrapped?, progressiveKeys?, afterRejectedBlock?,
    /// version?}`.
    pub fn from_json(options: &Value) -> Result<Self> {
        let mut out = ResponseOptions::default();
        let known = [
            "surfaceId",
            "wrapped",
            "progressiveKeys",
            "afterRejectedBlock",
            "version",
        ];
        let Some(map) = options_map(options, &known, "response")? else {
            return Ok(out);
        };
        if let Some(s) = string_option(map, "surfaceId")? {
            out.surface_id = s.to_owned();
        }
        if let Some(w) = bool_option(map, "wrapped")? {
            out.wrapped = w;
        }
        match map.get("progressiveKeys") {
            None | Some(Value::Null) => {}
            Some(Value::Array(keys)) => {
                out.progressive_keys = Some(
                    keys.iter()
                        .map(|k| {
                            k.as_str().map(str::to_owned).ok_or_else(|| {
                                FfiError::argument("progressiveKeys must be a list of strings")
                            })
                        })
                        .collect::<Result<_>>()?,
                );
            }
            Some(_) => {
                return Err(FfiError::argument(
                    "progressiveKeys must be a list of strings",
                ));
            }
        }
        out.after_rejected_block =
            parse_after_rejected_block(string_option(map, "afterRejectedBlock")?)?;
        out.version = string_option(map, "version")?.map(str::to_owned);
        Ok(out)
    }
}

/// Reads a whole response and compiles each block.
pub fn parse_response(
    catalogs: Value,
    content: &str,
    options: &ResponseOptions,
) -> Result<Vec<Value>> {
    Catalogs::new(catalogs, options.version.as_deref())?.parse_response(content, options)
}

/// Loaded catalogs: a catalog is read once and used for any number of
/// compiles, responses and streams (loading a large catalog costs far more
/// than compiling a block against it).
#[derive(Debug)]
pub struct Catalogs {
    /// The parser for the default options, which also holds the compiler.
    base: ExpressParser,
    /// Parsers by (surface id, progressive keys); usually one.
    parsers: Mutex<Vec<CachedParser>>,
}

/// A parser for one surface id and set of progressive keys.
type CachedParser = (String, Option<Vec<String>>, ExpressParser);

impl Catalogs {
    /// Loads catalog documents (one, or a list) for `version` (default:
    /// the catalogs' own).
    pub fn new(catalogs: Value, version: Option<&str>) -> Result<Self> {
        Self::with_surface(catalogs, version, None)
    }

    /// Like [`Catalogs::new`], with the parser for `surface_id` (default
    /// `default_surface`) set up front. Responses and streams for another
    /// surface id set up their own parser on first use, which copies the
    /// catalogs (about what loading them costs).
    pub fn with_surface(
        catalogs: Value,
        version: Option<&str>,
        surface_id: Option<&str>,
    ) -> Result<Self> {
        let surface_id = surface_id.unwrap_or(DEFAULT_SURFACE_ID);
        let base = ExpressParser::new(compiler(catalogs, version)?, surface_id);
        Ok(Catalogs {
            parsers: Mutex::new(vec![(surface_id.to_owned(), None, base.clone())]),
            base,
        })
    }

    /// The protocol version messages are compiled for (`"1.0"`, `"v0.9"`…).
    pub fn version(&self) -> &'static str {
        self.base.compiler().version().wire()
    }

    /// The catalog ids, in order.
    pub fn ids(&self) -> Vec<String> {
        let set = self.base.compiler().catalogs();
        set.catalogs().iter().map(|c| c.id().to_owned()).collect()
    }

    /// Compiles one Express block (sentinel tags optional).
    pub fn compile(&self, source: &str, surface_id: Option<&str>) -> Result<Vec<Value>> {
        Ok(self
            .base
            .compiler()
            .compile(source, surface_id.unwrap_or(DEFAULT_SURFACE_ID))?)
    }

    /// Writes A2UI messages back as one Express block.
    pub fn decompile(
        &self,
        messages: Value,
        keyword_args: bool,
        order: Option<&str>,
    ) -> Result<String> {
        decompile_with(
            self.base.compiler().catalogs(),
            messages,
            keyword_args,
            order,
        )
    }

    /// The system-prompt section for these catalogs.
    pub fn generate_prompt(&self, options: &PromptOptions) -> Result<String> {
        generate_prompt_with(self.base.compiler().catalogs(), options)
    }

    /// The functions an agent may call standalone.
    pub fn agent_callable_functions(&self) -> Vec<String> {
        prompt::agent_callable_functions(self.base.compiler().catalogs())
    }

    /// Reads a whole response and compiles each block.
    pub fn parse_response(&self, content: &str, options: &ResponseOptions) -> Result<Vec<Value>> {
        let parser = self.parser(options)?;
        Ok(parts_to_json(
            parser.parse_response(content, options.wrapped)?,
        ))
    }

    /// Opens a stream against a new renderer state.
    pub fn stream(&self, options: &ResponseOptions) -> Result<Stream> {
        let parser = self.parser(options)?;
        let state = parser.new_state();
        Ok(Stream {
            inner: parser.open_stream_with(options.wrapped, options.after_rejected_block, state),
        })
    }

    /// Fails unless `version` is unset or the one these catalogs compile
    /// for (a version is chosen when the catalogs are loaded).
    pub fn check_version(&self, version: Option<&str>) -> Result<()> {
        match parse_version(version)? {
            Some(v) if v != self.base.compiler().version() => Err(FfiError::argument(format!(
                "these catalogs compile for version {}; load them again for another",
                self.version()
            ))),
            _ => Ok(()),
        }
    }

    fn parser(&self, options: &ResponseOptions) -> Result<ExpressParser> {
        self.check_version(options.version.as_deref())?;
        let mut parsers = self.parsers.lock().unwrap_or_else(|e| e.into_inner());
        let cached = parsers.iter().find(|(surface, keys, _)| {
            *surface == options.surface_id && *keys == options.progressive_keys
        });
        if let Some((_, _, parser)) = cached {
            return Ok(parser.clone());
        }
        let parser = ExpressParser::new(self.base.compiler().clone(), &options.surface_id);
        let parser = match &options.progressive_keys {
            Some(keys) => parser.with_progressive_keys(keys.iter().cloned()),
            None => parser,
        };
        parsers.push((
            options.surface_id.clone(),
            options.progressive_keys.clone(),
            parser.clone(),
        ));
        Ok(parser)
    }
}

/// A response read as it streams in.
#[derive(Debug)]
pub struct Stream {
    inner: response::ResponseStream,
}

impl Stream {
    /// Opens a stream against a new renderer state.
    pub fn new(catalogs: Value, options: &ResponseOptions) -> Result<Self> {
        Catalogs::new(catalogs, options.version.as_deref())?.stream(options)
    }

    /// Feeds a chunk; returns the parts it made ready.
    pub fn push(&mut self, chunk: &str) -> Result<Vec<Value>> {
        Ok(parts_to_json(self.inner.push(chunk)?))
    }

    /// Ends the response; returns what was still held back.
    pub fn finish(&mut self) -> Result<Vec<Value>> {
        Ok(parts_to_json(self.inner.finish()?))
    }

    /// Whether the stream is inside a block.
    pub fn in_block(&self) -> bool {
        self.inner.is_in_block()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Value {
        json!({
            "catalogId": "demo",
            "protocolVersion": "1.0",
            "components": {
                "Text": {"properties": {
                    "component": {"const": "Text"},
                    "text": {"$ref": "common_types.json#/$defs/DynamicString"}
                }, "required": ["component", "text"]}
            }
        })
    }

    #[test]
    fn compiles_and_decompiles() {
        let messages = compile(catalog(), "root = Text(\"Hi\")", None, None).unwrap();
        assert_eq!(
            messages[0]["createSurface"]["surfaceId"],
            json!(DEFAULT_SURFACE_ID)
        );
        let back = decompile(catalog(), Value::Array(messages), false, Some("top-down")).unwrap();
        assert!(back.contains("root = Text(\"Hi\")"), "{back}");
    }

    #[test]
    fn errors_carry_their_category() {
        let e = compile(catalog(), "root = Nope(\"x\")", None, None).unwrap_err();
        assert_eq!(e.category, Category::Validation);
        let e = compile(catalog(), "root = Text(", None, None).unwrap_err();
        assert_eq!(e.category, Category::Parse);
        let e = compile(json!(3), "", None, None).unwrap_err();
        assert_eq!(e.category, Category::Argument);
        let e = compile(json!({}), "", None, None).unwrap_err();
        assert_eq!(e.category, Category::Catalog);
    }

    #[test]
    fn streams_parts_as_json() {
        let options = ResponseOptions::from_json(&json!({"surfaceId": "main"})).unwrap();
        let mut stream = Stream::new(catalog(), &options).unwrap();
        let mut parts = stream.push("Hello <a2ui>root = Text(\"H").unwrap();
        assert!(stream.in_block());
        parts.extend(stream.push("i\")</a2ui>").unwrap());
        parts.extend(stream.finish().unwrap());
        assert_eq!(parts[0], json!({"text": "Hello"}));
        assert!(parts[1]["a2ui"].is_array());
    }

    #[test]
    fn rejected_blocks_become_error_parts_or_errors() {
        let text = "<a2ui>root = Nope()</a2ui> after";
        let options =
            ResponseOptions::from_json(&json!({"afterRejectedBlock": "continue"})).unwrap();
        let mut stream = Stream::new(catalog(), &options).unwrap();
        let mut parts = stream.push(text).unwrap();
        parts.extend(stream.finish().unwrap());
        assert_eq!(parts[0]["error"]["category"], json!("ValidationError"));
        let e =
            parse_response(catalog(), "before <a2ui>root = Nope()</a2ui>", &options).unwrap_err();
        assert_eq!(e.category, Category::Validation);
        assert_eq!(e.partial, vec![json!({"text": "before"})]);
    }

    #[test]
    fn generates_a_prompt() {
        let options = PromptOptions::from_json(&json!({"examples": [[]]})).unwrap();
        let text = generate_prompt(catalog(), &options).unwrap();
        assert!(text.contains("Text("), "{text}");
        let e = generate_prompt(json!([]), &PromptOptions::default()).unwrap_err();
        assert_eq!(e.category, Category::Catalog);
        assert!(agent_callable_functions(catalog()).unwrap().is_empty());
    }

    #[test]
    fn loaded_catalogs_are_reused() {
        let catalogs = Catalogs::new(catalog(), None).unwrap();
        assert_eq!(catalogs.version(), "1.0");
        assert_eq!(catalogs.ids(), vec!["demo".to_owned()]);
        let options = ResponseOptions::default();
        for _ in 0..2 {
            let mut stream = catalogs.stream(&options).unwrap();
            assert!(
                !stream
                    .push("<a2ui>root = Text(\"x\")</a2ui>")
                    .unwrap()
                    .is_empty()
            );
        }
        assert_eq!(catalogs.parsers.lock().unwrap().len(), 1);
        let main = ResponseOptions {
            surface_id: "main".into(),
            ..ResponseOptions::default()
        };
        catalogs.stream(&main).unwrap();
        catalogs.stream(&main).unwrap();
        assert_eq!(catalogs.parsers.lock().unwrap().len(), 2);
        let other = ResponseOptions {
            version: Some("0.9".into()),
            ..ResponseOptions::default()
        };
        assert_eq!(
            catalogs.stream(&other).unwrap_err().category,
            Category::Argument
        );
    }

    #[test]
    fn options_refuse_unknown_keys() {
        let e = ResponseOptions::from_json(&json!({"surfaceID": "x"})).unwrap_err();
        assert_eq!(e.category, Category::Argument);
    }

    #[test]
    fn unwrap_and_wrap_round_trip() {
        let parts = unwrap("Hi\n<a2ui>\nroot = Text(\"x\")\n</a2ui>");
        assert_eq!(parts[1], json!({"a2ui_raw": "root = Text(\"x\")"}));
        let text = wrap(&Value::Array(parts.clone())).unwrap();
        assert_eq!(unwrap(&text), parts);
        assert!(has_format_content(&text, true));
    }
}
