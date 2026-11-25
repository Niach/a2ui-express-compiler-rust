//! Reading Express: source repairs, lexer, parser and syntax tree.
//!
//! A hand-written port of `specification/inference_formats/express/Express.g4`
//! (no ANTLR). [`parse`] reads a block into a [`Program`]; every node carries
//! a [`Span`], and every error the line and column upstream reports, worded
//! the same way (see [`parser`] for how). A statement with an error is
//! dropped and the rest of the block still parses. [`LineParser`] feeds the
//! same parser one line at a time for the streaming compiler.

pub mod ast;
pub mod error;
pub mod lexer;
pub mod normalize;
pub mod parser;
pub mod stream;
pub mod uax31;
mod uax31_tables;
mod xid;

pub use ast::*;
pub use error::{ParseError, ParseErrorKind};
pub use lexer::is_identifier;
pub use parser::{Dialect, ParseOutput, parse, parse_at, parse_dialect};
pub use stream::{LineParser, StreamItem};
