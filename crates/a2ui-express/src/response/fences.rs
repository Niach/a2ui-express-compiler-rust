//! The markdown fences and inline-code backticks a model puts around tags,
//! as upstream's `response_stream.py` matches them with regular
//! expressions (`_OUTER_*_FENCE_*_RE`), written out as scanners.

use super::lexer::{is_info, is_space};

/// "```" + `[a-zA-Z0-9_-]*` + `[ \t]*` at `i`: where it ends.
fn fence(b: &[u8], i: usize) -> Option<usize> {
    if !b.get(i..)?.starts_with(b"```") {
        return None;
    }
    let mut j = i + 3;
    while j < b.len() && is_info(b[j]) {
        j += 1;
    }
    while matches!(b.get(j), Some(b' ' | b'\t')) {
        j += 1;
    }
    Some(j)
}

fn skip_ws(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && is_space(b[i]) {
        i += 1;
    }
    i
}

fn blank_to_end(b: &[u8], i: usize) -> bool {
    b[i..].iter().all(|c| matches!(c, b' ' | b'\t'))
}

/// `(?:\r?\n[ \t]*)?\Z` at `j`.
fn opt_newline_blank_end(b: &[u8], mut j: usize) -> bool {
    if j == b.len() {
        return true;
    }
    if b[j] == b'\r' {
        j += 1;
    }
    b.get(j) == Some(&b'\n') && blank_to_end(b, j + 1)
}

/// The leftmost `i` at which `matches(i)` holds, among the positions of the
/// trailing run of whitespace, backticks and info characters (no match can
/// start before it).
fn leftmost_tail(b: &[u8], matches: impl Fn(usize) -> bool) -> Option<usize> {
    let mut lo = b.len();
    while lo > 0 && (is_space(b[lo - 1]) || b[lo - 1] == b'`' || is_info(b[lo - 1])) {
        lo -= 1;
    }
    (lo..=b.len()).find(|&i| matches(i))
}

/// `(?:^|(?<=\s))`{1,2}\Z` at `i`. `^` is the start of the text run, not
/// of the chunk: `after_space` tells whether the text before `b` (if any)
/// ends in whitespace.
fn inline_backticks_at(b: &[u8], i: usize, after_space: bool) -> bool {
    let n = b.len() - i;
    (1..=2).contains(&n)
        && b[i..].iter().all(|&c| c == b'`')
        && if i == 0 {
            after_space
        } else {
            is_space(b[i - 1])
        }
}

/// `_OUTER_TRAILING_FENCE_RE` removed: a fence opening, or inline-code
/// backticks, right before an open tag.
pub(crate) fn strip_outer_trailing_fence(text: &str, after_space: bool) -> &str {
    let b = text.as_bytes();
    let alt1 = |i: usize| {
        let j = skip_ws(b, i);
        fence(b, j).is_some_and(|k| opt_newline_blank_end(b, k))
    };
    match leftmost_tail(b, |i| {
        i < b.len() && (alt1(i) || inline_backticks_at(b, i, after_space))
    }) {
        Some(i) => &text[..i],
        None => text,
    }
}

/// `_OUTER_INLINE_OPEN_BACKTICK_RE`: the text before an open tag ends in
/// inline-code backticks.
pub(crate) fn ends_in_inline_backticks(text: &str, after_space: bool) -> bool {
    let b = text.as_bytes();
    let run = b.iter().rev().take_while(|&&c| c == b'`').count();
    run > 0 && inline_backticks_at(b, b.len() - run, after_space)
}

/// `_possible_fence_start`: where a tail of `text` could be a fence (or
/// backticks) still arriving, else its length.
pub(crate) fn possible_fence_start(text: &str) -> usize {
    let b = text.as_bytes();
    let alt = |i: usize| {
        if fence(b, i).is_some_and(|k| opt_newline_blank_end(b, k)) {
            return true;
        }
        i < b.len() && b[i..].iter().all(|&c| c == b'`')
    };
    leftmost_tail(b, |i| i < b.len() && alt(i)).unwrap_or(b.len())
}

/// `^\s*```[a-zA-Z0-9_-]*[ \t]*(?:\r?\n|\Z)`: where it ends.
fn leading_fence_end(b: &[u8]) -> Option<usize> {
    let j = fence(b, skip_ws(b, 0))?;
    if j == b.len() {
        return Some(j);
    }
    let k = if b[j] == b'\r' { j + 1 } else { j };
    (b.get(k) == Some(&b'\n')).then_some(k + 1)
}

/// The fence closing a block, at the start of the text after its close tag,
/// removed. `inline` when the open tag was written as inline code, whose
/// closing backticks are removed too.
pub(crate) fn strip_outer_leading_fence(text: &str, inline: bool) -> &str {
    let b = text.as_bytes();
    if let Some(end) = leading_fence_end(b) {
        return &text[end..];
    }
    if inline {
        let run = b.iter().take_while(|&&c| c == b'`').count();
        return &text[run..];
    }
    text
}

/// Whether the text after a close tag could still be the start of the
/// fence that closes the block, so it waits for more.
pub(crate) fn is_outer_leading_fence_prefix(text: &str, inline: bool) -> bool {
    let b = text.as_bytes();
    // `\s*(?:`{0,2}|```[a-zA-Z0-9_-]*[ \t]*\r?)\Z`
    let ws = |b: &[u8]| -> bool {
        let i = skip_ws(b, 0);
        let rest = &b[i..];
        if rest.len() <= 2 && rest.iter().all(|&c| c == b'`') {
            return true;
        }
        match fence(b, i) {
            Some(k) => k == b.len() || (b[k] == b'\r' && k + 1 == b.len()),
            None => false,
        }
    };
    if inline && !b.is_empty() && b.iter().all(|&c| c == b'`') {
        return true;
    }
    ws(b)
}

/// `_TRAILING_FENCE_CLOSE_RE` (`\s*```[a-zA-Z0-9_-]*[ \t]*\r?\n?\Z`)
/// removed, from the text held at the end of a response.
pub(crate) fn strip_trailing_fence_close(text: &str) -> &str {
    let b = text.as_bytes();
    let at = |i: usize| {
        let j = skip_ws(b, i);
        fence(b, j).is_some_and(|mut k| {
            if b.get(k) == Some(&b'\r') {
                k += 1;
            }
            if b.get(k) == Some(&b'\n') {
                k += 1;
            }
            k == b.len()
        })
    };
    match leftmost_tail(b, |i| i < b.len() && at(i)) {
        Some(i) => &text[..i],
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fences_around_tags() {
        assert_eq!(
            strip_outer_trailing_fence("And UI:\n```a2ui\n", true),
            "And UI:"
        );
        assert_eq!(strip_outer_trailing_fence("See `foo`", true), "See `foo`");
        assert_eq!(strip_outer_trailing_fence("x `", true), "x ");
        assert_eq!(strip_outer_trailing_fence("`", false), "`");
        assert!(!ends_in_inline_backticks("`", false));
        assert_eq!(possible_fence_start("And UI:\n```"), 8);
        assert!(is_outer_leading_fence_prefix("\n`", false));
        assert!(!is_outer_leading_fence_prefix("\n``` x", false));
        assert_eq!(strip_outer_leading_fence("\n```\nDone.", false), "Done.");
        assert_eq!(
            strip_outer_leading_fence("`baz` done.", false),
            "`baz` done."
        );
    }
}
