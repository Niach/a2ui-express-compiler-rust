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

use a2ui_express_ffi::{self as ffi, FfiError, ResponseOptions};
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
export type Catalogs = CatalogDocument | CatalogDocument[];
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
  /** The protocol version, `"1.0"` or `"0.9"` (default: the catalogs'). */
  version?: string;
}
export interface DecompileOptions {
  /** Write arguments as `name=value` instead of positionally. */
  keywordArgs?: boolean;
  /** `"upstream"` (message order, default) or `"top-down"` (`root` first, for streaming). */
  order?: "upstream" | "top-down";
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
  /** The protocol version, `"1.0"` or `"0.9"` (default: the catalogs'). */
  version?: string;
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
#[wasm_bindgen(unchecked_return_type = "A2uiMessage[]")]
pub fn compile(
    #[wasm_bindgen(unchecked_param_type = "Catalogs")] catalogs: JsValue,
    source: &str,
    #[wasm_bindgen(unchecked_param_type = "CompileOptions")] options: Option<JsValue>,
) -> JsResult<JsValue> {
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
#[wasm_bindgen]
pub fn decompile(
    #[wasm_bindgen(unchecked_param_type = "Catalogs")] catalogs: JsValue,
    #[wasm_bindgen(unchecked_param_type = "A2uiMessage[]")] messages: JsValue,
    #[wasm_bindgen(unchecked_param_type = "DecompileOptions")] options: Option<JsValue>,
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
#[wasm_bindgen(js_name = parseResponse, unchecked_return_type = "Part[]")]
pub fn parse_response(
    #[wasm_bindgen(unchecked_param_type = "Catalogs")] catalogs: JsValue,
    text: &str,
    #[wasm_bindgen(unchecked_param_type = "ResponseOptions")] options: Option<JsValue>,
) -> JsResult<JsValue> {
    let options = ok(ResponseOptions::from_json(&opt(options)?))?;
    let parts = ok(ffi::parse_response(from_js(&catalogs)?, text, &options))?;
    Ok(to_js(&parts))
}

/// Splits a response into text and raw block parts.
#[wasm_bindgen(unchecked_return_type = "RawPart[]")]
pub fn unwrap(text: &str) -> JsValue {
    to_js(&ffi::unwrap(text))
}

/// Writes raw parts back out as a model would have emitted them.
#[wasm_bindgen]
pub fn wrap(
    #[wasm_bindgen(unchecked_param_type = "RawPart[]")] parts: JsValue,
) -> JsResult<String> {
    ok(ffi::wrap(&from_js(&parts)?))
}

/// Whether a response holds an `<a2ui>` block (`complete`: a closed one).
#[wasm_bindgen(js_name = hasFormatContent)]
pub fn has_format_content(text: &str, complete: Option<bool>) -> bool {
    ffi::has_format_content(text, complete.unwrap_or(false))
}

fn opt(options: Option<JsValue>) -> JsResult<Value> {
    options.as_ref().map_or(Ok(Value::Null), from_js)
}

/// Reads a model response as it streams in: each `push` returns the parts
/// that chunk made ready, so a surface renders while the model types.
#[wasm_bindgen]
pub struct StreamCompiler {
    inner: ffi::Stream,
}

#[wasm_bindgen]
impl StreamCompiler {
    /// Opens a stream against a new renderer state.
    #[wasm_bindgen(constructor)]
    pub fn new(
        #[wasm_bindgen(unchecked_param_type = "Catalogs")] catalogs: JsValue,
        #[wasm_bindgen(unchecked_param_type = "ResponseOptions")] options: Option<JsValue>,
    ) -> JsResult<StreamCompiler> {
        let options = ok(ResponseOptions::from_json(&opt(options)?))?;
        Ok(StreamCompiler {
            inner: ok(ffi::Stream::new(from_js(&catalogs)?, &options))?,
        })
    }

    /// Feeds a chunk; returns the parts it made ready.
    #[wasm_bindgen(unchecked_return_type = "Part[]")]
    pub fn push(&mut self, chunk: &str) -> JsResult<JsValue> {
        Ok(to_js(&ok(self.inner.push(chunk))?))
    }

    /// Ends the response; returns what was still held back.
    #[wasm_bindgen(unchecked_return_type = "Part[]")]
    pub fn finish(&mut self) -> JsResult<JsValue> {
        Ok(to_js(&ok(self.inner.finish())?))
    }

    /// Whether the stream is inside an `<a2ui>` block.
    #[wasm_bindgen(getter = inBlock)]
    pub fn in_block(&self) -> bool {
        self.inner.in_block()
    }
}
