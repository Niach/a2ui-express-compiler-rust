//! A2UI Express compiler.
//!
//! A Rust port of the Express inference format from Google's A2UI agent SDK.
//! Express is a compact notation an agent writes instead of A2UI JSON; this
//! crate compiles it to A2UI v1.0 (and v0.9) messages, emitted as
//! [`serde_json::Value`].
//!
//! The behaviour is pinned by the upstream language-neutral conformance suite
//! vendored under `vendor/a2ui/conformance/agent/express/`.

pub mod compile;
pub mod decompile;
pub mod prompt;
pub mod schema;
pub mod syntax;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A2UI messages are emitted as plain JSON values (insertion-ordered).
pub use serde_json::Value;

/// The A2UI protocol version a block is compiled against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProtocolVersion {
    /// A2UI v0.9.
    #[serde(rename = "0.9")]
    V0_9,
    /// A2UI v1.0.
    #[serde(rename = "1.0")]
    V1_0,
}

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
    fn protocol_version_serializes_as_the_wire_string() {
        assert_eq!(
            serde_json::to_value(ProtocolVersion::V1_0).unwrap(),
            Value::from("1.0")
        );
        let v: ProtocolVersion = serde_json::from_value(Value::from("0.9")).unwrap();
        assert_eq!(v, ProtocolVersion::V0_9);
    }

    #[test]
    fn errors_display_their_category() {
        assert_eq!(Error::Parse("x".into()).to_string(), "parse error: x");
        assert_eq!(
            Error::Validation("y".into()).to_string(),
            "validation error: y"
        );
    }
}
