//! The identifiers of A2UI v1.0: component ids, catalog names (components,
//! properties, functions, arguments) and extension keys.
//!
//! The v1.0 spec defines them as `^[\p{XID_Start}_][\p{XID_Continue}]*$`
//! (UAX #31), and each implementation reads the sets of its own Unicode
//! version: upstream's TS SDK the JS engine's, its Python SDK
//! `str.isidentifier`, eui a pinned table. This module reads eui's version
//! ([`UNICODE_VERSION`]).
//!
//! These are not the identifiers Express lexes. The lexer
//! ([`is_identifier`](super::is_identifier)) has the older sets compiled into
//! upstream's ANTLR lexer, and must keep them to lex what upstream lexes.
//! Unicode only ever adds to the two sets, so every name Express lexes is a
//! v1.0 identifier, and a few v1.0 identifiers are not Express names: the
//! reserved words (`true`, `false`, `null`, `_`) and the names with a code
//! point Unicode added since, such as ZWJ and ZWNJ (U+200D, U+200C;
//! `XID_Continue` since Unicode 15.1). The decompiler writes a component
//! with such an id under a derived variable name and an `id="…"` argument.

use super::lexer::{in_ranges, is_id_continue, is_id_start};
use super::uax31_tables::{CONTINUE_ADDED, START_ADDED};

pub use super::uax31_tables::UNICODE_VERSION;

/// `[\p{XID_Start}_]` of [`UNICODE_VERSION`].
pub fn is_xid_start(c: char) -> bool {
    is_id_start(c) || in_ranges(c, START_ADDED)
}

/// `[\p{XID_Continue}]` of [`UNICODE_VERSION`].
pub fn is_xid_continue(c: char) -> bool {
    is_id_continue(c) || in_ranges(c, CONTINUE_ADDED)
}

/// Whether `s` is a v1.0 identifier: `^[\p{XID_Start}_][\p{XID_Continue}]*$`
/// over code points, without normalization. Reserved words of Express are
/// identifiers; the empty string is not.
pub fn is_uax31_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if is_xid_start(c)) && chars.all(is_xid_continue)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::is_identifier;

    #[test]
    fn reads_a_newer_unicode_than_the_lexer() {
        // ZWJ and ZWNJ: XID_Continue since Unicode 15.1.
        for id in ["a\u{200d}", "a\u{200c}b"] {
            assert!(is_uax31_identifier(id), "{id:?}");
            assert!(!is_identifier(id), "the lexer's tables changed: {id:?}");
        }
        assert!(!is_uax31_identifier("\u{200d}a"));
        // Garay (Unicode 16) and Sidetic (Unicode 17) letters.
        assert!(is_uax31_identifier("\u{10d4a}") && is_uax31_identifier("\u{10940}"));
    }

    #[test]
    fn reserved_words_are_identifiers() {
        for id in ["true", "false", "null", "_", "class"] {
            assert!(is_uax31_identifier(id), "{id}");
        }
        for id in ["", "1st", "a-b", "a b", "@index", "\u{1f600}"] {
            assert!(!is_uax31_identifier(id), "{id:?}");
        }
    }

    #[test]
    fn added_ranges_are_sorted_and_outside_the_lexers() {
        for (added, lexed) in [
            (START_ADDED, is_id_start as fn(char) -> bool),
            (CONTINUE_ADDED, is_id_continue),
        ] {
            assert!(added.windows(2).all(|w| w[0].1 < w[1].0));
            for &(lo, hi) in added {
                assert!(lo <= hi);
                for c in (lo..=hi).filter_map(char::from_u32) {
                    assert!(!lexed(c), "U+{:04X} is in both tables", c as u32);
                }
            }
        }
    }
}
