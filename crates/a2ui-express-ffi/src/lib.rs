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
//! - a raw part ([`unwrap`], [`wrap`]): `{"text": …}` or
//!   `{"a2ui_raw": …}`, with `"is_final": false` on a block whose close tag
//!   never came.

use a2ui_express::compile::{Category as CompileCategory, CompileError, Compiler};
use a2ui_express::decompile::{self as dec, DecompileOptions, Order};
use a2ui_express::response::{self, AfterRejectedBlock, ExpressParser, Part, ResponseError};
use a2ui_express::schema::{Catalog, CatalogError, CatalogSet, CatalogVersion};
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
    let compiler = compiler(catalogs, version)?;
    Ok(compiler.compile(source, surface_id.unwrap_or(DEFAULT_SURFACE_ID))?)
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
    let Value::Array(messages) = messages else {
        return Err(FfiError::argument("messages must be a list"));
    };
    let options = DecompileOptions {
        keyword_args,
        order: parse_order(order)?,
    };
    let set = CatalogSet::new(load_catalogs(catalogs)?)?;
    Ok(dec::decompile(&set, &messages, &options)?)
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

    fn parser(&self, catalogs: Value) -> Result<ExpressParser> {
        let parser = ExpressParser::new(
            compiler(catalogs, self.version.as_deref())?,
            &self.surface_id,
        );
        Ok(match &self.progressive_keys {
            Some(keys) => parser.with_progressive_keys(keys.iter().cloned()),
            None => parser,
        })
    }
}

/// Reads a whole response and compiles each block.
pub fn parse_response(
    catalogs: Value,
    content: &str,
    options: &ResponseOptions,
) -> Result<Vec<Value>> {
    let parser = options.parser(catalogs)?;
    Ok(parts_to_json(
        parser.parse_response(content, options.wrapped)?,
    ))
}

/// A response read as it streams in.
#[derive(Debug)]
pub struct Stream {
    inner: response::ResponseStream,
}

impl Stream {
    /// Opens a stream against a new renderer state.
    pub fn new(catalogs: Value, options: &ResponseOptions) -> Result<Self> {
        let parser = options.parser(catalogs)?;
        let state = parser.new_state();
        Ok(Stream {
            inner: parser.open_stream_with(options.wrapped, options.after_rejected_block, state),
        })
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
