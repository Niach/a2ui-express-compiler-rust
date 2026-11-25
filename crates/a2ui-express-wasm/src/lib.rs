//! WebAssembly bindings for the A2UI Express compiler.
//!
//! Built with `wasm-pack build --target web` (or `cargo build --target
//! wasm32-unknown-unknown` + `wasm-bindgen --target web`); packaged as
//! `@niach/a2ui-express` from `packages/npm`. Every call goes through
//! `a2ui-express-ffi`, which the Python binding shares, so both accept and
//! return the same JSON.
//!
//! Values cross the boundary as JSON (`JSON.stringify` / `JSON.parse`), so
//! messages are plain objects with their keys in emission order.
//!
//! Features (both default): `decompile` and `prompt` (`generatePrompt`,
//! `agentCallableFunctions`). Without them (`--no-default-features`) the
//! module is the renderer build, ~48 KB smaller gzipped. The TypeScript
//! declarations are written out below (the generated ones cannot say an
//! options argument is optional).

#[cfg(feature = "prompt")]
use a2ui_express_ffi::PromptOptions;
use a2ui_express_ffi::{self as ffi, FfiError, ResponseOptions};
use std::rc::Rc;

use js_sys::{JSON, Reflect};
use serde_json::Value;
use wasm_bindgen::prelude::*;

#[cfg(all(not(target_feature = "atomics"), target_family = "wasm"))]
#[global_allocator]
static TALC: talc::wasm::WasmDynamicTalc = talc::wasm::new_wasm_dynamic_allocator();

#[wasm_bindgen(typescript_custom_section)]
const TS_TYPES: &str = r#"
/** A catalog document (A2UI catalog JSON). */
export type CatalogDocument = Record<string, unknown>;
/** One catalog document, or a list of them. */
export type CatalogDocuments = CatalogDocument | CatalogDocument[];
/** An A2UI message, as compiled (`createSurface`, `updateComponents`, …). */
export type A2uiMessage = Record<string, unknown>;
/** The error categories: the suite's two, plus catalog and argument errors. */
export type ErrorCategory = "ParseError" | "ValidationError" | "CatalogError" | "ArgumentError";
/** A part of a parsed response. */
export type Part =
  | { text: string }
  | { a2ui: A2uiMessage[] }
  | { error: { category: ErrorCategory; message: string } };
/** A part of a response before its blocks are compiled. */
export type RawPart = { text: string } | { a2ui_raw: string; is_final?: boolean };
/** What every call throws: an `Error` whose `name` is its category. */
export interface ExpressError extends Error {
  category: ErrorCategory;
  /** Parts made ready before the failure (response reads only; else empty). */
  partial: Part[];
}
export interface CompileOptions {
  /** The surface a block without `surface(...)` targets (default `default_surface`). */
  surfaceId?: string;
  /** The protocol version, only `"1.0"` (the default). */
  version?: "1.0";
}
export interface ResponseOptions {
  /** The surface a block without `surface(...)` targets (default `default_surface`). */
  surfaceId?: string;
  /** `false` reads the whole response as one block without `<a2ui>` tags (default `true`). */
  wrapped?: boolean;
  /** Keys whose open strings heal while streaming (default `["text"]`; `[]` turns it off). */
  progressiveKeys?: string[];
  /** After a rejected block: `"end"` throws (default), `"continue"` yields an error part and reads on. */
  afterRejectedBlock?: "end" | "continue";
  /** The protocol version, only `"1.0"` (the default). */
  version?: "1.0";
}

/** Compiles one Express block (`<a2ui>` tags optional) to A2UI messages. */
export function compile(catalogs: CatalogDocuments, source: string, options?: CompileOptions): A2uiMessage[];
/** Reads a whole response (prose with `<a2ui>` blocks) and compiles each block. */
export function parseResponse(catalogs: CatalogDocuments, text: string, options?: ResponseOptions): Part[];
/** Splits a response into text and raw block parts. */
export function unwrap(text: string): RawPart[];
/** Writes raw parts back out as a model would have emitted them. */
export function wrap(parts: RawPart[]): string;
/** Whether a response holds an `<a2ui>` block (`complete`: a closed one). */
export function hasFormatContent(text: string, complete?: boolean): boolean;

export interface CatalogsOptions {
  /** The protocol version, only `"1.0"` (the default). */
  version?: "1.0";
  /**
   * The surface id streams and responses mostly use (default `default_surface`):
   * its parser is set up with the catalogs. Another surface id's is set up on
   * first use, which costs about what loading the catalogs does.
   */
  surfaceId?: string;
}

/**
 * Loaded catalogs: a catalog is read once, then compiled against any number of
 * times. Prefer it to the free functions for a large catalog (loading eui's
 * catalog takes ~70 ms; compiling a block against it, about a millisecond per
 * statement). Options' `version`, if given, must be the one loaded.
 */
export class Catalogs {
  constructor(catalogs: CatalogDocuments, options?: CatalogsOptions);
  /** The protocol version messages are compiled for (`"1.0"`). */
  readonly version: string;
  /** The catalog ids, in order. */
  readonly ids: string[];
  compile(source: string, options?: CompileOptions): A2uiMessage[];
  parseResponse(text: string, options?: ResponseOptions): Part[];
  /** Opens a `StreamCompiler` on these catalogs. */
  stream(options?: ResponseOptions): StreamCompiler;
  free(): void;
  [Symbol.dispose](): void;
}

/**
 * Reads a model response as it streams in: each `push` returns the parts that
 * chunk made ready, so a surface renders while the model types. Streamed in
 * any chunk size, a response yields the same surfaces as `parseResponse`.
 */
export class StreamCompiler {
  constructor(catalogs: CatalogDocuments, options?: ResponseOptions);
  /** Feeds a chunk; returns the parts it made ready. Throws an `ExpressError`. */
  push(chunk: string): Part[];
  /** Ends the response; returns what was still held back. Throws an `ExpressError`. */
  finish(): Part[];
  /** Whether the stream is inside an `<a2ui>` block. */
  readonly inBlock: boolean;
  /** Frees the wasm memory now (else the finalizer does). */
  free(): void;
  [Symbol.dispose](): void;
}
"#;

#[cfg(feature = "decompile")]
#[wasm_bindgen(typescript_custom_section)]
const TS_DECOMPILE: &str = r#"
export interface DecompileOptions {
  /** Write arguments as `name=value` instead of positionally. */
  keywordArgs?: boolean;
  /** `"upstream"` (message order, default) or `"top-down"` (`root` first, for streaming). */
  order?: "upstream" | "top-down";
}
/** Writes A2UI messages back as one Express block (without tags). */
export function decompile(catalogs: CatalogDocuments, messages: A2uiMessage[], options?: DecompileOptions): string;
export interface Catalogs {
  /** Writes A2UI messages back as one Express block (without tags). */
  decompile(messages: A2uiMessage[], options?: DecompileOptions): string;
}
"#;

#[cfg(feature = "prompt")]
#[wasm_bindgen(typescript_custom_section)]
const TS_PROMPT: &str = r#"
export interface PromptOptions {
  /** The surface the agent writes to (default `default_surface`). */
  surfaceId?: string;
  /** Examples: one list of A2UI messages per turn, rewritten into Express in the prompt. */
  examples?: A2uiMessage[][];
  /** The message types the agent may write (`createSurface`, `updateComponents`, …); default all. */
  allowedMessages?: string[];
  /**
   * How the signatures are written: `"upstream"` (default, upstream's format byte for byte)
   * or `"compact"` (shared types and common properties listed once; far shorter on large catalogs).
   */
  style?: "upstream" | "compact";
}
/** Generates the system-prompt section that teaches a model Express for the catalogs. */
export function generatePrompt(catalogs: CatalogDocuments, options?: PromptOptions): string;
/** The functions an agent may call standalone (`callRendererFunction`), across the catalogs. */
export function agentCallableFunctions(catalogs: CatalogDocuments): string[];
export interface Catalogs {
  /** The system-prompt section that teaches a model Express for these catalogs. */
  generatePrompt(options?: PromptOptions): string;
  /** The functions an agent may call standalone, across these catalogs. */
  agentCallableFunctions(): string[];
}
"#;

fn from_js(value: &JsValue) -> Result<Value, JsValue> {
    if value.is_undefined() || value.is_null() {
        return Ok(Value::Null);
    }
    let text: String = JSON::stringify(value)?.into();
    serde_json::from_str(&text)
        .map_err(|e| throw(FfiError::new(ffi::Category::Argument, e.to_string())))
}

fn to_js(value: &[Value]) -> JsValue {
    // Serializing a `Value` cannot fail, and its text is valid JSON.
    let text = serde_json::to_string(value).unwrap_or_else(|_| "null".to_owned());
    JSON::parse(&text).unwrap_or(JsValue::NULL)
}

/// An `Error` named after its category, with `category` and `partial`.
fn throw(error: FfiError) -> JsValue {
    let js = js_sys::Error::new(&error.message);
    js.set_name(error.category.as_str());
    let obj: &JsValue = js.as_ref();
    let _ = Reflect::set(obj, &"category".into(), &error.category.as_str().into());
    let _ = Reflect::set(obj, &"partial".into(), &to_js(&error.partial));
    js.into()
}

type JsResult<T> = Result<T, JsValue>;

fn ok<T>(result: ffi::Result<T>) -> JsResult<T> {
    result.map_err(throw)
}

/// Compiles one Express block (sentinel tags optional) to A2UI messages.
#[wasm_bindgen(skip_typescript)]
pub fn compile(catalogs: JsValue, source: &str, options: Option<JsValue>) -> JsResult<JsValue> {
    let (surface_id, version) = ok(ffi::compile_options(&opt(options)?))?;
    let messages = ok(ffi::compile(
        from_js(&catalogs)?,
        source,
        surface_id.as_deref(),
        version.as_deref(),
    ))?;
    Ok(to_js(&messages))
}

/// Writes A2UI messages back as one Express block (without tags).
#[cfg(feature = "decompile")]
#[wasm_bindgen(skip_typescript)]
pub fn decompile(
    catalogs: JsValue,
    messages: JsValue,
    options: Option<JsValue>,
) -> JsResult<String> {
    let (keyword_args, order) = ok(ffi::decompile_options(&opt(options)?))?;
    ok(ffi::decompile(
        from_js(&catalogs)?,
        from_js(&messages)?,
        keyword_args,
        order.as_deref(),
    ))
}

/// Reads a whole response (prose with `<a2ui>` blocks) and compiles each
/// block.
#[wasm_bindgen(js_name = parseResponse, skip_typescript)]
pub fn parse_response(
    catalogs: JsValue,
    text: &str,
    options: Option<JsValue>,
) -> JsResult<JsValue> {
    let options = ok(ResponseOptions::from_json(&opt(options)?))?;
    let parts = ok(ffi::parse_response(from_js(&catalogs)?, text, &options))?;
    Ok(to_js(&parts))
}

/// Splits a response into text and raw block parts.
#[wasm_bindgen(skip_typescript)]
pub fn unwrap(text: &str) -> JsValue {
    to_js(&ffi::unwrap(text))
}

/// Writes raw parts back out as a model would have emitted them.
#[wasm_bindgen(skip_typescript)]
pub fn wrap(parts: JsValue) -> JsResult<String> {
    ok(ffi::wrap(&from_js(&parts)?))
}

/// Whether a response holds an `<a2ui>` block (`complete`: a closed one).
#[wasm_bindgen(js_name = hasFormatContent, skip_typescript)]
pub fn has_format_content(text: &str, complete: Option<bool>) -> bool {
    ffi::has_format_content(text, complete.unwrap_or(false))
}

/// Generates the system-prompt section that teaches a model Express for
/// the catalogs.
#[cfg(feature = "prompt")]
#[wasm_bindgen(js_name = generatePrompt, skip_typescript)]
pub fn generate_prompt(catalogs: JsValue, options: Option<JsValue>) -> JsResult<String> {
    let options = ok(PromptOptions::from_json(&opt(options)?))?;
    ok(ffi::generate_prompt(from_js(&catalogs)?, &options))
}

/// The functions an agent may call standalone, across the catalogs.
#[cfg(feature = "prompt")]
#[wasm_bindgen(js_name = agentCallableFunctions, skip_typescript)]
pub fn agent_callable_functions(catalogs: JsValue) -> JsResult<JsValue> {
    let names = ok(ffi::agent_callable_functions(from_js(&catalogs)?))?;
    let names: Vec<Value> = names.into_iter().map(Value::String).collect();
    Ok(to_js(&names))
}

fn opt(options: Option<JsValue>) -> JsResult<Value> {
    options.as_ref().map_or(Ok(Value::Null), from_js)
}

/// Loaded catalogs: a catalog is read once, then compiled against any
/// number of times. Prefer it to the free functions when the catalog is
/// large (loading eui's catalog costs ~70 ms; a compile against it, ~1 ms
/// a statement).
#[wasm_bindgen(skip_typescript)]
pub struct Catalogs {
    inner: Rc<ffi::Catalogs>,
}

#[wasm_bindgen]
impl Catalogs {
    /// Loads catalog documents (one, or a list).
    #[wasm_bindgen(constructor)]
    pub fn new(catalogs: JsValue, options: Option<JsValue>) -> JsResult<Catalogs> {
        let options = opt(options)?;
        let map = ok(ffi::options_map(
            &options,
            &["version", "surfaceId"],
            "catalogs",
        ))?;
        let get = |key| map.and_then(|m| m.get(key)).and_then(Value::as_str);
        let loaded =
            ffi::Catalogs::with_surface(from_js(&catalogs)?, get("version"), get("surfaceId"));
        Ok(Catalogs {
            inner: Rc::new(ok(loaded)?),
        })
    }

    /// The protocol version messages are compiled for.
    #[wasm_bindgen(getter)]
    pub fn version(&self) -> String {
        self.inner.version().to_owned()
    }

    /// The catalog ids, in order.
    #[wasm_bindgen(getter)]
    pub fn ids(&self) -> Vec<String> {
        self.inner.ids()
    }

    /// Compiles one Express block.
    pub fn compile(&self, source: &str, options: Option<JsValue>) -> JsResult<JsValue> {
        let (surface_id, version) = ok(ffi::compile_options(&opt(options)?))?;
        ok(self.inner.check_version(version.as_deref()))?;
        Ok(to_js(&ok(self
            .inner
            .compile(source, surface_id.as_deref()))?))
    }

    /// Reads a whole response and compiles each block.
    #[wasm_bindgen(js_name = parseResponse)]
    pub fn parse_response(&self, text: &str, options: Option<JsValue>) -> JsResult<JsValue> {
        let options = ok(ResponseOptions::from_json(&opt(options)?))?;
        Ok(to_js(&ok(self.inner.parse_response(text, &options))?))
    }

    /// Opens a stream.
    pub fn stream(&self, options: Option<JsValue>) -> JsResult<StreamCompiler> {
        let options = ok(ResponseOptions::from_json(&opt(options)?))?;
        Ok(StreamCompiler {
            inner: ok(self.inner.stream(&options))?,
        })
    }

    /// Writes A2UI messages back as one Express block.
    #[cfg(feature = "decompile")]
    pub fn decompile(&self, messages: JsValue, options: Option<JsValue>) -> JsResult<String> {
        let (keyword_args, order) = ok(ffi::decompile_options(&opt(options)?))?;
        ok(self
            .inner
            .decompile(from_js(&messages)?, keyword_args, order.as_deref()))
    }

    /// The system-prompt section for these catalogs.
    #[cfg(feature = "prompt")]
    #[wasm_bindgen(js_name = generatePrompt)]
    pub fn generate_prompt(&self, options: Option<JsValue>) -> JsResult<String> {
        let options = ok(PromptOptions::from_json(&opt(options)?))?;
        ok(self.inner.generate_prompt(&options))
    }

    /// The functions an agent may call standalone.
    #[cfg(feature = "prompt")]
    #[wasm_bindgen(js_name = agentCallableFunctions)]
    pub fn agent_callable_functions(&self) -> Vec<String> {
        self.inner.agent_callable_functions()
    }
}

/// Reads a model response as it streams in: each `push` returns the parts
/// that chunk made ready, so a surface renders while the model types.
#[wasm_bindgen(skip_typescript)]
pub struct StreamCompiler {
    inner: ffi::Stream,
}

#[wasm_bindgen]
impl StreamCompiler {
    /// Opens a stream against a new renderer state.
    #[wasm_bindgen(constructor)]
    pub fn new(catalogs: JsValue, options: Option<JsValue>) -> JsResult<StreamCompiler> {
        let options = ok(ResponseOptions::from_json(&opt(options)?))?;
        Ok(StreamCompiler {
            inner: ok(ffi::Stream::new(from_js(&catalogs)?, &options))?,
        })
    }

    /// Feeds a chunk; returns the parts it made ready.
    #[wasm_bindgen(skip_typescript)]
    pub fn push(&mut self, chunk: &str) -> JsResult<JsValue> {
        Ok(to_js(&ok(self.inner.push(chunk))?))
    }

    /// Ends the response; returns what was still held back.
    #[wasm_bindgen(skip_typescript)]
    pub fn finish(&mut self) -> JsResult<JsValue> {
        Ok(to_js(&ok(self.inner.finish())?))
    }

    /// Whether the stream is inside an `<a2ui>` block.
    #[wasm_bindgen(getter = inBlock)]
    pub fn in_block(&self) -> bool {
        self.inner.in_block()
    }
}
