//! Splitting a whole response into text and `<a2ui>` blocks.
//!
//! A port of upstream's `BlockLexer` (`parser/lexer.py`) configured the way
//! `ExpressParser.unwrap` configures it: strings are double-quoted only, and
//! `#`, `//`, `--` and `/* */` are comments, as `Express.g4` reads them. The
//! regular expressions upstream uses are written out as scanners, matching
//! exactly what they match.

use super::RawPart;

/// The open tag `<a2ui>` (attributes allowed) at `i`: where it ends.
pub(crate) fn open_tag_at(s: &[u8], i: usize) -> Option<usize> {
    // `<a2ui(?:\s[^>]*)?>`, case-insensitive.
    if s.get(i) != Some(&b'<') || !s.get(i + 1..i + 5)?.eq_ignore_ascii_case(b"a2ui") {
        return None;
    }
    let j = i + 5;
    match s.get(j)? {
        b'>' => Some(j + 1),
        b if is_space(*b) => s[j..].iter().position(|&c| c == b'>').map(|p| j + p + 1),
        _ => None,
    }
}

/// The first open tag at or after `from`: (start, end).
pub(crate) fn find_open_tag(s: &[u8], from: usize) -> Option<(usize, usize)> {
    let mut i = from;
    while i < s.len() {
        let p = s[i..].iter().position(|&c| c == b'<')? + i;
        if let Some(end) = open_tag_at(s, p) {
            return Some((p, end));
        }
        i = p + 1;
    }
    None
}

/// The close tag `</a2ui>` at `i`, within `s[..end]`: where it ends.
pub(crate) fn close_tag_at(s: &[u8], i: usize, end: usize) -> Option<usize> {
    // `</a2ui\s*>`, case-insensitive.
    let s = &s[..end];
    if s.get(i..i + 2)? != b"</" || !s.get(i + 2..i + 6)?.eq_ignore_ascii_case(b"a2ui") {
        return None;
    }
    let mut j = i + 6;
    while s.get(j).is_some_and(|b| is_space(*b)) {
        j += 1;
    }
    (s.get(j) == Some(&b'>')).then_some(j + 1)
}

/// Python's `\s` for ASCII bytes. Non-ASCII whitespace is rare enough in
/// tags and fences to be read as text.
pub(crate) fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

/// `[a-zA-Z0-9_-]`, the characters of a fence's info string.
pub(crate) fn is_info(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

fn skip_info(s: &[u8], mut i: usize) -> usize {
    while s.get(i).is_some_and(|b| is_info(*b)) {
        i += 1;
    }
    i
}

fn skip_blank(s: &[u8], mut i: usize) -> usize {
    while matches!(s.get(i), Some(b' ' | b'\t')) {
        i += 1;
    }
    i
}

/// `_clean_markdown`: trims, drops a leading and a trailing code fence, and
/// drops a run that is only backticks.
fn clean_markdown(text: &str) -> String {
    let mut t = text.trim();
    // `^```[a-zA-Z0-9_-]*\s*`
    if t.starts_with("```") {
        let b = t.as_bytes();
        let mut i = skip_info(b, 3);
        while b.get(i).is_some_and(|c| is_space(*c)) {
            i += 1;
        }
        t = &t[i..];
    }
    // `\s*```[a-zA-Z0-9_-]*$`
    let b = t.as_bytes();
    let mut end = b.len();
    while end > 0 && is_info(b[end - 1]) {
        end -= 1;
    }
    if end >= 3 && &b[end - 3..end] == b"```" {
        t = t[..end - 3].trim_end();
    }
    let t = t.trim();
    if t.trim_matches('`').is_empty() {
        String::new()
    } else {
        t.to_owned()
    }
}

/// `_clean_raw`: [`clean_markdown`], then backticks left at the edges.
fn clean_raw(text: &str) -> String {
    clean_markdown(text).trim_matches('`').trim().to_owned()
}

fn add_text(parts: &mut Vec<RawPart>, text: &str) {
    let cleaned = clean_markdown(text);
    if !cleaned.is_empty() {
        parts.push(RawPart::Text(cleaned));
    }
}

/// `_scan_block`: the close tag in `s[start..end]` outside strings and block
/// comments, as (start, end).
fn scan_block(s: &[u8], start: usize, end: usize) -> Option<(usize, usize)> {
    #[derive(PartialEq)]
    enum St {
        Code,
        Str { triple: bool },
        Line,
        Block,
    }
    let mut i = start;
    let mut st = St::Code;
    while i < end {
        match st {
            St::Code => {
                if let Some(e) = close_tag_at(s, i, end) {
                    return Some((i, e));
                }
                let c = s[i];
                if c == b'"' {
                    if i + 2 < end && &s[i..i + 3] == b"\"\"\"" {
                        st = St::Str { triple: true };
                        i += 3;
                    } else {
                        st = St::Str { triple: false };
                        i += 1;
                    }
                    continue;
                }
                if s[i..end].starts_with(b"/*") {
                    i += 2;
                    st = St::Block;
                    continue;
                }
                if c == b'#' {
                    i += 1;
                    st = St::Line;
                    continue;
                }
                if s[i..end].starts_with(b"//") || s[i..end].starts_with(b"--") {
                    i += 2;
                    st = St::Line;
                    continue;
                }
                i += 1;
            }
            St::Str { triple } => {
                if s[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if triple {
                    if s[i..end].starts_with(b"\"\"\"") {
                        i += 3;
                        st = St::Code;
                        continue;
                    }
                } else if s[i] == b'"' {
                    i += 1;
                    st = St::Code;
                    continue;
                }
                i += 1;
            }
            St::Line => {
                if let Some(e) = close_tag_at(s, i, end) {
                    return Some((i, e));
                }
                if matches!(s[i], b'\n' | b'\r') {
                    st = St::Code;
                }
                i += 1;
            }
            St::Block => {
                if s[i..end].starts_with(b"*/") {
                    i += 2;
                    st = St::Code;
                    continue;
                }
                i += 1;
            }
        }
    }
    None
}

/// `_OPEN_FENCE_RE` (`^[ \t]*```[a-zA-Z0-9_-]*[ \t]*\r?\n?`, multiline):
/// every match, as (start, end).
fn open_fences(s: &[u8]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut line = 0;
    while line <= s.len() {
        let i = skip_blank(s, line);
        if s[i..].starts_with(b"```") {
            let mut j = skip_blank(s, skip_info(s, i + 3));
            if s.get(j) == Some(&b'\r') {
                j += 1;
            }
            if s.get(j) == Some(&b'\n') {
                j += 1;
            }
            out.push((line, j));
            // The next match starts at a line start at or after `j`.
            if j > line && j > 0 && s[j - 1] == b'\n' {
                line = j;
                continue;
            }
        }
        match s[line..].iter().position(|&c| c == b'\n') {
            Some(p) => line = line + p + 1,
            None => break,
        }
    }
    out
}

/// `_CLOSING_FENCE_END_RE` (`\r?\n?[ \t]*```[ \t]*\r?\n?$`) removed.
fn strip_closing_fence(body: &str) -> &str {
    let b = body.as_bytes();
    let mut end = b.len();
    // `$` also matches before a final newline; `\r?\n?` takes it either way.
    if end > 0 && b[end - 1] == b'\n' {
        end -= 1;
    }
    if end > 0 && b[end - 1] == b'\r' {
        end -= 1;
    }
    while end > 0 && matches!(b[end - 1], b' ' | b'\t') {
        end -= 1;
    }
    if end < 3 || &b[end - 3..end] != b"```" {
        return body;
    }
    end -= 3;
    while end > 0 && matches!(b[end - 1], b' ' | b'\t') {
        end -= 1;
    }
    if end > 0 && b[end - 1] == b'\n' {
        end -= 1;
    }
    if end > 0 && b[end - 1] == b'\r' {
        end -= 1;
    }
    &body[..end]
}

/// `EXPRESS_PAYLOAD_START_RE`, multiline: the first line from `from` that
/// starts an Express statement (`surface(`, `deleteSurface(`, `$path =` or
/// `name =`).
fn payload_start(s: &[u8], from: usize) -> Option<usize> {
    let mut line = 0;
    loop {
        if line >= from && statement_start(s, line) {
            return Some(line);
        }
        let p = s[line..].iter().position(|&c| c == b'\n')?;
        line += p + 1;
        if line > s.len() {
            return None;
        }
    }
}

fn statement_start(s: &[u8], line: usize) -> bool {
    let i = skip_blank(s, line);
    let skip_ws = |mut j: usize| {
        while s.get(j).is_some_and(|b| is_space(*b)) {
            j += 1;
        }
        j
    };
    let assign_at = |j: usize| s.get(j) == Some(&b'=') && s.get(j + 1) != Some(&b'=');
    for kw in [&b"surface"[..], b"deleteSurface"] {
        if s[i..].starts_with(kw) && s.get(skip_ws(i + kw.len())) == Some(&b'(') {
            return true;
        }
    }
    if s.get(i) == Some(&b'$') {
        let mut j = i + 1;
        while s
            .get(j)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'/')
        {
            j += 1;
        }
        return assign_at(skip_ws(j));
    }
    if s.get(i)
        .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_')
    {
        let mut j = i + 1;
        while s
            .get(j)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
        {
            j += 1;
        }
        return assign_at(skip_ws(j));
    }
    false
}

/// `_try_recover_unmatched_close`: at most one block whose open tag was left
/// out, closed by a close tag. Returns where reading resumes.
fn recover_unmatched_close(
    parts: &mut Vec<RawPart>,
    content: &str,
    start: usize,
    search_end: usize,
) -> Option<usize> {
    let segment = &content[start..search_end];
    let seg = segment.as_bytes();
    if !(0..seg.len()).any(|i| close_tag_at(seg, i, seg.len()).is_some()) {
        return None;
    }
    let fences = open_fences(seg);
    for &(fence_start, fence_end) in fences.iter().step_by(2) {
        let Some((close_start, close_end)) = scan_block(seg, fence_end, seg.len()) else {
            continue;
        };
        let body = strip_closing_fence(&segment[fence_end..close_start]);
        if !open_fences(body.as_bytes()).is_empty() {
            continue;
        }
        let raw = clean_raw(body);
        if raw.is_empty() {
            continue;
        }
        add_text(parts, &segment[..fence_start]);
        parts.push(RawPart::A2ui {
            raw,
            is_final: true,
        });
        return Some(start + close_end);
    }

    let mut search_from = 0;
    let mut pair = 0;
    while pair + 1 < fences.len() {
        if scan_block(seg, fences[pair + 1].1, seg.len()).is_none() {
            break;
        }
        search_from = fences[pair + 1].1;
        pair += 2;
    }
    let stmt = payload_start(seg, search_from)?;
    let (close_start, close_end) = scan_block(seg, stmt, seg.len())?;
    let raw = clean_raw(&segment[stmt..close_start]);
    if raw.is_empty() {
        return None;
    }
    add_text(parts, &segment[..stmt]);
    parts.push(RawPart::A2ui {
        raw,
        is_final: true,
    });
    Some(start + close_end)
}

/// Splits a response into text and raw Express parts.
///
/// Text runs are trimmed and dropped when blank; markdown fences and the
/// backticks of tags written as inline code are removed. A block still open
/// at the end is a part with `is_final: false`. One block whose open tag was
/// left out is recovered when a close tag ends it.
pub fn unwrap(content: &str) -> Vec<RawPart> {
    let s = content.as_bytes();
    let n = s.len();
    let mut parts = Vec::new();
    let mut i = 0;
    let mut recovered = false;
    while i < n {
        let open = find_open_tag(s, i);
        let search_end = open.map_or(n, |(p, _)| p);
        if !recovered && search_end > i {
            if let Some(next) = recover_unmatched_close(&mut parts, content, i, search_end) {
                recovered = true;
                i = next;
                continue;
            }
        }
        let Some((open_start, open_end)) = open else {
            break;
        };
        add_text(&mut parts, &content[i..open_start]);
        match scan_block(s, open_end, n) {
            None => {
                parts.push(RawPart::A2ui {
                    raw: clean_raw(&content[open_end..]),
                    is_final: false,
                });
                return parts;
            }
            Some((close_start, close_end)) => {
                parts.push(RawPart::A2ui {
                    raw: clean_raw(&content[open_end..close_start]),
                    is_final: true,
                });
                i = close_end;
            }
        }
    }
    add_text(&mut parts, &content[i.min(n)..]);
    parts
}

/// Whether a response holds Express: a block, or with `complete` a closed
/// one.
pub fn has_format_content(content: &str, complete: bool) -> bool {
    unwrap(content).iter().any(|p| match p {
        RawPart::A2ui { is_final, .. } => !complete || *is_final,
        RawPart::Text(_) => false,
    })
}

/// Writes parts back out as a model would emit them: text as it is, each
/// block between `<a2ui>` tags on their own lines, parts joined by newlines.
pub fn wrap(parts: &[RawPart]) -> String {
    parts
        .iter()
        .map(|p| match p {
            RawPart::Text(t) => t.clone(),
            RawPart::A2ui { raw, .. } => format!("<a2ui>\n{raw}\n</a2ui>"),
        })
        .collect::<Vec<_>>()
        .join("\n")
}
