//! Scanning a streamed block: where its close tag is, which statements are
//! complete, and healing an open string so a sentence types in live.
//!
//! Ports of the module-level helpers of upstream's `response_stream.py`.

use std::collections::HashSet;

use super::lexer::{close_tag_at, is_info, is_space};
use crate::schema::CatalogSet;

const CLOSE_TAG: &[u8] = b"</a2ui>";

/// Whether `s[i..]` is the start of `tag`, cut off by the end of `s`.
fn is_tag_prefix(s: &[u8], i: usize, tag: &[u8]) -> bool {
    let rest = &s[i..];
    rest.len() < tag.len() && tag[..rest.len()].eq_ignore_ascii_case(rest)
}

/// `</a2ui\s*$`: a close tag still arriving.
fn close_tag_start(s: &[u8], i: usize) -> bool {
    s.len() >= i + 6
        && &s[i..i + 2] == b"</"
        && s[i + 2..i + 6].eq_ignore_ascii_case(b"a2ui")
        && s[i + 6..].iter().all(|b| is_space(*b))
}

/// `<a2ui\s[^>]*$`: an open tag with attributes still arriving.
fn open_tag_start(s: &[u8], i: usize) -> bool {
    s.len() > i + 5
        && s[i] == b'<'
        && s[i + 1..i + 5].eq_ignore_ascii_case(b"a2ui")
        && is_space(s[i + 5])
        && !s[i + 6..].contains(&b'>')
}

/// Where a tail of `text` could start `<a2ui>`, else its length.
pub(crate) fn possible_open_tag_start(text: &str) -> usize {
    let s = text.as_bytes();
    let mut i = 0;
    while let Some(p) = s[i..].iter().position(|&c| c == b'<') {
        let at = i + p;
        if is_tag_prefix(s, at, b"<a2ui>") || open_tag_start(s, at) {
            return at;
        }
        i = at + 1;
    }
    s.len()
}

/// The close tag at `i`: `Some(Some(i))` for a whole one, `Some(None)` for
/// one still arriving.
fn check_close_tag_at(s: &[u8], i: usize) -> Option<Option<usize>> {
    if close_tag_at(s, i, s.len()).is_some() {
        return Some(Some(i));
    }
    if is_tag_prefix(s, i, CLOSE_TAG) || close_tag_start(s, i) {
        return Some(None);
    }
    None
}

/// The end of the string literal opening at `start` (a `"`), or `None`
/// when it is still open. A raw string has no escapes. A bare `""` at the
/// very end is open: a third quote may make it a triple-quoted string.
fn skip_string(s: &[u8], start: usize, raw: bool) -> Option<usize> {
    let triple = s[start..].starts_with(b"\"\"\"");
    let delim: &[u8] = if triple { b"\"\"\"" } else { b"\"" };
    let mut i = start + delim.len();
    while i < s.len() {
        if !raw && s[i] == b'\\' {
            if i + 1 >= s.len() {
                return None;
            }
            i += 2;
            continue;
        }
        if s[i..].starts_with(delim) {
            let end = i + delim.len();
            if !triple && end == s.len() && end - start == 2 {
                return None;
            }
            return Some(end);
        }
        i += 1;
    }
    None
}

fn word_char_before(text: &str, i: usize) -> bool {
    text[..i]
        .chars()
        .next_back()
        .is_some_and(|c| c.is_alphanumeric() || c == '_')
}

/// `_skip_block_literal_or_comment`: past a string or comment at `i`, and
/// whether it is complete.
fn skip_literal_or_comment(text: &str, i: usize) -> (usize, bool) {
    let s = text.as_bytes();
    let n = s.len();
    if matches!(s[i], b'r' | b'R') && s.get(i + 1) == Some(&b'"') && !word_char_before(text, i) {
        return match skip_string(s, i + 1, true) {
            Some(e) => (e, true),
            None => (n, false),
        };
    }
    if s[i] == b'"' {
        return match skip_string(s, i, false) {
            Some(e) => (e, true),
            None => (n, false),
        };
    }
    if s[i..].starts_with(b"/*") {
        return match find(s, i + 2, b"*/") {
            Some(e) => (e + 2, true),
            None => (n, false),
        };
    }
    if s[i] == b'#' || s[i..].starts_with(b"//") || s[i..].starts_with(b"--") {
        let mut j = i + if s[i] == b'#' { 1 } else { 2 };
        while j < n && !matches!(s[j], b'\n' | b'\r') {
            if s[j] == b'<' && check_close_tag_at(s, j).is_some() {
                return (j, true);
            }
            j += 1;
        }
        return if j < n { (j + 1, true) } else { (n, false) };
    }
    (i, true)
}

fn find(s: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    s.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

/// `_scan_express_block_from`: scans a block from `start` for its close tag
/// outside strings and comments. Returns `(close_start, readable_end,
/// resume)`: where the close tag starts, if it arrived; how much of the
/// block can be read (up to a close tag still arriving); and where the next
/// scan resumes.
pub(crate) fn scan_block_from(text: &str, start: usize) -> (Option<usize>, usize, usize) {
    let s = text.as_bytes();
    let n = s.len();
    let mut i = start;
    while i < n {
        let (next, complete) = skip_literal_or_comment(text, i);
        if next > i {
            if !complete {
                return (None, n, i);
            }
            i = next;
            continue;
        }
        if s[i] == b'<' {
            match check_close_tag_at(s, i) {
                Some(Some(at)) => return (Some(at), at, i),
                Some(None) => return (None, i, i),
                None => {}
            }
        }
        i += 1;
    }
    let resume = if n > 0 && matches!(s[n - 1], b'r' | b'R' | b'/' | b'-') {
        n - 1
    } else {
        n
    };
    (None, n, resume)
}

/// `clean_express_payload`: `block[..end]` without the markdown fences at
/// its edges, or `None` while a leading fence is still arriving.
pub(crate) fn clean_payload(block: &str, end: usize) -> Option<&str> {
    let text = &block[..end];
    let stripped = text.trim_start();
    if !stripped.is_empty() && stripped.len() <= 3 && "```".starts_with(stripped) {
        return None;
    }
    if stripped.starts_with("```") && !stripped.contains('\n') {
        return None;
    }
    let b = text.as_bytes();
    // `^\s*(?:```[a-zA-Z0-9_-]*[ \t]*\r?\n|`+\s*)`
    let mut start = 0;
    let mut i = 0;
    while i < b.len() && is_space(b[i]) {
        i += 1;
    }
    if b[i..].starts_with(b"```") {
        let mut j = i + 3;
        while j < b.len() && is_info(b[j]) {
            j += 1;
        }
        while j < b.len() && matches!(b[j], b' ' | b'\t') {
            j += 1;
        }
        if b.get(j) == Some(&b'\r') {
            j += 1;
        }
        if b.get(j) == Some(&b'\n') {
            start = j + 1;
        }
    }
    if start == 0 && b.get(i) == Some(&b'`') {
        let mut j = i;
        while j < b.len() && b[j] == b'`' {
            j += 1;
        }
        while j < b.len() && is_space(b[j]) {
            j += 1;
        }
        start = j;
    }
    let cleaned = &text[start..];
    Some(strip_trailing_fence(cleaned))
}

/// `(?:\r?\n?[ \t]*```[a-zA-Z0-9_-]*|`+)\s*$`, removed.
fn strip_trailing_fence(text: &str) -> &str {
    let b = text.as_bytes();
    // Only the trailing run of whitespace, backticks and info characters can
    // hold the match.
    let mut lo = b.len();
    while lo > 0 && (is_space(b[lo - 1]) || b[lo - 1] == b'`' || is_info(b[lo - 1])) {
        lo -= 1;
    }
    let ws_to_end = |j: usize| b[j..].iter().all(|c| is_space(*c));
    for i in lo..b.len() {
        // `\r?\n?[ \t]*```[a-zA-Z0-9_-]*\s*$`
        let mut j = i;
        if b.get(j) == Some(&b'\r') {
            j += 1;
        }
        if b.get(j) == Some(&b'\n') {
            j += 1;
        }
        while matches!(b.get(j), Some(b' ' | b'\t')) {
            j += 1;
        }
        if b[j..].starts_with(b"```") {
            let mut k = j + 3;
            while k < b.len() && is_info(b[k]) {
                k += 1;
            }
            if ws_to_end(k) {
                return &text[..i];
            }
        }
        // `` `+\s*$ ``
        if b[i] == b'`' {
            let mut k = i;
            while k < b.len() && b[k] == b'`' {
                k += 1;
            }
            if ws_to_end(k) {
                return &text[..i];
            }
        }
    }
    text
}

// ---- complete statements ----------------------------------------------------

#[derive(Default)]
struct Cursor {
    depth: i32,
    has_token: bool,
    saw_equals: bool,
    has_rhs: bool,
    rhs_closed: bool,
    last_complete: usize,
}

impl Cursor {
    fn reset(&mut self, pos: usize) {
        self.has_token = false;
        self.saw_equals = false;
        self.has_rhs = false;
        self.rhs_closed = false;
        self.last_complete = pos;
    }
}

/// `_scan_double_quoted`: the end of a string, or `None` when open.
fn scan_quoted(s: &[u8], quote: usize, raw: bool) -> Option<usize> {
    let triple = s[quote..].starts_with(b"\"\"\"");
    let delim: &[u8] = if triple { b"\"\"\"" } else { b"\"" };
    let mut i = quote + delim.len();
    while i < s.len() {
        if !raw && s[i] == b'\\' {
            if i + 1 >= s.len() {
                return None;
            }
            i += 2;
            continue;
        }
        if s[i..].starts_with(delim) {
            return Some(i + delim.len());
        }
        i += 1;
    }
    None
}

/// `_step_string_or_comment`: past a string or comment at `i`, and whether
/// a string closed there.
fn step_string_or_comment(s: &[u8], i: usize, hex_colors: bool) -> (usize, bool) {
    let n = s.len();
    if hex_colors && crate::syntax::lexer::hex_color_at(s, i).is_some() {
        return (i, false);
    }
    let quoted = |q: usize, raw: bool| match scan_quoted(s, q, raw) {
        Some(end) if !(end == n && end - q == 2) => (end, true),
        _ => (n, false),
    };
    if matches!(s[i], b'r' | b'R') && s.get(i + 1) == Some(&b'"') {
        return quoted(i + 1, true);
    }
    if s[i] == b'"' {
        return quoted(i, false);
    }
    if s[i..].starts_with(b"/*") {
        return match find(s, i + 2, b"*/") {
            Some(c) => (c + 2, false),
            None => (n, false),
        };
    }
    if s[i] == b'#' || s[i..].starts_with(b"//") || s[i..].starts_with(b"--") {
        return match s[i..].iter().position(|&c| c == b'\n') {
            Some(p) => (i + p, false),
            None => (n, false),
        };
    }
    (i, false)
}

/// Splits `source` into the statements that are complete and the fragment
/// after them. A statement is complete once its brackets close and a line
/// break or `;` follows its value. With `hex_colors` (the extension's
/// dialect) a `#rrggbb[aa]` colour is part of its statement, not a comment.
pub(crate) fn split_complete_statements(source: &str, hex_colors: bool) -> (&str, &str) {
    let s = source.as_bytes();
    let n = s.len();
    let mut cur = Cursor::default();
    let mut i = 0;
    while i < n {
        let (next, closed_string) = step_string_or_comment(s, i, hex_colors);
        if next > i {
            if closed_string && cur.depth == 0 && cur.saw_equals {
                cur.has_rhs = true;
                cur.rhs_closed = true;
                cur.last_complete = next;
            }
            i = next;
            continue;
        }
        let c = s[i];
        match c {
            b'(' | b'[' | b'{' => {
                cur.depth += 1;
                cur.has_token = true;
                if cur.saw_equals {
                    cur.has_rhs = true;
                    cur.rhs_closed = false;
                }
            }
            b')' | b']' | b'}' => {
                if cur.depth > 0 {
                    cur.depth -= 1;
                    if cur.depth == 0 && (cur.saw_equals || cur.has_token) {
                        cur.has_rhs = true;
                        cur.rhs_closed = true;
                        cur.last_complete = i + 1;
                    }
                }
            }
            b'\n' | b';' if cur.depth == 0 => {
                if cur.rhs_closed || (cur.saw_equals && cur.has_rhs) {
                    cur.reset(i + 1);
                } else if !cur.has_token {
                    cur.last_complete = i + 1;
                }
            }
            _ if c.is_ascii_whitespace() => {}
            _ => {
                if cur.depth == 0 && c == b'=' && s.get(i + 1) != Some(&b'=') {
                    cur.saw_equals = true;
                    cur.has_token = true;
                } else {
                    cur.has_token = true;
                    if cur.depth == 0 && cur.saw_equals {
                        cur.has_rhs = true;
                    }
                }
            }
        }
        i += 1;
    }
    source.split_at(char_boundary(source, cur.last_complete))
}

fn char_boundary(s: &str, mut i: usize) -> usize {
    while !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

// ---- progressive healing ----------------------------------------------------

struct Frame {
    name: String,
    arg_index: usize,
    kwarg: Option<String>,
    last_ident: Option<String>,
}

struct Open {
    content_start: usize,
    delim: &'static str,
    raw: bool,
    brackets: Vec<u8>,
    calls: Vec<Frame>,
}

/// The string literal `fragment` ends inside, if any.
fn unclosed_string(fragment: &str) -> Option<Open> {
    let s = fragment.as_bytes();
    let n = s.len();
    let mut brackets: Vec<u8> = Vec::new();
    let mut calls: Vec<Frame> = Vec::new();
    let mut last_ident: Option<String> = None;
    let mut i = 0;
    while i < n {
        let raw = matches!(s[i], b'r' | b'R') && s.get(i + 1) == Some(&b'"');
        if raw || s[i] == b'"' {
            let q = if raw { i + 1 } else { i };
            let triple = s[q..].starts_with(b"\"\"\"");
            match scan_quoted(s, q, raw) {
                Some(end) => {
                    i = end;
                    continue;
                }
                None => {
                    let delim = if triple { "\"\"\"" } else { "\"" };
                    return Some(Open {
                        content_start: q + delim.len(),
                        delim,
                        raw,
                        brackets,
                        calls,
                    });
                }
            }
        }
        if s[i..].starts_with(b"/*") {
            i = find(s, i + 2, b"*/")? + 2;
            continue;
        }
        if s[i] == b'#' || s[i..].starts_with(b"//") || s[i..].starts_with(b"--") {
            i += s[i..].iter().position(|&c| c == b'\n')? + 1;
            continue;
        }
        if s[i].is_ascii_alphabetic() || s[i] == b'_' {
            let mut j = i + 1;
            while j < n && (s[j].is_ascii_alphanumeric() || s[j] == b'_') {
                j += 1;
            }
            let ident = fragment[i..j].to_owned();
            if brackets.last() == Some(&b'(') {
                if let Some(f) = calls.last_mut() {
                    f.last_ident = Some(ident.clone());
                }
            }
            last_ident = Some(ident);
            i = j;
            continue;
        }
        let c = s[i];
        let in_call = !calls.is_empty() && brackets.last() == Some(&b'(');
        match c {
            b'(' => {
                brackets.push(b'(');
                calls.push(Frame {
                    name: last_ident.take().unwrap_or_default(),
                    arg_index: 0,
                    kwarg: None,
                    last_ident: None,
                });
            }
            b'[' | b'{' => {
                brackets.push(c);
                last_ident = None;
            }
            b')' | b']' | b'}' => {
                if brackets.pop() == Some(b'(') {
                    calls.pop();
                }
                last_ident = None;
            }
            b',' if in_call => {
                let f = calls.last_mut().expect("in a call");
                f.arg_index += 1;
                f.kwarg = None;
                f.last_ident = None;
            }
            b'=' if in_call && calls.last().is_some_and(|f| f.last_ident.is_some()) => {
                let f = calls.last_mut().expect("in a call");
                f.kwarg = f.last_ident.clone();
            }
            _ if !c.is_ascii_whitespace() => last_ident = None,
            _ => {}
        }
        i += 1;
    }
    None
}

/// Whether the open string sits where a progressive key goes: an argument
/// of a component whose property is one of `keys`, or a data path whose
/// last segment is.
fn is_progressive_slot(
    fragment: &str,
    calls: &[Frame],
    catalogs: &CatalogSet,
    keys: &HashSet<String>,
) -> bool {
    let Some(frame) = calls.last() else {
        // `^\s*\$(?P<path>[a-zA-Z0-9_/]*)\s*=(?!=)`
        let t = fragment.trim_start();
        let Some(rest) = t.strip_prefix('$') else {
            return false;
        };
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '/'))
            .unwrap_or(rest.len());
        let (path, after) = rest.split_at(end);
        let after = after.trim_start();
        if !after.starts_with('=') || after.starts_with("==") {
            return false;
        }
        let last = path.trim_end_matches('/').rsplit('/').next().unwrap_or("");
        return keys.contains(last);
    };
    if frame.name.is_empty() {
        return false;
    }
    for cat in catalogs.catalogs() {
        let Some(component) = cat.component(&frame.name) else {
            continue;
        };
        if let Some(k) = &frame.kwarg {
            return keys.contains(k);
        }
        let check = component.check_rule_property();
        let props: Vec<&String> = component
            .properties()
            .iter()
            .filter(|p| {
                p.as_str() != "checks" && p.as_str() != "@checks" && Some(p.as_str()) != check
            })
            .collect();
        return props
            .get(frame.arg_index)
            .is_some_and(|p| keys.contains(*p));
    }
    false
}

/// Closes a string the fragment ends inside, and the brackets around it,
/// when the string is the value of a progressive key; `None` otherwise.
pub(crate) fn heal_progressive_fragment(
    fragment: &str,
    catalogs: &CatalogSet,
    keys: &HashSet<String>,
) -> Option<String> {
    if keys.is_empty() || fragment.trim().is_empty() {
        return None;
    }
    let open = unclosed_string(fragment)?;
    let content = fragment[open.content_start..]
        .trim_start()
        .to_ascii_lowercase();
    if content.starts_with("http://")
        || content.starts_with("https://")
        || content.starts_with("data:")
    {
        return None;
    }
    if !is_progressive_slot(fragment, &open.calls, catalogs, keys) {
        return None;
    }
    let mut healed = fragment.to_owned();
    if !open.raw {
        let trailing = healed.bytes().rev().take_while(|&b| b == b'\\').count();
        if trailing % 2 == 1 {
            healed.pop();
        }
    }
    if open.delim == "\"\"\"" && healed[open.content_start.min(healed.len())..].ends_with('"') {
        if open.raw {
            return None;
        }
        healed.pop();
        healed.push_str("\\\"");
    }
    healed.push_str(open.delim);
    for b in open.brackets.iter().rev() {
        healed.push(match b {
            b'[' => ']',
            b'{' => '}',
            _ => ')',
        });
    }
    Some(healed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_statements_end_at_a_line_break_after_their_value() {
        assert_eq!(
            split_complete_statements("a = Text(\"x\")\nb = Text(\"y", false),
            ("a = Text(\"x\")\n", "b = Text(\"y")
        );
        assert_eq!(
            split_complete_statements("root = Card(\n", false),
            ("", "root = Card(\n")
        );
    }

    #[test]
    fn a_close_tag_inside_a_string_is_not_one() {
        let (close, _, _) = scan_block_from("root = Text(\"</a2ui>\")\n</a2ui>", 0);
        assert_eq!(close, Some(23));
        let (close, readable, _) = scan_block_from("root = Text(\"x\")\n</a2", 0);
        assert_eq!((close, readable), (None, 17));
    }

    #[test]
    fn payload_fences_are_stripped() {
        assert_eq!(
            clean_payload("\n```express\nroot = Text(\"x\")\n```\n", 33),
            Some("root = Text(\"x\")")
        );
        assert_eq!(clean_payload("\n``", 3), None);
    }
}
