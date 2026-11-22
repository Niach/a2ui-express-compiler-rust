//! Stripping `<a2ui>` sentinel tags from a block, as upstream's compiler
//! does before parsing (`stripSentinelTags` in `compiler.ts`).
//!
//! Tags inside string literals and comments are kept. When the text holds an
//! open tag, only what lies between open and close tags is kept; otherwise
//! the text is all payload and only the tags themselves are removed.

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tag {
    Open,
    Close,
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

/// The tag starting at `pos`, and where it ends.
fn match_tag(s: &[u8], pos: usize) -> Option<(Tag, usize)> {
    if s.get(pos) != Some(&b'<') {
        return None;
    }
    let word = |at: usize| {
        s.get(at..at + 4)
            .is_some_and(|w| w.eq_ignore_ascii_case(b"a2ui"))
    };
    // `<a2ui(?:\s[^>]*)?>`
    if word(pos + 1) {
        let i = pos + 5;
        match s.get(i) {
            Some(b'>') => return Some((Tag::Open, i + 1)),
            Some(&b) if is_space(b) => {
                if let Some(off) = s[i..].iter().position(|&c| c == b'>') {
                    return Some((Tag::Open, i + off + 1));
                }
            }
            _ => {}
        }
    }
    // `</a2ui\s*>`
    if s.get(pos + 1) == Some(&b'/') && word(pos + 2) {
        let mut i = pos + 6;
        while s.get(i).is_some_and(|&b| is_space(b)) {
            i += 1;
        }
        if s.get(i) == Some(&b'>') {
            return Some((Tag::Close, i + 1));
        }
    }
    None
}

fn find(s: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    s.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

/// The end of a comment starting at `pos`. A line comment ends before a
/// close tag on its line.
fn skip_comment(s: &[u8], pos: usize) -> Option<usize> {
    let n = s.len();
    let rest = &s[pos..];
    if rest.starts_with(b"//") || rest.starts_with(b"#") || rest.starts_with(b"--") {
        let end = find(s, pos, b"\n").unwrap_or(n);
        let mut j = pos + if s[pos] == b'#' { 1 } else { 2 };
        while j < end {
            if s[j] == b'<' && matches!(match_tag(s, j), Some((Tag::Close, _))) {
                return Some(j);
            }
            j += 1;
        }
        return Some(end);
    }
    if rest.starts_with(b"/*") {
        return Some(find(s, pos + 2, b"*/").map_or(n, |e| e + 2));
    }
    None
}

/// The end of a string literal starting at `pos`.
fn skip_string(s: &[u8], pos: usize) -> Option<usize> {
    let n = s.len();
    let mut raw = false;
    let mut q = pos;
    if matches!(s[pos], b'r' | b'R')
        && s.get(pos + 1) == Some(&b'"')
        && (pos == 0 || !(s[pos - 1].is_ascii_alphanumeric() || s[pos - 1] == b'_'))
    {
        raw = true;
        q = pos + 1;
    } else if s[pos] != b'"' {
        return None;
    }
    let triple = s[q..].starts_with(b"\"\"\"");
    let mut i = q + if triple { 3 } else { 1 };
    while i < n {
        if !raw && s[i] == b'\\' {
            i += 2;
            continue;
        }
        if triple && s[i..].starts_with(b"\"\"\"") {
            return Some(i + 3);
        }
        if !triple && s[i] == b'"' {
            return Some(i + 1);
        }
        i += 1;
    }
    Some(n)
}

/// Removes sentinel tags, keeping only the text inside them when there is an
/// open tag.
pub(crate) fn strip_sentinel_tags(text: &str) -> String {
    let s = text.as_bytes();
    let n = s.len();

    let mut has_open = false;
    let mut pos = 0;
    while pos < n {
        if let Some(end) = skip_string(s, pos).or_else(|| skip_comment(s, pos)) {
            pos = end;
            continue;
        }
        if matches!(match_tag(s, pos), Some((Tag::Open, _))) {
            has_open = true;
            break;
        }
        pos += 1;
    }

    let mut out: Vec<u8> = Vec::with_capacity(n);
    let mut inside = !has_open;
    let mut seg_start: Option<usize> = inside.then_some(0);
    let mut i = 0;
    while i < n {
        if let Some(end) = skip_string(s, i).or_else(|| skip_comment(s, i)) {
            i = end;
            continue;
        }
        if let Some((tag, end)) = match_tag(s, i) {
            if let (true, Some(start)) = (inside, seg_start) {
                out.extend_from_slice(&s[start..i]);
            }
            inside = tag == Tag::Open;
            seg_start = inside.then_some(end);
            i = end;
            continue;
        }
        i += 1;
    }
    if let (true, Some(start)) = (inside, seg_start) {
        if start < n {
            out.extend_from_slice(&s[start..]);
        }
    }
    // Every cut falls on an ASCII byte, so the result is still UTF-8.
    String::from_utf8(out).expect("cuts fall on ASCII boundaries")
}

#[cfg(test)]
mod tests {
    use super::strip_sentinel_tags as strip;

    #[test]
    fn text_without_tags_is_kept() {
        assert_eq!(strip("root = Text(\"a\")"), "root = Text(\"a\")");
    }

    #[test]
    fn only_the_inside_of_tags_is_kept() {
        assert_eq!(strip("hi <a2ui>x = 1</a2ui> bye"), "x = 1");
        assert_eq!(strip("<A2UI version=\"1\">x</a2ui >"), "x");
    }

    #[test]
    fn tags_in_strings_and_comments_stay() {
        let s = "<a2ui>t = Text(\"</a2ui>\") // </a2ui> no\n</a2ui>";
        assert_eq!(strip(s), "t = Text(\"</a2ui>\") // ");
        let c = "<a2ui>/* </a2ui> */ x</a2ui>";
        assert_eq!(strip(c), "/* </a2ui> */ x");
    }

    #[test]
    fn a_close_tag_without_an_open_tag_is_removed() {
        assert_eq!(strip("x = 1</a2ui>"), "x = 1");
    }
}
