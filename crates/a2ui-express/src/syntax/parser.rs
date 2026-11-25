//! The Express parser: `Express.g4`'s parser rules as recursive descent.
//!
//! # Error text
//!
//! Upstream parses with an ANTLR-generated parser and shows the model the
//! first error ANTLR reports. The training data holds repair examples built
//! from that text, so this parser reports the same message at the same
//! position. It follows the generated parser's structure: where the
//! generated code calls the error strategy's `sync` before a decision, this
//! parser checks the same token set and words the error as ANTLR's
//! `DefaultErrorStrategy` would (`extraneous input` when dropping one token
//! would let the rule go on, `mismatched input` otherwise, `missing` when
//! inserting one would, and `no viable alternative` where adaptive
//! prediction gives up). The expected-token sets are the ones ANTLR computes
//! for those parser states, and they print in ANTLR's order.
//!
//! Only the first error of a statement is reported. Where ANTLR would repair
//! the statement and carry on, this parser drops it.
//!
//! # Recovery
//!
//! A statement with an error is dropped, and parsing resumes at the next
//! statement: the next token that begins a line (or follows a `;`) and can
//! start a statement, once the brackets the bad statement opened are closed.
//! When they never close, it resumes at the first such token. A statement
//! sharing a line with a lexer error, or with a stray token between
//! statements, is dropped as well, so a bad line never compiles in part.

use super::ast::*;
use super::error::{ParseError, ParseErrorKind};
use super::lexer::{Lexed, Token, TokenSet, TokenType, escape_display, lex_with};
use super::normalize::normalize_dialect;
use std::collections::HashSet;

use TokenType as T;

/// FIRST(expression), which is also FIRST(statement) and FIRST(arg).
const EXPR_START: TokenSet = TokenSet::of(&[
    T::LBracket,
    T::LBrace,
    T::Underscore,
    T::Null,
    T::RawTripleString,
    T::TripleString,
    T::RawString,
    T::StandardString,
    T::Path,
    T::Check,
    T::Number,
    T::Boolean,
    T::Identifier,
    T::SystemIdentifier,
    // Only the extension's dialect lexes one; never printed in a set.
    T::Color,
]);
const STRINGS: TokenSet = TokenSet::of(&[
    T::RawTripleString,
    T::TripleString,
    T::RawString,
    T::StandardString,
]);
const PROGRAM_LOOP: TokenSet = EXPR_START.union(TokenSet::of(&[T::Eof]));
const MAP_KEY: TokenSet = STRINGS.union(TokenSet::of(&[T::Identifier]));

/// The result of parsing a block.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParseOutput {
    /// The statements that parsed cleanly.
    pub program: Program,
    /// Every error, in the order upstream's parser would report them.
    pub errors: Vec<ParseError>,
    /// The source each dropped statement covered, from its first token to
    /// where parsing resumed.
    pub dropped: Vec<Span>,
}

impl ParseOutput {
    /// The error upstream's compiler raises for the block, if any: the first
    /// lexer or syntax error, else the first out-of-range number, else the
    /// first malformed `_template`.
    pub fn first_error(&self) -> Option<&ParseError> {
        [
            &[ParseErrorKind::Lexer, ParseErrorKind::Syntax][..],
            &[ParseErrorKind::NumberOutOfRange],
            &[ParseErrorKind::Template],
        ]
        .iter()
        .find_map(|kinds| self.errors.iter().find(|e| kinds.contains(&e.kind)))
    }

    /// The program, or the block's error as upstream reports it.
    pub fn into_result(self) -> Result<Program, ParseError> {
        match self.first_error() {
            Some(e) => Err(e.clone()),
            None => Ok(self.program),
        }
    }
}

/// Parses a block of Express, applying upstream's source repairs first.
pub fn parse(source: &str) -> ParseOutput {
    parse_at(source, Pos::START, &HashSet::new())
}

/// [`parse`] for text that starts at `origin` within a larger block.
/// `known` holds names assigned earlier in the block (see
/// [`super::normalize::normalize_with`]).
pub fn parse_at(source: &str, origin: Pos, known: &HashSet<String>) -> ParseOutput {
    parse_dialect(source, origin, known, Dialect::UPSTREAM)
}

/// [`parse_at`] in a [`Dialect`].
pub fn parse_dialect(
    source: &str,
    origin: Pos,
    known: &HashSet<String>,
    dialect: Dialect,
) -> ParseOutput {
    let normalized = normalize_dialect(source, known, dialect.hex_colors);
    parse_tokens(lex_with(&normalized, origin, dialect.hex_colors))
}

/// What the parser reads on top of upstream Express.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Dialect {
    /// `#rrggbb` / `#rrggbbaa` colour literals ([`ExprKind::Color`]) instead
    /// of `#` comments where one stands (the `at_exponential_express`
    /// extension).
    pub hex_colors: bool,
}

impl Dialect {
    /// Upstream Express, as `Express.g4` reads it.
    pub const UPSTREAM: Dialect = Dialect { hex_colors: false };
    /// The `at_exponential_express` extension's dialect.
    pub const EXTENDED: Dialect = Dialect { hex_colors: true };
}

/// Parses already-lexed tokens, without source repairs.
pub fn parse_tokens(lexed: Lexed) -> ParseOutput {
    let mut p = Parser {
        tokens: lexed.tokens,
        lex_errors: lexed.errors,
        lex_flushed: 0,
        p: 0,
        errors: Vec::new(),
        dropped_lines: Vec::new(),
        dropped: Vec::new(),
    };
    let statements = p.program();
    let dropped = std::mem::take(&mut p.dropped_lines);
    let statements = statements
        .into_iter()
        .filter(|s| {
            !dropped
                .iter()
                .any(|&l| s.span.start.line <= l && l <= s.span.end.line)
        })
        .collect();
    ParseOutput {
        program: Program { statements },
        errors: p.errors,
        dropped: p.dropped,
    }
}

/// An error was recorded; the statement is abandoned.
struct Abort;

type PResult<T> = Result<T, Abort>;

struct Parser {
    tokens: Vec<Token>,
    lex_errors: Vec<super::lexer::LexError>,
    lex_flushed: usize,
    p: usize,
    errors: Vec<ParseError>,
    /// Lines whose statements are dropped: lexer errors and stray tokens.
    dropped_lines: Vec<u32>,
    dropped: Vec<Span>,
}

impl Parser {
    // ----- token stream ---------------------------------------------------

    /// Reports the lexer errors met while lexing up to token `i`, the way
    /// ANTLR's lazily filled token stream does.
    fn touch(&mut self, i: usize) {
        while let Some(e) = self.lex_errors.get(self.lex_flushed) {
            if e.before_token > i {
                break;
            }
            self.errors.push(ParseError {
                kind: ParseErrorKind::Lexer,
                message: e.message.clone(),
                span: e.span,
                at_eof: e.at_eof,
            });
            self.dropped_lines.push(e.span.start.line);
            self.lex_flushed += 1;
        }
    }

    fn idx(&self, k: usize) -> usize {
        (self.p + k - 1).min(self.tokens.len() - 1)
    }

    fn la(&mut self, k: usize) -> TokenType {
        let i = self.idx(k);
        self.touch(i);
        self.tokens[i].ty
    }

    fn lt(&mut self, k: usize) -> Token {
        let i = self.idx(k);
        self.touch(i);
        self.tokens[i].clone()
    }

    fn consume(&mut self) -> Token {
        let t = self.tokens[self.p].clone();
        if t.ty != T::Eof {
            self.p += 1;
        }
        self.touch(self.p);
        t
    }

    // ----- error reporting ------------------------------------------------

    fn display(t: &Token) -> String {
        if t.ty == T::Eof {
            "'<EOF>'".to_string()
        } else {
            format!("'{}'", escape_display(&t.text))
        }
    }

    fn report(&mut self, kind: ParseErrorKind, message: String, at: &Token) {
        self.errors.push(ParseError {
            kind,
            message,
            span: at.span,
            at_eof: at.ty == T::Eof,
        });
    }

    fn syntax(&mut self, message: String, at: &Token) -> Abort {
        self.report(ParseErrorKind::Syntax, message, at);
        Abort
    }

    /// `extraneous input` when dropping the current token would leave one
    /// in `expected` next (single-token deletion), else `mismatched input`.
    fn unexpected(&mut self, expected: TokenSet) -> Abort {
        let t = self.lt(1);
        let next = self.la(2);
        let what = if expected.contains(next) {
            "extraneous"
        } else {
            "mismatched"
        };
        self.syntax(
            format!("{what} input {} expecting {expected}", Self::display(&t)),
            &t,
        )
    }

    /// `sync` at a block start or loop entry: nothing when the current token
    /// is in `expected`.
    fn sync(&mut self, expected: TokenSet) -> PResult<()> {
        if expected.contains(self.la(1)) {
            Ok(())
        } else {
            Err(self.unexpected(expected))
        }
    }

    /// `sync` at a loop back edge: ANTLR always reports `extraneous input`.
    fn sync_loop_back(&mut self, expected: TokenSet) -> PResult<()> {
        if expected.contains(self.la(1)) {
            return Ok(());
        }
        let t = self.lt(1);
        Err(self.syntax(
            format!(
                "extraneous input {} expecting {expected}",
                Self::display(&t)
            ),
            &t,
        ))
    }

    /// `match` of a token prediction did not already guarantee: tries
    /// single-token deletion, then insertion (`missing`), then reports the
    /// mismatch. `then` is what may follow the expected token.
    fn expect(&mut self, ty: TokenType, then: TokenSet) -> PResult<Token> {
        if self.la(1) == ty {
            return Ok(self.consume());
        }
        let expected = TokenSet::of(&[ty]);
        let t = self.lt(1);
        if expected.contains(self.la(2)) {
            return Err(self.syntax(
                format!(
                    "extraneous input {} expecting {expected}",
                    Self::display(&t)
                ),
                &t,
            ));
        }
        if then.contains(t.ty) {
            return Err(self.syntax(format!("missing {expected} at {}", Self::display(&t)), &t));
        }
        Err(self.syntax(
            format!(
                "mismatched input {} expecting {expected}",
                Self::display(&t)
            ),
            &t,
        ))
    }

    /// Adaptive prediction for `(',' item)*` followed by `','? close`, with
    /// the current token a `,` or `close`: true to take another item.
    fn predict_another(&mut self, item_start: TokenSet, close: TokenType) -> PResult<bool> {
        if self.la(1) == close {
            return Ok(false);
        }
        let next = self.la(2);
        if item_start.contains(next) {
            Ok(true)
        } else if next == close {
            Ok(false)
        } else {
            let start = self.lt(1);
            let at = self.lt(2);
            // ANTLR reads the rest of the input to print the offending text,
            // reporting any lexer errors left in it first.
            self.touch(self.tokens.len() - 1);
            let text = if at.ty == T::Eof {
                start.text.clone()
            } else {
                format!("{}{}", start.text, at.text)
            };
            Err(self.syntax(
                format!("no viable alternative at input '{}'", escape_display(&text)),
                &at,
            ))
        }
    }

    // ----- rules ----------------------------------------------------------

    fn program(&mut self) -> Vec<Statement> {
        let mut statements = Vec::new();
        let mut entry = true;
        loop {
            let la = self.la(1);
            if la == T::Eof {
                break;
            }
            let start = self.p;
            if !EXPR_START.contains(la) {
                // A token no statement starts with.
                let t = self.lt(1);
                if entry {
                    let _ = self.unexpected(PROGRAM_LOOP);
                } else {
                    let _ = self.sync_loop_back(PROGRAM_LOOP);
                }
                self.dropped_lines.push(t.span.start.line);
                self.resync(start);
                entry = false;
                continue;
            }
            entry = false;
            let errors_before = self.errors.len();
            match self.statement() {
                Ok(stmt) => {
                    let failed = self.errors[errors_before..]
                        .iter()
                        .any(|e| e.kind != ParseErrorKind::Lexer);
                    if !failed {
                        statements.push(stmt);
                    }
                }
                Err(Abort) => self.resync(start),
            }
        }
        statements
    }

    /// Skips to where the next statement can start after an error in the
    /// statement that began at token `start`.
    fn resync(&mut self, start: usize) {
        let delta = |t: &Token| match t.ty {
            T::LParen | T::LBracket | T::LBrace => 1,
            T::RParen | T::RBracket | T::RBrace => -1,
            _ => 0,
        };
        // Bracket depth before each token from `start` on, and whether the
        // depth ever comes back to zero after it.
        let depths: Vec<i32> = self.tokens[start..]
            .iter()
            .scan(0, |d, t| {
                let before = *d;
                *d += delta(t);
                Some(before)
            })
            .collect();
        let mut closes_later = vec![false; depths.len() + 1];
        for k in (0..depths.len()).rev() {
            closes_later[k] = closes_later[k + 1] || (k + 1 < depths.len() && depths[k + 1] <= 0);
        }
        let mut j = self.p.max(start);
        let from = self.tokens[start].span;
        loop {
            let t = &self.tokens[j];
            if t.ty == T::Eof {
                break;
            }
            let k = j - start;
            if j > start
                && t.line_initial
                && EXPR_START.contains(t.ty)
                && (depths[k] <= 0 || !closes_later[k])
            {
                break;
            }
            j += 1;
        }
        self.dropped.push(Span {
            start: from.start,
            end: self.tokens[j].span.start,
        });
        self.p = j;
        self.touch(j);
    }

    fn statement(&mut self) -> PResult<Statement> {
        // Decision 1: assignment when `=` follows an identifier or a path.
        let la1 = self.la(1);
        if matches!(la1, T::Identifier | T::Path) && self.la(2) == T::Assign {
            let t = self.consume();
            let target = if t.ty == T::Identifier {
                Target::Variable {
                    name: t.text,
                    span: t.span,
                }
            } else {
                Target::Path {
                    path: t.text[1..].to_string(),
                    span: t.span,
                }
            };
            self.consume(); // '='
            let value = self.expression()?;
            let span = target.span().to(value.span);
            Ok(Statement {
                kind: StatementKind::Assign { target, value },
                span,
            })
        } else {
            let value = self.expression()?;
            Ok(Statement {
                span: value.span,
                kind: StatementKind::Expr(value),
            })
        }
    }

    fn expression(&mut self) -> PResult<Expr> {
        self.sync(EXPR_START)?;
        let t = self.lt(1);
        match t.ty {
            T::LBracket => self.array(),
            T::LBrace => self.map(),
            T::Path => {
                self.consume();
                Ok(Expr {
                    kind: ExprKind::Path(t.text[1..].to_string()),
                    span: t.span,
                })
            }
            T::Check => self.check(),
            T::SystemIdentifier => self.call(),
            T::Identifier if self.la(2) == T::LParen => self.call(),
            T::Identifier => {
                self.consume();
                Ok(Expr {
                    kind: ExprKind::Variable(t.text),
                    span: t.span,
                })
            }
            T::Underscore => {
                self.consume();
                Ok(Expr {
                    kind: ExprKind::Skipped,
                    span: t.span,
                })
            }
            _ => self.literal(),
        }
    }

    fn array(&mut self) -> PResult<Expr> {
        let open = self.consume();
        let close_set = TokenSet::of(&[T::Comma, T::RBracket]);
        self.sync(EXPR_START.union(TokenSet::of(&[T::RBracket])))?;
        let mut items = Vec::new();
        if EXPR_START.contains(self.la(1)) {
            items.push(self.expression()?);
            self.sync(close_set)?;
            while self.predict_another(EXPR_START, T::RBracket)? {
                self.consume(); // ','
                items.push(self.expression()?);
                self.sync_loop_back(close_set)?;
            }
            if self.la(1) == T::Comma {
                self.consume();
            }
        }
        let close = self.consume(); // ']'
        Ok(Expr {
            kind: ExprKind::List(items),
            span: open.span.to(close.span),
        })
    }

    fn map(&mut self) -> PResult<Expr> {
        let open = self.consume();
        let close_set = TokenSet::of(&[T::Comma, T::RBrace]);
        self.sync(MAP_KEY.union(TokenSet::of(&[T::RBrace])))?;
        let mut entries = Vec::new();
        if MAP_KEY.contains(self.la(1)) {
            entries.push(self.map_entry()?);
            self.sync(close_set)?;
            while self.predict_another(MAP_KEY, T::RBrace)? {
                self.consume(); // ','
                entries.push(self.map_entry()?);
                self.sync_loop_back(close_set)?;
            }
            if self.la(1) == T::Comma {
                self.consume();
            }
        }
        let close = self.consume(); // '}'
        Ok(Expr {
            kind: ExprKind::Map(entries),
            span: open.span.to(close.span),
        })
    }

    fn map_entry(&mut self) -> PResult<MapEntry> {
        let t = self.consume();
        let key = if t.ty == T::Identifier {
            t.text.clone()
        } else {
            string_value(&t)
        };
        self.expect(T::Colon, EXPR_START)?;
        let value = self.expression()?;
        Ok(MapEntry {
            key,
            key_span: t.span,
            value,
        })
    }

    fn check(&mut self) -> PResult<Expr> {
        let t = self.consume();
        let name = t.text[1..].to_string();
        let mut args = Vec::new();
        let mut span = t.span;
        // The parenthesised part is optional, so `sync` passes on anything.
        if self.la(1) == T::LParen {
            self.consume();
            let close_set = TokenSet::of(&[T::Comma, T::RParen]);
            self.sync(EXPR_START.union(TokenSet::of(&[T::RParen])))?;
            if EXPR_START.contains(self.la(1)) {
                args.push(self.expression()?);
                self.sync(close_set)?;
                while self.predict_another(EXPR_START, T::RParen)? {
                    self.consume();
                    args.push(self.expression()?);
                    self.sync_loop_back(close_set)?;
                }
                if self.la(1) == T::Comma {
                    self.consume();
                }
            }
            span = span.to(self.consume().span); // ')'
        }
        Ok(Expr {
            kind: ExprKind::Check { name, args },
            span,
        })
    }

    fn call(&mut self) -> PResult<Expr> {
        let name_tok = self.consume();
        self.expect(T::LParen, EXPR_START.union(TokenSet::of(&[T::RParen])))?;
        let close_set = TokenSet::of(&[T::Comma, T::RParen]);
        self.sync(EXPR_START.union(TokenSet::of(&[T::RParen])))?;
        let mut args = Vec::new();
        if EXPR_START.contains(self.la(1)) {
            args.push(self.arg()?);
            self.sync(close_set)?;
            while self.predict_another(EXPR_START, T::RParen)? {
                self.consume();
                args.push(self.arg()?);
                self.sync_loop_back(close_set)?;
            }
            if self.la(1) == T::Comma {
                self.consume();
            }
        }
        let close = self.consume(); // ')'
        let call = Call {
            name: name_tok.text.clone(),
            name_span: name_tok.span,
            args,
        };
        let span = name_tok.span.to(close.span);
        if call.kind() == CallKind::Template && call.positional().count() < 2 {
            self.report(
                ParseErrorKind::Template,
                "_template helper requires exactly 2 arguments: path and templateComponent."
                    .to_string(),
                &name_tok,
            );
        }
        Ok(Expr {
            kind: ExprKind::Call(call),
            span,
        })
    }

    fn arg(&mut self) -> PResult<Arg> {
        // Decision 19: a keyword argument when `=` follows an identifier.
        if self.la(1) == T::Identifier && self.la(2) == T::Assign {
            let t = self.consume();
            self.consume(); // '='
            let value = self.expression()?;
            return Ok(Arg {
                name: Some((t.text, t.span)),
                value,
            });
        }
        Ok(Arg {
            name: None,
            value: self.expression()?,
        })
    }

    fn literal(&mut self) -> PResult<Expr> {
        let t = self.consume();
        let kind = match t.ty {
            T::Null => ExprKind::Null,
            T::Boolean => ExprKind::Bool(t.text == "true"),
            T::Number => match number_value(&t.text) {
                Some(value) => ExprKind::Number(Number {
                    text: t.text.clone(),
                    value,
                }),
                None => {
                    self.report(
                        ParseErrorKind::NumberOutOfRange,
                        format!("Number literal {} is out of range.", t.text),
                        &t,
                    );
                    ExprKind::Null
                }
            },
            T::Color => ExprKind::Color(t.text.to_ascii_lowercase()),
            _ => ExprKind::String(string_value(&t)),
        };
        Ok(Expr { kind, span: t.span })
    }
}

/// Reads a `NUMBER` token: integer text that fits an `i64` as an integer,
/// anything else as a double. `None` when the double is not finite.
pub fn number_value(text: &str) -> Option<NumberValue> {
    let value: f64 = text.parse().ok()?;
    if !value.is_finite() {
        return None;
    }
    if !text.contains(['.', 'e', 'E']) {
        if let Ok(i) = text.trim_start_matches('+').parse::<i64>() {
            return Some(NumberValue::Int(i));
        }
    }
    Some(NumberValue::Float(value))
}

/// Resolves `\n`, `\r`, `\t`, `\\` and `\"`; any other escape stays as
/// written, backslash included.
pub fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// The value of a string token of any of the four kinds.
fn string_value(t: &Token) -> String {
    let s = t.text.as_str();
    match t.ty {
        T::RawTripleString => s[4..s.len() - 3].to_string(),
        T::RawString => s[2..s.len() - 1].to_string(),
        T::TripleString => unescape(&s[3..s.len() - 3]),
        // A raw line break at either edge of a plain string is dropped.
        _ => unescape(s[1..s.len() - 1].trim_matches(['\r', '\n'])),
    }
}
