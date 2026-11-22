//! The Express lexer: `Express.g4`'s lexer rules by hand.
//!
//! It behaves like the ANTLR lexer upstream generates from the grammar: at
//! each position every rule is tried, the longest match wins and a tie goes
//! to the rule defined first (the implicit literal tokens such as `'null'`
//! and `'_'` come before every named rule, `BOOLEAN` before `IDENTIFIER`).
//! A rule that runs past its last accepting point falls back to it, so
//! `1e)` reads as the number `1` followed by the identifier `e`, and an
//! unclosed `"""` reads as the empty string `""` followed by a `"`.
//!
//! A position where no rule matches is a `token recognition error` covering
//! every character the rules got through plus the one that stopped them;
//! those characters are dropped and lexing resumes after them, as ANTLR does.

use super::ast::{Pos, Span};
use super::xid::{ID_CONTINUE, ID_START};

/// Token types, numbered as in upstream's generated parser so expected-token
/// sets list in the same order ANTLR lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum TokenType {
    /// End of input (ANTLR's `EOF`, shown as `<EOF>`).
    Eof = 0,
    /// `=`
    Assign = 1,
    /// `[`
    LBracket = 2,
    /// `,`
    Comma = 3,
    /// `]`
    RBracket = 4,
    /// `{`
    LBrace = 5,
    /// `}`
    RBrace = 6,
    /// `:`
    Colon = 7,
    /// `(`
    LParen = 8,
    /// `)`
    RParen = 9,
    /// `_`
    Underscore = 10,
    /// `null`
    Null = 11,
    /// `r"""…"""`
    RawTripleString = 12,
    /// `"""…"""`
    TripleString = 13,
    /// `r"…"`
    RawString = 14,
    /// `"…"`
    StandardString = 15,
    /// `$…`
    Path = 16,
    /// `?name`
    Check = 17,
    /// A number literal.
    Number = 18,
    /// `true` or `false`.
    Boolean = 19,
    /// A UAX #31 identifier.
    Identifier = 20,
    /// `@name`
    SystemIdentifier = 21,
}

impl TokenType {
    /// The name ANTLR prints for the type in an expected-token set.
    pub fn display_name(self) -> &'static str {
        use TokenType::*;
        match self {
            Eof => "<EOF>",
            Assign => "'='",
            LBracket => "'['",
            Comma => "','",
            RBracket => "']'",
            LBrace => "'{'",
            RBrace => "'}'",
            Colon => "':'",
            LParen => "'('",
            RParen => "')'",
            Underscore => "'_'",
            Null => "'null'",
            RawTripleString => "RAW_TRIPLE_STRING",
            TripleString => "TRIPLE_STRING",
            RawString => "RAW_STRING",
            StandardString => "STANDARD_STRING",
            Path => "PATH",
            Check => "CHECK",
            Number => "NUMBER",
            Boolean => "BOOLEAN",
            Identifier => "IDENTIFIER",
            SystemIdentifier => "SYSTEM_IDENTIFIER",
        }
    }

    const ALL: [TokenType; 22] = {
        use TokenType::*;
        [
            Eof,
            Assign,
            LBracket,
            Comma,
            RBracket,
            LBrace,
            RBrace,
            Colon,
            LParen,
            RParen,
            Underscore,
            Null,
            RawTripleString,
            TripleString,
            RawString,
            StandardString,
            Path,
            Check,
            Number,
            Boolean,
            Identifier,
            SystemIdentifier,
        ]
    };
}

/// A set of token types, printed the way ANTLR prints an `IntervalSet`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TokenSet(u32);

impl TokenSet {
    /// A set of the given types.
    pub const fn of(types: &[TokenType]) -> TokenSet {
        let mut bits = 0;
        let mut i = 0;
        while i < types.len() {
            bits |= 1 << types[i] as u32;
            i += 1;
        }
        TokenSet(bits)
    }

    /// The union of two sets.
    pub const fn union(self, other: TokenSet) -> TokenSet {
        TokenSet(self.0 | other.0)
    }

    /// Whether `t` is in the set.
    pub fn contains(self, t: TokenType) -> bool {
        self.0 & (1 << t as u32) != 0
    }
}

impl std::fmt::Display for TokenSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = TokenType::ALL
            .iter()
            .filter(|t| self.contains(**t))
            .map(|t| t.display_name())
            .collect();
        if names.len() == 1 {
            f.write_str(names[0])
        } else {
            write!(f, "{{{}}}", names.join(", "))
        }
    }
}

/// A token.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// The type.
    pub ty: TokenType,
    /// The text as written; empty for [`TokenType::Eof`].
    pub text: String,
    /// Where it is.
    pub span: Span,
    /// Only whitespace and comments containing a line break or a `;` stand
    /// between this token and the previous one (or it is the first token):
    /// a place a new statement can start after an error.
    pub line_initial: bool,
}

/// A `token recognition error`.
#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    /// ANTLR's message, `token recognition error at: '…'`.
    pub message: String,
    /// The characters dropped.
    pub span: Span,
    /// The dropped characters run to the end of the input, as an unclosed
    /// string or block comment does.
    pub at_eof: bool,
    /// The index of the token lexed right after the error; ANTLR reports the
    /// error when the parser first asks for that token.
    pub before_token: usize,
}

/// The output of [`lex`]: the tokens, ending with [`TokenType::Eof`], and the
/// errors in source order.
#[derive(Debug, Clone, PartialEq)]
pub struct Lexed {
    /// The tokens; the last one is always [`TokenType::Eof`].
    pub tokens: Vec<Token>,
    /// The recognition errors.
    pub errors: Vec<LexError>,
}

fn in_ranges(c: char, ranges: &[(u32, u32)]) -> bool {
    let c = c as u32;
    ranges
        .binary_search_by(|&(lo, hi)| {
            if hi < c {
                std::cmp::Ordering::Less
            } else if lo > c {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// `[\p{XID_Start}_]`, with upstream's exact tables.
pub fn is_id_start(c: char) -> bool {
    in_ranges(c, ID_START)
}

/// `[\p{XID_Continue}]`, with upstream's exact tables.
pub fn is_id_continue(c: char) -> bool {
    in_ranges(c, ID_CONTINUE)
}

/// Whether `s` reads as one `IDENTIFIER` token: a UAX #31 identifier that is
/// not one of the reserved words `true`, `false`, `null` and `_`.
pub fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if is_id_start(c))
        && chars.all(is_id_continue)
        && !matches!(s, "true" | "false" | "null" | "_")
}

/// How far one lexer rule gets from a position, in characters.
#[derive(Clone, Copy, Default)]
struct RuleMatch {
    /// The longest accepted length.
    accept: Option<usize>,
    /// Characters consumed before the rule could go no further.
    alive: usize,
}

impl RuleMatch {
    fn dead() -> Self {
        Self::default()
    }
}

/// Skipped token kinds and the named rules, in grammar order after the
/// implicit literal tokens.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Rule {
    Token(TokenType),
    Comment,
    BlockComment,
    Semicolon,
    Ws,
}

struct Lexer<'a> {
    chars: Vec<char>,
    offsets: Vec<usize>,
    src: &'a str,
}

impl Lexer<'_> {
    fn at(&self, i: usize) -> Option<char> {
        self.chars.get(i).copied()
    }

    fn n(&self) -> usize {
        self.chars.len()
    }

    /// A fixed word such as `null` or `true`.
    fn word(&self, s: usize, word: &str) -> RuleMatch {
        let mut i = s;
        for w in word.chars() {
            if self.at(i) != Some(w) {
                return RuleMatch {
                    accept: None,
                    alive: i - s,
                };
            }
            i += 1;
        }
        RuleMatch {
            accept: Some(i - s),
            alive: i - s,
        }
    }

    /// The end (exclusive) of the first `close` at or after `i`, scanning
    /// with `\\.` escapes when `escapes` is set; `Err(alive_end)` when the
    /// input ends first.
    fn scan_to(&self, mut i: usize, close: &[char], escapes: bool) -> Result<usize, usize> {
        loop {
            if self.chars[i.min(self.n())..].starts_with(close) {
                return Ok(i + close.len());
            }
            match self.at(i) {
                None => return Err(self.n()),
                Some('\\') if escapes => {
                    if i + 1 >= self.n() {
                        return Err(self.n());
                    }
                    i += 2;
                }
                Some(_) => i += 1,
            }
        }
    }

    fn string_rule(&self, s: usize, raw: bool, triple: bool) -> RuleMatch {
        let mut i = s;
        if raw {
            if !matches!(self.at(i), Some('r' | 'R')) {
                return RuleMatch::dead();
            }
            i += 1;
        }
        let quotes = if triple { 3 } else { 1 };
        for _ in 0..quotes {
            if self.at(i) != Some('"') {
                return RuleMatch {
                    accept: None,
                    alive: i - s,
                };
            }
            i += 1;
        }
        let result = if raw && !triple {
            // ~[\r\n"]* '"'
            loop {
                match self.at(i) {
                    Some('"') => break Ok(i + 1),
                    Some('\r' | '\n') | None => break Err(i),
                    Some(_) => i += 1,
                }
            }
        } else {
            let close: &[char] = if triple { &['"', '"', '"'] } else { &['"'] };
            self.scan_to(i, close, !raw)
        };
        match result {
            Ok(end) => RuleMatch {
                accept: Some(end - s),
                alive: end - s,
            },
            Err(stuck) => RuleMatch {
                accept: None,
                alive: stuck - s,
            },
        }
    }

    fn run(&self, mut i: usize, pred: impl Fn(char) -> bool) -> usize {
        while self.at(i).is_some_and(&pred) {
            i += 1;
        }
        i
    }

    /// `prefix? [\p{XID_Start}_] [\p{XID_Continue}]*`.
    fn ident_rule(&self, s: usize, prefix: Option<char>) -> RuleMatch {
        let mut i = s;
        if let Some(p) = prefix {
            if self.at(i) != Some(p) {
                return RuleMatch::dead();
            }
            i += 1;
        }
        if !self.at(i).is_some_and(is_id_start) {
            return RuleMatch {
                accept: None,
                alive: i - s,
            };
        }
        let end = self.run(i + 1, is_id_continue);
        RuleMatch {
            accept: Some(end - s),
            alive: end - s,
        }
    }

    /// `[+-]? ([0-9]+ ('.' [0-9]*)? | '.' [0-9]+) ([eE] [+-]? [0-9]+)?`.
    fn number_rule(&self, s: usize) -> RuleMatch {
        let digit = |c: char| c.is_ascii_digit();
        let mut i = s;
        if matches!(self.at(i), Some('+' | '-')) {
            i += 1;
        }
        let stuck = |i: usize| RuleMatch {
            accept: None,
            alive: i - s,
        };
        if self.at(i).is_some_and(digit) {
            i = self.run(i, digit);
            if self.at(i) == Some('.') {
                i = self.run(i + 1, digit);
            }
        } else if self.at(i) == Some('.') {
            i += 1;
            if !self.at(i).is_some_and(digit) {
                return stuck(i);
            }
            i = self.run(i, digit);
        } else {
            return stuck(i);
        }
        let accept = i;
        if matches!(self.at(i), Some('e' | 'E')) {
            let mut j = i + 1;
            if matches!(self.at(j), Some('+' | '-')) {
                j += 1;
            }
            if self.at(j).is_some_and(digit) {
                let end = self.run(j, digit);
                return RuleMatch {
                    accept: Some(end - s),
                    alive: end - s,
                };
            }
            return RuleMatch {
                accept: Some(accept - s),
                alive: j - s,
            };
        }
        RuleMatch {
            accept: Some(accept - s),
            alive: accept - s,
        }
    }

    fn rule(&self, rule: Rule, s: usize) -> RuleMatch {
        use TokenType as T;
        let single = |c: char| {
            if self.at(s) == Some(c) {
                RuleMatch {
                    accept: Some(1),
                    alive: 1,
                }
            } else {
                RuleMatch::dead()
            }
        };
        match rule {
            Rule::Token(t) => match t {
                T::Assign => single('='),
                T::LBracket => single('['),
                T::Comma => single(','),
                T::RBracket => single(']'),
                T::LBrace => single('{'),
                T::RBrace => single('}'),
                T::Colon => single(':'),
                T::LParen => single('('),
                T::RParen => single(')'),
                T::Underscore => single('_'),
                T::Null => self.word(s, "null"),
                T::RawTripleString => self.string_rule(s, true, true),
                T::TripleString => self.string_rule(s, false, true),
                T::RawString => self.string_rule(s, true, false),
                T::StandardString => self.string_rule(s, false, false),
                T::Path => {
                    if self.at(s) != Some('$') {
                        return RuleMatch::dead();
                    }
                    let end =
                        self.run(s + 1, |c| c.is_ascii_alphanumeric() || c == '_' || c == '/');
                    RuleMatch {
                        accept: Some(end - s),
                        alive: end - s,
                    }
                }
                T::Check => self.ident_rule(s, Some('?')),
                T::Number => self.number_rule(s),
                T::Boolean => {
                    let t = self.word(s, "true");
                    let f = self.word(s, "false");
                    RuleMatch {
                        accept: t.accept.or(f.accept),
                        alive: t.alive.max(f.alive),
                    }
                }
                T::Identifier => self.ident_rule(s, None),
                T::SystemIdentifier => self.ident_rule(s, Some('@')),
                T::Eof => RuleMatch::dead(),
            },
            Rule::Comment => {
                let start = match (self.at(s), self.at(s + 1)) {
                    (Some('#'), _) => s + 1,
                    (Some('/'), Some('/')) => s + 2,
                    (Some('/'), _) => {
                        return RuleMatch {
                            accept: None,
                            alive: 1,
                        };
                    }
                    _ => return RuleMatch::dead(),
                };
                let end = self.run(start, |c| c != '\r' && c != '\n');
                RuleMatch {
                    accept: Some(end - s),
                    alive: end - s,
                }
            }
            Rule::BlockComment => {
                if self.at(s) != Some('/') {
                    return RuleMatch::dead();
                }
                if self.at(s + 1) != Some('*') {
                    return RuleMatch {
                        accept: None,
                        alive: 1,
                    };
                }
                match self.scan_to(s + 2, &['*', '/'], false) {
                    Ok(end) => RuleMatch {
                        accept: Some(end - s),
                        alive: end - s,
                    },
                    Err(stuck) => RuleMatch {
                        accept: None,
                        alive: stuck - s,
                    },
                }
            }
            Rule::Semicolon => single(';'),
            Rule::Ws => {
                let end = self.run(s, |c| matches!(c, ' ' | '\t' | '\r' | '\n'));
                if end == s {
                    RuleMatch::dead()
                } else {
                    RuleMatch {
                        accept: Some(end - s),
                        alive: end - s,
                    }
                }
            }
        }
    }
}

const RULES: [Rule; 25] = {
    use TokenType as T;
    [
        Rule::Token(T::Assign),
        Rule::Token(T::LBracket),
        Rule::Token(T::Comma),
        Rule::Token(T::RBracket),
        Rule::Token(T::LBrace),
        Rule::Token(T::RBrace),
        Rule::Token(T::Colon),
        Rule::Token(T::LParen),
        Rule::Token(T::RParen),
        Rule::Token(T::Underscore),
        Rule::Token(T::Null),
        Rule::Token(T::RawTripleString),
        Rule::Token(T::TripleString),
        Rule::Token(T::RawString),
        Rule::Token(T::StandardString),
        Rule::Token(T::Path),
        Rule::Token(T::Check),
        Rule::Token(T::Number),
        Rule::Token(T::Boolean),
        Rule::Token(T::Identifier),
        Rule::Token(T::SystemIdentifier),
        Rule::Comment,
        Rule::BlockComment,
        Rule::Semicolon,
        Rule::Ws,
    ]
};

/// Escapes line breaks and tabs the way ANTLR's error displays do.
pub(crate) fn escape_display(s: &str) -> String {
    s.replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Lexes `src`, whose first character sits at `origin`.
pub fn lex(src: &str, origin: Pos) -> Lexed {
    let chars: Vec<char> = src.chars().collect();
    let mut offsets: Vec<usize> = src.char_indices().map(|(i, _)| i).collect();
    offsets.push(src.len());
    let lx = Lexer {
        chars,
        offsets,
        src,
    };

    // Positions of every character, ANTLR style: only `\n` starts a line.
    let mut positions = Vec::with_capacity(lx.n() + 1);
    let (mut line, mut column) = (origin.line, origin.column);
    for (i, &c) in lx.chars.iter().enumerate() {
        positions.push(Pos {
            line,
            column,
            offset: origin.offset + lx.offsets[i],
        });
        if c == '\n' {
            line += 1;
            column = 0;
        } else {
            column += 1;
        }
    }
    positions.push(Pos {
        line,
        column,
        offset: origin.offset + lx.src.len(),
    });

    let mut tokens = Vec::new();
    let mut errors = Vec::new();
    let mut line_initial = true;
    let mut s = 0;
    while s < lx.n() {
        let mut best: Option<(usize, Rule)> = None;
        let mut alive = 0;
        for rule in RULES {
            let m = lx.rule(rule, s);
            alive = alive.max(m.alive);
            if let Some(len) = m.accept {
                if len > 0 && best.is_none_or(|(l, _)| len > l) {
                    best = Some((len, rule));
                }
            }
        }
        match best {
            Some((len, rule)) => {
                let e = s + len;
                match rule {
                    Rule::Token(ty) => {
                        tokens.push(Token {
                            ty,
                            text: lx.src[lx.offsets[s]..lx.offsets[e]].to_string(),
                            span: Span {
                                start: positions[s],
                                end: positions[e],
                            },
                            line_initial,
                        });
                        line_initial = false;
                    }
                    Rule::Semicolon => line_initial = true,
                    _ => {
                        if lx.chars[s..e].contains(&'\n') {
                            line_initial = true;
                        }
                    }
                }
                s = e;
            }
            None => {
                // The character that stopped every rule is part of the text
                // and is dropped with it.
                let stuck = s + alive;
                let e = (stuck + 1).min(lx.n());
                errors.push(LexError {
                    message: format!(
                        "token recognition error at: '{}'",
                        escape_display(&lx.src[lx.offsets[s]..lx.offsets[e]])
                    ),
                    span: Span {
                        start: positions[s],
                        end: positions[e],
                    },
                    at_eof: stuck >= lx.n(),
                    before_token: tokens.len(),
                });
                s = e;
            }
        }
    }
    let end = positions[lx.n()];
    tokens.push(Token {
        ty: TokenType::Eof,
        text: String::new(),
        span: Span { start: end, end },
        line_initial: true,
    });
    Lexed { tokens, errors }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn types(src: &str) -> Vec<(TokenType, String)> {
        lex(src, Pos::START)
            .tokens
            .into_iter()
            .map(|t| (t.ty, t.text))
            .collect()
    }

    #[test]
    fn reserved_words_win_ties_and_lose_to_longer_identifiers() {
        use TokenType::*;
        let got = types("null _ true false nullable _x truthy");
        let want = [
            (Null, "null"),
            (Underscore, "_"),
            (Boolean, "true"),
            (Boolean, "false"),
            (Identifier, "nullable"),
            (Identifier, "_x"),
            (Identifier, "truthy"),
            (Eof, ""),
        ];
        assert_eq!(got, want.map(|(t, s)| (t, s.to_string())).to_vec());
    }

    #[test]
    fn numbers_fall_back_to_their_last_accepting_point() {
        use TokenType::*;
        assert_eq!(
            types("1e)"),
            vec![
                (Number, "1".into()),
                (Identifier, "e".into()),
                (RParen, ")".into()),
                (Eof, "".into())
            ]
        );
        assert_eq!(types("1.e5")[0], (Number, "1.e5".into()));
        assert_eq!(types("-.25")[0], (Number, "-.25".into()));
    }

    #[test]
    fn an_unclosed_triple_quote_reads_as_an_empty_string() {
        let lexed = lex("\"\"\"abc", Pos::START);
        assert_eq!(lexed.tokens[0].ty, TokenType::StandardString);
        assert_eq!(lexed.tokens[0].text, "\"\"");
        assert_eq!(
            lexed.errors[0].message,
            "token recognition error at: '\"abc'"
        );
        assert!(lexed.errors[0].at_eof);
    }

    #[test]
    fn identifiers_follow_uax_31() {
        assert!(is_identifier("größe"));
        assert!(is_identifier("_private"));
        assert!(!is_identifier("1a"));
        assert!(!is_identifier("null"));
        assert!(!is_identifier("_"));
        assert!(!is_identifier("a-b"));
    }

    #[test]
    fn token_sets_print_like_antlr() {
        use TokenType::*;
        assert_eq!(TokenSet::of(&[RParen]).to_string(), "')'");
        assert_eq!(TokenSet::of(&[RParen, Comma]).to_string(), "{',', ')'}");
        assert_eq!(
            TokenSet::of(&[Identifier, Eof]).to_string(),
            "{<EOF>, IDENTIFIER}"
        );
    }
}
