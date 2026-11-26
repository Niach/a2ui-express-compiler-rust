//! A2UI Express compiler.
//!
//! A Rust port of the Express inference format from Google's A2UI agent SDK.
//! Express is a compact notation an agent writes instead of A2UI JSON; this
//! crate compiles it to A2UI v1.0 messages, emitted as
//! [`serde_json::Value`].
//!
//! - [`compile`]: one block to messages.
//! - [`response`]: model responses (prose with `<a2ui>` blocks), whole or
//!   streamed chunk by chunk, so a surface renders while the model types.
//! - [`state`]: the mirror of the surfaces a renderer holds, which streamed
//!   and parsed messages are checked against.
//! - [`decompile`]: messages back to Express.
//! - [`prompt`]: the system-prompt section that teaches a model Express.
//!
//! The behaviour is pinned by the upstream language-neutral conformance suite
//! vendored under `vendor/a2ui/conformance/agent/express/`.

pub mod compile;
pub mod decompile;
pub mod ext;
pub mod prompt;
pub mod response;
pub mod schema;
pub mod state;
pub mod syntax;

use thiserror::Error;

/// A2UI messages are emitted as plain JSON values (insertion-ordered).
pub use serde_json::Value;

/// Errors produced by the Express toolchain.
///
/// The categories mirror the conformance suite: `ParseError` for payload text
/// the format cannot read, `ValidationError` for a payload that reads but
/// names something the catalog does not declare.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum Error {
    /// The payload text is not valid Express.
    #[error("parse error: {0}")]
    Parse(String),
    /// The payload parses but does not match the catalog.
    #[error("validation error: {0}")]
    Validation(String),
}

/// Convenience alias for results returned by this crate.
pub type Result<T, E = Error> = std::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_display_their_category() {
        assert_eq!(Error::Parse("x".into()).to_string(), "parse error: x");
        assert_eq!(
            Error::Validation("y".into()).to_string(),
            "validation error: y"
        );
    }
}
