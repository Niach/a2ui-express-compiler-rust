//! Upstream's source repairs, applied before lexing.
//!
//! A port of `_normalize_express_source` (`compiler.py`, mirrored by
//! `normalizeExpressSource` in `visitor.ts`). On top of `Express.g4` it
//! tolerates, outside string literals and comments:
//!
//! - `--` single-line comments, rewritten to `//`;
//! - a data path split across a line break after a `/` (`$/user/` + `name`);
//! - an assignment target split across a line break (`greeting_` + `label =`),
//!   unless either half is itself assigned in the block;
//! - an argument identifier split across a line break, when the joined name
//!   is assigned in the block and neither half is.
//!
//! The rewrite keeps every line break, so line numbers in errors still point
//! into the original text. Upstream implements this with regular
//! expressions; the scanners below match exactly what those expressions
//! match, including their quirks (an `r"…"` is found even at the end of an
//! identifier such as `bar"x"`, and only ASCII identifiers are joined).

use std::collections::HashSet;

/// Applies upstream's repairs to `source`.
pub fn normalize(source: &str) -> String {
    normalize_with(source, &HashSet::new())
}

/// [`normalize`], treating `known` as assigned too.
///
/// A streaming caller passes the names assigned by statements it has
/// already emitted, which are no longer in the text it normalizes.
pub fn normalize_with(source: &str, known: &HashSet<String>) -> String {
    let chars: Vec<char> = source.chars().collect();
    let segments = segment(&chars);

    // Bracket depth before each character of the plain segments.
    let mut depth = 0i32;
    let seg_depths: Vec<Vec<i32>> = segments
        .iter()
        .map(|seg| {
            if seg.is_token {
                return Vec::new();
            }
            seg.text
                .iter()
                .map(|&c| {
                    let d = depth;
                    match c {
                        '(' | '[' | '{' => depth += 1,
                        ')' | ']' | '}' if depth > 0 => depth -= 1,
                        _ => {}
                    }
                    d
                })
                .collect()
        })
        .collect();

    let plain = || {
        segments
            .iter()
            .enumerate()
            .filter(|(_, s)| !s.is_token)
            .map(|(i, s)| (i, &s.text[..], &seg_depths[i][..]))
    };

    let mut split_p2_starts: HashSet<(usize, usize)> = HashSet::new();
    for (i, text, depths) in plain() {
        for m in split_lhs_matches(text) {
            if depths[m.start] == 0 {
                split_p2_starts.insert((i, m.p2.0));
            }
        }
    }

    let mut direct: HashSet<String> = known.clone();
    for (i, text, depths) in plain() {
        for (start, end) in declared_lhs_matches(text) {
            if depths[start] == 0 && !split_p2_starts.contains(&(i, start)) {
                direct.insert(text[start..end].iter().collect());
            }
        }
    }

    let mut declared = direct.clone();
    for (_, text, depths) in plain() {
        for m in split_lhs_matches(text) {
            if depths[m.start] == 0 {
                let p1 = m.p1_trimmed(text);
                let p2 = m.p2(text);
                if !direct.contains(&p1) && !direct.contains(&p2) {
                    declared.insert(p1 + &p2);
                }
            }
        }
    }

    let mut out = String::with_capacity(source.len());
    for seg in &segments {
        if seg.is_token {
            out.extend(seg.text.iter());
            continue;
        }
        let text = join_data_paths(&seg.text);
        let text = join_split_lhs(&text, &direct);
        let text = join_split_args(&text, &declared);
        out.extend(text.iter());
    }
    out
}

struct Segment {
    is_token: bool,
    text: Vec<char>,
}

fn starts(chars: &[char], i: usize, s: &str) -> bool {
    (i..).zip(s.chars()).all(|(j, c)| chars.get(j) == Some(&c))
}

/// The end of the first `close` at or after `i`, with `\\.` escapes when
/// `escapes` is set (`(?:\\[\s\S]|[^\\])*?close`).
fn lazy_close(chars: &[char], mut i: usize, close: &str, escapes: bool) -> Option<usize> {
    loop {
        if starts(chars, i, close) {
            return Some(i + close.chars().count());
        }
        match chars.get(i) {
            None => return None,
            Some('\\') if escapes => {
                if i + 1 >= chars.len() {
                    return None;
                }
                i += 2;
            }
            Some(_) => i += 1,
        }
    }
}

/// `_SQL_COMMENT_OR_TOKEN_RE` at `i`: the end of a string or comment token,
/// and whether it was a `--` comment.
fn token_at(chars: &[char], i: usize) -> Option<(usize, bool)> {
    let c = chars[i];
    if c == 'r' || c == 'R' {
        // [rR]"""[\s\S]*?"""
        if starts(chars, i + 1, "\"\"\"") {
            if let Some(e) = lazy_close(chars, i + 4, "\"\"\"", false) {
                return Some((e, false));
            }
        }
        // [rR]"[^\r\n"]*"
        if chars.get(i + 1) == Some(&'"') {
            let mut j = i + 2;
            while let Some(&d) = chars.get(j) {
                match d {
                    '"' => return Some((j + 1, false)),
                    '\r' | '\n' => break,
                    _ => j += 1,
                }
            }
        }
    }
    if c == '"' {
        if starts(chars, i, "\"\"\"") {
            if let Some(e) = lazy_close(chars, i + 3, "\"\"\"", true) {
                return Some((e, false));
            }
        }
        if let Some(e) = lazy_close(chars, i + 1, "\"", true) {
            return Some((e, false));
        }
    }
    let line_end = |mut j: usize| {
        while chars.get(j).is_some_and(|&d| d != '\r' && d != '\n') {
            j += 1;
        }
        j
    };
    if c == '#' {
        return Some((line_end(i + 1), false));
    }
    if starts(chars, i, "//") {
        return Some((line_end(i + 2), false));
    }
    if starts(chars, i, "/*") {
        if let Some(e) = lazy_close(chars, i + 2, "*/", false) {
            return Some((e, false));
        }
    }
    if starts(chars, i, "--") {
        return Some((line_end(i + 2), true));
    }
    None
}

fn segment(chars: &[char]) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut plain_start = 0;
    let mut i = 0;
    while i < chars.len() {
        match token_at(chars, i) {
            Some((end, sql)) => {
                if i > plain_start {
                    segments.push(Segment {
                        is_token: false,
                        text: chars[plain_start..i].to_vec(),
                    });
                }
                let mut text = chars[i..end].to_vec();
                if sql {
                    text[0] = '/';
                    text[1] = '/';
                }
                segments.push(Segment {
                    is_token: true,
                    text,
                });
                i = end;
                plain_start = end;
            }
            None => i += 1,
        }
    }
    if plain_start < chars.len() {
        segments.push(Segment {
            is_token: false,
            text: chars[plain_start..].to_vec(),
        });
    }
    segments
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn is_blank(c: char) -> bool {
    c == ' ' || c == '\t'
}

/// Python's `\s` for `str` patterns.
fn is_py_space(c: char) -> bool {
    c.is_whitespace() || ('\x1c'..='\x1f').contains(&c)
}

fn skip(text: &[char], mut i: usize, pred: impl Fn(char) -> bool) -> usize {
    while text.get(i).is_some_and(|&c| pred(c)) {
        i += 1;
    }
    i
}

/// `\r?\n` at `i`: the index after it.
fn newline(text: &[char], i: usize) -> Option<usize> {
    match (text.get(i), text.get(i + 1)) {
        (Some('\r'), Some('\n')) => Some(i + 2),
        (Some('\n'), _) => Some(i + 1),
        _ => None,
    }
}

/// `[ \t]*=(?!=)` at `i`.
fn assign_follows(text: &[char], i: usize) -> bool {
    let j = skip(text, i, is_blank);
    text.get(j) == Some(&'=') && text.get(j + 1) != Some(&'=')
}

/// Start indices where `^` matches in multi-line mode.
fn line_starts(text: &[char]) -> impl Iterator<Item = usize> + '_ {
    std::iter::once(0).chain(
        text.iter()
            .enumerate()
            .filter(|(_, c)| **c == '\n')
            .map(|(i, _)| i + 1),
    )
}

/// A match of `_SPLIT_LHS_RE`:
/// `^([ \t]*IDENT)[ \t]*(\r?\n)[ \t]*([IDCHARS]+)(?=[ \t]*=(?!=))`.
struct SplitLhs {
    start: usize,
    /// Group 1, leading blanks included.
    p1: (usize, usize),
    /// Group 2.
    nl: (usize, usize),
    /// Group 3.
    p2: (usize, usize),
}

impl SplitLhs {
    fn p1_trimmed(&self, text: &[char]) -> String {
        text[self.p1.0..self.p1.1]
            .iter()
            .collect::<String>()
            .trim()
            .to_string()
    }

    fn p2(&self, text: &[char]) -> String {
        text[self.p2.0..self.p2.1].iter().collect()
    }
}

fn split_lhs_at(text: &[char], start: usize) -> Option<SplitLhs> {
    let id = skip(text, start, is_blank);
    if !text.get(id).copied().is_some_and(is_ident_start) {
        return None;
    }
    let p1_end = skip(text, id + 1, is_ident_char);
    let nl_start = skip(text, p1_end, is_blank);
    let nl_end = newline(text, nl_start)?;
    let p2_start = skip(text, nl_end, is_blank);
    let p2_end = skip(text, p2_start, is_ident_char);
    if p2_end == p2_start || !assign_follows(text, p2_end) {
        return None;
    }
    Some(SplitLhs {
        start,
        p1: (start, p1_end),
        nl: (nl_start, nl_end),
        p2: (p2_start, p2_end),
    })
}

fn split_lhs_matches(text: &[char]) -> Vec<SplitLhs> {
    let mut out: Vec<SplitLhs> = Vec::new();
    for start in line_starts(text) {
        if out.last().is_some_and(|m| start < m.p2.1) {
            continue;
        }
        if let Some(m) = split_lhs_at(text, start) {
            out.push(m);
        }
    }
    out
}

/// `^[ \t]*(IDENT)[ \t]*=(?!=)` in multi-line mode: group 1's range.
fn declared_lhs_matches(text: &[char]) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for start in line_starts(text) {
        if out.last().is_some_and(|&(_, end)| start < end) {
            continue;
        }
        let id = skip(text, start, is_blank);
        if !text.get(id).copied().is_some_and(is_ident_start) {
            continue;
        }
        let end = skip(text, id + 1, is_ident_char);
        if assign_follows(text, end) {
            out.push((id, end));
        }
    }
    out
}

fn is_path_char(c: char) -> bool {
    is_ident_char(c) || c == '/'
}

/// `_SPLIT_DATA_PATH_RE` substitution:
/// `(\$[IDCHARS/]*/)[ \t]*(\r?\n)[ \t]*([IDCHARS/]+)` becomes `p1 p2 nl`,
/// unless `p2` is a bare identifier followed by `=`, which starts the next
/// assignment instead.
fn join_data_paths(text: &[char]) -> Vec<char> {
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        if text[i] == '$' {
            let run_end = skip(text, i + 1, is_path_char);
            if run_end > i + 1 && text[run_end - 1] == '/' {
                let nl_start = skip(text, run_end, is_blank);
                if let Some(nl_end) = newline(text, nl_start) {
                    let p2_start = skip(text, nl_end, is_blank);
                    let p2_end = skip(text, p2_start, is_path_char);
                    if p2_end > p2_start {
                        let p2 = &text[p2_start..p2_end];
                        let ident_only =
                            is_ident_start(p2[0]) && p2.iter().all(|&c| is_ident_char(c));
                        if !(ident_only && assign_follows(text, p2_end)) {
                            out.extend_from_slice(&text[i..run_end]);
                            out.extend_from_slice(p2);
                            out.extend_from_slice(&text[nl_start..nl_end]);
                        } else {
                            out.extend_from_slice(&text[i..p2_end]);
                        }
                        i = p2_end;
                        continue;
                    }
                }
            }
        }
        out.push(text[i]);
        i += 1;
    }
    out
}

/// `_SPLIT_LHS_RE` substitution: joins the halves unless either is assigned
/// directly.
fn join_split_lhs(text: &[char], direct: &HashSet<String>) -> Vec<char> {
    let mut out = Vec::with_capacity(text.len());
    let mut copied = 0;
    for m in split_lhs_matches(text) {
        let p1 = m.p1_trimmed(text);
        let p2 = m.p2(text);
        if direct.contains(&p1) || direct.contains(&p2) {
            continue;
        }
        out.extend_from_slice(&text[copied..m.p1.1]);
        out.extend_from_slice(&text[m.p2.0..m.p2.1]);
        out.extend_from_slice(&text[m.nl.0..m.nl.1]);
        copied = m.p2.1;
    }
    out.extend_from_slice(&text[copied..]);
    out
}

/// `_SPLIT_ARG_IDENT_RE` at `i`:
/// `([(\[,:=][ \t]*\r?\n?[ \t]*)(IDENT)[ \t]*(\r?\n)[ \t]*([IDCHARS]+)`
/// followed by `[ \t]*[,)\]}]` or `\s*\r?\n`. Returns the end of group 1,
/// group 2, group 3, group 4.
type ArgSplit = (usize, (usize, usize), (usize, usize), (usize, usize));

fn split_arg_at(text: &[char], i: usize) -> Option<ArgSplit> {
    if !matches!(text[i], '(' | '[' | ',' | ':' | '=') {
        return None;
    }
    let mut j = skip(text, i + 1, is_blank);
    if text.get(j) == Some(&'\r') {
        j += 1;
    }
    if text.get(j) == Some(&'\n') {
        j += 1;
    }
    let p1_start = skip(text, j, is_blank);
    if !text.get(p1_start).copied().is_some_and(is_ident_start) {
        return None;
    }
    let p1_end = skip(text, p1_start + 1, is_ident_char);
    let nl_start = skip(text, p1_end, is_blank);
    let nl_end = newline(text, nl_start)?;
    let p2_start = skip(text, nl_end, is_blank);
    let p2_end = skip(text, p2_start, is_ident_char);
    if p2_end == p2_start {
        return None;
    }
    let k = skip(text, p2_end, is_blank);
    let closes = matches!(text.get(k), Some(',' | ')' | ']' | '}'));
    let breaks = text[p2_end..]
        .iter()
        .take_while(|&&c| is_py_space(c))
        .any(|&c| c == '\n');
    if !(closes || breaks) {
        return None;
    }
    Some((
        p1_start,
        (p1_start, p1_end),
        (nl_start, nl_end),
        (p2_start, p2_end),
    ))
}

/// `_SPLIT_ARG_IDENT_RE` substitution: joins an argument split across a line
/// break when the joined name is assigned and neither half is.
fn join_split_args(text: &[char], declared: &HashSet<String>) -> Vec<char> {
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        if let Some((prefix_end, p1, nl, p2)) = split_arg_at(text, i) {
            let s1: String = text[p1.0..p1.1].iter().collect();
            let s2: String = text[p2.0..p2.1].iter().collect();
            let joined = format!("{s1}{s2}");
            if declared.contains(&joined) && !declared.contains(&s1) && !declared.contains(&s2) {
                out.extend_from_slice(&text[i..prefix_end]);
                out.extend(joined.chars());
                out.extend_from_slice(&text[nl.0..nl.1]);
            } else {
                out.extend_from_slice(&text[i..p2.1]);
            }
            i = p2.1;
            continue;
        }
        out.push(text[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::normalize;

    #[test]
    fn sql_comments_become_slash_comments_outside_strings() {
        assert_eq!(
            normalize("-- note\nroot = Text(\"a -- b\") -- end"),
            "// note\nroot = Text(\"a -- b\") // end"
        );
    }

    #[test]
    fn split_identifier_and_split_path_are_joined() {
        assert_eq!(
            normalize(
                "greeting_\nlabel = Text($/user/\nname)\nroot = Card(\n  greeting_\n  label\n)"
            ),
            "greeting_label\n = Text($/user/name\n)\nroot = Card(\n  greeting_label\n\n)"
        );
    }

    #[test]
    fn halves_that_are_assigned_are_not_joined() {
        let src = "header = Text(\"H\")\nbody = Text(\"B\")\nheaderbody = Text(\"HB\")\nroot = Column([header\nbody])";
        assert_eq!(normalize(src), src);
    }

    #[test]
    fn a_path_before_the_next_assignment_is_not_joined() {
        let src = "data = $/user/\nroot = Text(\"hi\")";
        assert_eq!(normalize(src), src);
    }
}
