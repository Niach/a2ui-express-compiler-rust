//! The catalog protocol version the schema helper reads.

use serde_json::{Map, Value};

use crate::Error;

/// The protocol version a catalog document is read with, and messages are
/// emitted for: A2UI v1.0 only.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CatalogVersion {
    /// A2UI v1.0.
    #[default]
    V1_0,
}

impl CatalogVersion {
    /// Parses a version string: `1.0` (or `v1.0`, `1.0.0`, `1.0.0-beta`, …).
    ///
    /// Returns `None` for a version this crate does not read (v0.9, v2.0, ...).
    pub fn parse(raw: &str) -> Option<CatalogVersion> {
        let text = raw.trim();
        let text = text
            .strip_prefix('v')
            .or_else(|| text.strip_prefix('V'))
            .unwrap_or(text);
        let core = text
            .split(['-', '+'])
            .next()
            .unwrap_or("")
            .replace('_', ".");
        let mut parts = core.split('.');
        let major = parts.next()?.parse::<u64>().ok()?;
        let minor = parts.next().map_or(Some(0), |p| p.parse::<u64>().ok())?;
        let patch = parts.next().map_or(Some(0), |p| p.parse::<u64>().ok())?;
        match (major, minor, patch, parts.next()) {
            (1, 0, 0, None) => Some(CatalogVersion::V1_0),
            _ => None,
        }
    }

    /// The wire string of the version: `1.0`.
    pub fn wire(self) -> &'static str {
        match self {
            CatalogVersion::V1_0 => "1.0",
        }
    }
}

/// Refuses a message that does not state `"version": "1.0"`.
pub(crate) fn check_message_version(message: &Map<String, Value>) -> Result<(), Error> {
    let wire = CatalogVersion::V1_0.wire();
    match message.get("version") {
        Some(Value::String(v)) if v == wire => Ok(()),
        stated => Err(Error::Validation(format!(
            "A message states version {}, expected \"{wire}\".",
            stated.map_or("none".to_owned(), Value::to_string)
        ))),
    }
}

impl std::fmt::Display for CatalogVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.wire())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_v1_0_only() {
        for raw in ["1.0", "v1.0", "1", "1.0.0", "1.0.0-beta", "v1_0"] {
            assert_eq!(
                CatalogVersion::parse(raw),
                Some(CatalogVersion::V1_0),
                "{raw}"
            );
        }
        for raw in [
            "0.8", "0.9", "v0.9", "0.9.1", "v0_9", "2.0", "", "x", "1.0.0.0",
        ] {
            assert_eq!(CatalogVersion::parse(raw), None, "{raw}");
        }
    }
}
