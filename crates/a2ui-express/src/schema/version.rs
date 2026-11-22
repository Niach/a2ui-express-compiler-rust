//! The catalog protocol versions the schema helper reads.

use crate::ProtocolVersion;

/// The protocol version a catalog document is read with.
///
/// v0.9 and v0.9.1 share one release line and one set of common types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CatalogVersion {
    /// A2UI v0.9.
    V0_9,
    /// A2UI v0.9.1, read with the v0.9 common types.
    V0_9_1,
    /// A2UI v1.0.
    V1_0,
}

impl CatalogVersion {
    /// The version a catalog document that declares no `protocolVersion` has.
    ///
    /// Catalogs only began carrying `protocolVersion` at v1.0.
    pub const DEFAULT: CatalogVersion = CatalogVersion::V0_9;

    /// Parses a version string such as `1.0`, `v0.9`, `v0.9.1`, `0.9.0` or `v1_0`.
    ///
    /// Returns `None` for a version this crate does not read (v0.8, v2.0, ...).
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
        if parts.next().is_some() {
            return None;
        }
        match (major, minor, patch) {
            (0, 9, 0) => Some(CatalogVersion::V0_9),
            (0, 9, 1) => Some(CatalogVersion::V0_9_1),
            (1, 0, 0) => Some(CatalogVersion::V1_0),
            _ => None,
        }
    }

    /// The wire string of the version: `v0.9`, `v0.9.1` or `1.0`.
    pub fn wire(self) -> &'static str {
        match self {
            CatalogVersion::V0_9 => "v0.9",
            CatalogVersion::V0_9_1 => "v0.9.1",
            CatalogVersion::V1_0 => "1.0",
        }
    }

    /// The release line of the version, which is what messages are emitted for.
    pub fn line(self) -> ProtocolVersion {
        match self {
            CatalogVersion::V0_9 | CatalogVersion::V0_9_1 => ProtocolVersion::V0_9,
            CatalogVersion::V1_0 => ProtocolVersion::V1_0,
        }
    }

    /// Whether the version is v1.0 or later, where envelope keys carry `@`.
    pub fn at_least_v1(self) -> bool {
        self.line() == ProtocolVersion::V1_0
    }
}

impl From<ProtocolVersion> for CatalogVersion {
    fn from(v: ProtocolVersion) -> Self {
        match v {
            ProtocolVersion::V0_9 => CatalogVersion::V0_9,
            ProtocolVersion::V1_0 => CatalogVersion::V1_0,
        }
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
    fn parses_the_spellings_of_each_version() {
        for raw in ["0.9", "v0.9", "V0.9", "0.9.0", "v0_9"] {
            assert_eq!(
                CatalogVersion::parse(raw),
                Some(CatalogVersion::V0_9),
                "{raw}"
            );
        }
        for raw in ["0.9.1", "v0.9.1"] {
            assert_eq!(
                CatalogVersion::parse(raw),
                Some(CatalogVersion::V0_9_1),
                "{raw}"
            );
        }
        for raw in ["1.0", "v1.0", "1", "1.0.0", "1.0.0-beta"] {
            assert_eq!(
                CatalogVersion::parse(raw),
                Some(CatalogVersion::V1_0),
                "{raw}"
            );
        }
        for raw in ["0.8", "v0.8", "2.0", "", "x", "1.0.0.0"] {
            assert_eq!(CatalogVersion::parse(raw), None, "{raw}");
        }
    }

    #[test]
    fn v0_9_1_is_on_the_v0_9_line() {
        assert_eq!(CatalogVersion::V0_9_1.line(), ProtocolVersion::V0_9);
        assert!(!CatalogVersion::V0_9_1.at_least_v1());
        assert!(CatalogVersion::V1_0.at_least_v1());
    }
}
