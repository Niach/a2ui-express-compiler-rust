//! Reading model responses: prose with Express blocks between `<a2ui>` tags.
//!
//! Ports upstream's response parser and response stream (`parser/lexer.py`,
//! `express/parser.py`, `express/response_stream.py`):
//!
//! - [`unwrap`] splits a response into text and raw blocks, [`wrap`] writes
//!   them back out, and [`has_format_content`] tells whether a response
//!   holds a block.
//! - [`ExpressParser::parse_response`] reads a whole response and compiles
//!   every block, checked against a [`RendererState`].
//! - [`ExpressParser::open_stream`] reads a response as it streams in:
//!   [`ResponseStream::push`] returns the parts each chunk makes ready, so
//!   a surface renders while the model is still writing it, and
//!   [`ResponseStream::finish`] ends it. Streamed and whole, a response
//!   leaves the renderer with the same surfaces.
//!
//! ```
//! use a2ui_express::compile::Compiler;
//! use a2ui_express::response::{ExpressParser, Part};
//! use a2ui_express::schema::Catalog;
//! use serde_json::json;
//!
//! let catalog = Catalog::from_json(json!({
//!     "catalogId": "demo",
//!     "protocolVersion": "1.0",
//!     "components": {
//!         "Text": {"properties": {
//!             "component": {"const": "Text"},
//!             "text": {"$ref": "common_types.json#/$defs/DynamicString"}
//!         }, "required": ["component", "text"]},
//!         "Card": {"properties": {
//!             "component": {"const": "Card"},
//!             "child": {"$ref": "common_types.json#/$defs/ComponentId"}
//!         }, "required": ["component", "child"]}
//!     }
//! }))
//! .unwrap();
//! let parser = ExpressParser::new(Compiler::new(vec![catalog], None).unwrap(), "main");
//! let mut stream = parser.open_stream();
//! let mut parts = Vec::new();
//! for chunk in ["Here it is.\n<a2ui>\nroot = Card(body)\n", "body = Text(\"Hi", "\")\n</a2ui>"] {
//!     parts.extend(stream.push(chunk).unwrap());
//! }
//! parts.extend(stream.finish().unwrap());
//! assert_eq!(parts[0], Part::Text("Here it is.".into()));
//! // `root` renders as soon as its statement is complete…
//! assert!(matches!(&parts[1], Part::A2ui(m) if m[0].get("createSurface").is_some()));
//! // …and `body` types in: "Hi" shows before its string closes. The close
//! // tag changes nothing more, so it emits nothing.
//! assert!(matches!(&parts[2], Part::A2ui(m) if m[0]["updateComponents"]["components"][0]["text"] == "Hi"));
//! assert_eq!(parts.len(), 3);
//! ```

mod fences;
mod lexer;
mod scan;
mod stream;

use std::collections::HashSet;
use std::fmt;
use std::sync::Arc;

use serde_json::Value;

use crate::Error;
use crate::compile::{CompileError, Compiler};
use crate::schema::CatalogSet;
use crate::state::RendererState;
use crate::syntax;

pub use lexer::{has_format_content, unwrap, wrap};
pub use stream::{AfterRejectedBlock, ResponseStream};

/// The property keys whose open strings heal while streaming by default.
pub const DEFAULT_PROGRESSIVE_KEYS: &[&str] = &["text"];

/// A part of a response before its blocks are compiled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawPart {
    /// Conversational text.
    Text(String),
    /// The Express text of one block; `is_final` is false for a block still
    /// open at the end of the response.
    A2ui {
        /// The block, without its tags.
        raw: String,
        /// Whether its close tag arrived.
        is_final: bool,
    },
}

/// A part of a parsed response.
#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    /// Conversational text.
    Text(String),
    /// Messages a block compiled to: a whole block's, or what a streamed
    /// chunk made ready.
    A2ui(Vec<Value>),
    /// A block a stream rejected and read past
    /// ([`AfterRejectedBlock::Continue`]).
    Error(Error),
}

/// A failed parse or stream: the error, and the parts made ready before it
/// (applied to the state, so the caller shows them too).
#[derive(Debug, Clone, PartialEq)]
pub struct ResponseError {
    /// What failed.
    pub error: Error,
    /// The parts before the failure.
    pub partial: Vec<Part>,
}

impl ResponseError {
    fn new(error: Error, partial: Vec<Part>) -> Self {
        ResponseError { error, partial }
    }
}

impl fmt::Display for ResponseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(f)
    }
}

impl std::error::Error for ResponseError {}

#[derive(Debug)]
pub(crate) struct Inner {
    pub compiler: Compiler,
    /// The compiler's catalogs, shared with every state.
    pub catalogs: Arc<CatalogSet>,
    pub surface_id: String,
    pub progressive_keys: HashSet<String>,
}

impl Inner {
    /// Compiles one whole block; a block cut off (`!is_final`) compiles as
    /// far as its statements read.
    pub fn compile(&self, payload: &str, is_final: bool) -> Result<Vec<Value>, Error> {
        if is_final {
            return Ok(self.compiler.compile(payload, &self.surface_id)?);
        }
        let body = crate::compile::strip_sentinel_tags(payload);
        let parsed = syntax::parse(&body);
        let mut program = parsed.program;
        if let Some(line) = parsed.errors.iter().map(|e| e.line()).min() {
            program.statements.retain(|s| s.span.start.line < line);
        }
        self.compiler
            .compile_program(&program, &self.surface_id)
            .map_err(|e: CompileError| e.into())
    }
}

/// Reads Express responses for one set of catalogs.
///
/// Cheap to clone; holds no per-response state.
#[derive(Debug, Clone)]
pub struct ExpressParser {
    inner: Arc<Inner>,
}

impl ExpressParser {
    /// A parser compiling with `compiler`; blocks without a `surface(...)`
    /// line target `surface_id`.
    pub fn new(compiler: Compiler, surface_id: &str) -> Self {
        ExpressParser {
            inner: Arc::new(Inner {
                catalogs: Arc::new(compiler.catalogs().clone()),
                compiler,
                surface_id: surface_id.to_owned(),
                progressive_keys: DEFAULT_PROGRESSIVE_KEYS
                    .iter()
                    .map(|k| (*k).to_owned())
                    .collect(),
            }),
        }
    }

    /// The property keys whose open strings heal while streaming (default
    /// [`DEFAULT_PROGRESSIVE_KEYS`]; empty turns healing off).
    pub fn with_progressive_keys<I, S>(self, keys: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let inner = Inner {
            compiler: self.inner.compiler.clone(),
            catalogs: self.inner.catalogs.clone(),
            surface_id: self.inner.surface_id.clone(),
            progressive_keys: keys.into_iter().map(Into::into).collect(),
        };
        ExpressParser {
            inner: Arc::new(inner),
        }
    }

    /// The compiler.
    pub fn compiler(&self) -> &Compiler {
        &self.inner.compiler
    }

    /// The empty state of a renderer holding this parser's catalogs.
    pub fn new_state(&self) -> RendererState {
        RendererState::new(self.inner.catalogs.clone(), self.inner.compiler.version())
    }

    /// Reads a whole response, checked against a new state.
    ///
    /// `wrapped: false` reads the whole response as one block without tags.
    pub fn parse_response(&self, content: &str, wrapped: bool) -> Result<Vec<Part>, ResponseError> {
        self.parse_response_with(content, wrapped, &mut self.new_state())
    }

    /// Reads a whole response against `state`, which keeps the surfaces of
    /// earlier turns, and applies it, all or nothing.
    ///
    /// A component may name a child a later block adds: trees are checked
    /// once every block is applied. A block cut off at the end is applied as
    /// far as it reads, and its trees are not checked.
    pub fn parse_response_with(
        &self,
        content: &str,
        wrapped: bool,
        state: &mut RendererState,
    ) -> Result<Vec<Part>, ResponseError> {
        let raw = if wrapped {
            unwrap(content)
        } else {
            vec![RawPart::A2ui {
                raw: content.to_owned(),
                is_final: true,
            }]
        };
        let mark = state.begin();
        let mut parts = Vec::new();
        let mut touched: Vec<String> = Vec::new();
        let mut incomplete: HashSet<String> = HashSet::new();
        for part in raw {
            match part {
                RawPart::Text(t) => parts.push(Part::Text(t)),
                RawPart::A2ui { raw, is_final } => {
                    let applied = self
                        .inner
                        .compile(&raw, is_final)
                        .and_then(|m| state.apply(&m).map(|t| (m, t)));
                    let (messages, block_touched) = match applied {
                        Ok(r) => r,
                        Err(e) => {
                            state.rollback(mark);
                            return Err(ResponseError::new(e, parts));
                        }
                    };
                    for s in block_touched {
                        if !is_final {
                            incomplete.insert(s.clone());
                        }
                        if !touched.contains(&s) {
                            touched.push(s);
                        }
                    }
                    parts.push(Part::A2ui(messages));
                }
            }
        }
        let complete: Vec<&str> = touched
            .iter()
            .filter(|s| !incomplete.contains(*s))
            .map(String::as_str)
            .collect();
        if let Err(e) = state.check_topology(complete.iter().copied()) {
            state.rollback(mark);
            let text = parts
                .into_iter()
                .filter(|p| matches!(p, Part::Text(_)))
                .collect();
            return Err(ResponseError::new(e, text));
        }
        state.mark_checked(incomplete.iter().map(String::as_str));
        state.commit(mark);
        Ok(parts)
    }

    /// Opens a stream on a response with `<a2ui>` blocks, against a new
    /// state; it ends at a rejected block.
    pub fn open_stream(&self) -> ResponseStream {
        self.open_stream_with(true, AfterRejectedBlock::End, self.new_state())
    }

    /// Opens a stream. `wrapped: false` reads the whole response as one
    /// block without tags; `state` holds the surfaces of earlier turns.
    pub fn open_stream_with(
        &self,
        wrapped: bool,
        after: AfterRejectedBlock,
        state: RendererState,
    ) -> ResponseStream {
        ResponseStream::new(self.inner.clone(), wrapped, after, state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hosts run decoders on transport threads and hand them across FFI
    /// boundaries (UniFFI objects must be `Send + Sync`).
    #[test]
    fn parsers_streams_and_states_are_send_and_sync() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<Compiler>();
        send_sync::<ExpressParser>();
        send_sync::<ResponseStream>();
        send_sync::<RendererState>();
        send_sync::<Part>();
        send_sync::<ResponseError>();
    }
}
