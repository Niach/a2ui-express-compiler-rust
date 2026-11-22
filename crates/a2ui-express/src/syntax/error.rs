//! Parse errors, worded as upstream words them.

use super::ast::Span;

/// Why a statement could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParseErrorKind {
    /// A character no lexer rule admits (ANTLR's `token recognition error`).
    Lexer,
    /// The tokens do not fit the grammar (ANTLR's parser errors).
    Syntax,
    /// A number literal no double can hold, such as `1e999`.
    NumberOutOfRange,
    /// `_template(…)` without both a path and a component.
    Template,
}

/// One parse error.
///
/// `message` is the bare message: for [`ParseErrorKind::Lexer`] and
/// [`ParseErrorKind::Syntax`] exactly what ANTLR reports for the same input
/// (`mismatched input '<EOF>' expecting {',', ')'}`), otherwise upstream's
/// own text. [`ParseError::upstream_message`] wraps it the way upstream's
/// compiler does, which is the text a model sees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// The category.
    pub kind: ParseErrorKind,
    /// The bare message.
    pub message: String,
    /// The offending token or characters.
    pub span: Span,
    /// The error is at the end of the input: the text may only be
    /// unfinished. A streaming caller waits for more lines instead of
    /// reporting it.
    pub at_eof: bool,
}

impl ParseError {
    /// 1-based line of the error.
    pub fn line(&self) -> u32 {
        self.span.start.line
    }

    /// 0-based column of the error, as ANTLR counts it.
    pub fn column(&self) -> u32 {
        self.span.start.column
    }

    /// The error as upstream's `ExpressCompiler.compile` raises it.
    ///
    /// - lexer: `Syntax error at line 1:7: token recognition error at: '^'`
    /// - syntax: `Failed to parse expression: Syntax error at line 1:19: mismatched input …`
    /// - number: `Failed to parse expression: Number literal 1e999 is out of range.`
    /// - template: `_template helper requires exactly 2 arguments: path and templateComponent.`
    pub fn upstream_message(&self) -> String {
        let (line, col) = (self.line(), self.column());
        match self.kind {
            ParseErrorKind::Lexer => {
                format!("Syntax error at line {line}:{col}: {}", self.message)
            }
            ParseErrorKind::Syntax => format!(
                "Failed to parse expression: Syntax error at line {line}:{col}: {}",
                self.message
            ),
            ParseErrorKind::NumberOutOfRange => {
                format!("Failed to parse expression: {}", self.message)
            }
            ParseErrorKind::Template => self.message.clone(),
        }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.upstream_message())
    }
}

impl std::error::Error for ParseError {}

impl From<ParseError> for crate::Error {
    fn from(e: ParseError) -> Self {
        crate::Error::Parse(e.upstream_message())
    }
}
