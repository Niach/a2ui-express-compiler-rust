//! The Express syntax tree.
//!
//! The shapes follow upstream's visitor (`visitor.py` / `visitor.ts`):
//! literals, lists and maps are values; paths, variables, the skipped
//! argument, checks and calls are nodes of their own, so a map literal is
//! never mistaken for a reference, whatever its keys are. Unlike upstream,
//! every node carries a [`Span`], and call arguments keep their order with
//! positional and keyword arguments interleaved as written.

/// A position in the source.
///
/// `line` is 1-based and `column` 0-based, counted in Unicode scalar values,
/// which is how ANTLR reports positions; upstream's error text
/// (`Syntax error at line 3:14: …`) uses the same numbers. `offset` is the
/// byte offset into the text that was parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Pos {
    /// 1-based line.
    pub line: u32,
    /// 0-based column in Unicode scalar values.
    pub column: u32,
    /// Byte offset.
    pub offset: usize,
}

impl Pos {
    /// The start of a text: line 1, column 0.
    pub const START: Pos = Pos {
        line: 1,
        column: 0,
        offset: 0,
    };
}

/// The source range of a token or node: `start` inclusive, `end` exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    /// The first character.
    pub start: Pos,
    /// Just past the last character.
    pub end: Pos,
}

impl Span {
    /// The span from the start of `self` to the end of `other`.
    pub fn to(self, other: Span) -> Span {
        Span {
            start: self.start,
            end: other.end,
        }
    }
}

/// A parsed block: its statements in source order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Program {
    /// The statements that parsed. A statement with an error is left out.
    pub statements: Vec<Statement>,
}

/// One statement: an assignment or a standalone expression.
#[derive(Debug, Clone, PartialEq)]
pub struct Statement {
    /// What the statement is.
    pub kind: StatementKind,
    /// From its first token to its last.
    pub span: Span,
}

/// The two statement forms of the grammar.
#[derive(Debug, Clone, PartialEq)]
pub enum StatementKind {
    /// `name = value` or `$/path = value`.
    Assign {
        /// What is assigned.
        target: Target,
        /// The value.
        value: Expr,
    },
    /// A standalone expression, normally a call such as `surface("main")`.
    Expr(Expr),
}

/// The left-hand side of an assignment.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    /// A variable, `name = …`.
    Variable {
        /// The variable name.
        name: String,
        /// The name's span.
        span: Span,
    },
    /// A data path, `$/a/b = …`. `path` is the text after the `$`.
    Path {
        /// The path without its `$`; empty for a bare `$`.
        path: String,
        /// The token's span.
        span: Span,
    },
}

impl Target {
    /// The target as written: the variable name, or the path with its `$`.
    ///
    /// This is the key upstream stores the assignment under.
    pub fn text(&self) -> String {
        match self {
            Target::Variable { name, .. } => name.clone(),
            Target::Path { path, .. } => format!("${path}"),
        }
    }

    /// The target's span.
    pub fn span(&self) -> Span {
        match self {
            Target::Variable { span, .. } | Target::Path { span, .. } => *span,
        }
    }
}

/// How a statement reads to the compiler, as upstream's `_build_units` sorts
/// statements.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StatementForm<'a> {
    /// `name = value` or `$/path = value`.
    Assign(&'a Target, &'a Expr),
    /// `surface("id", …)`: opens a surface scope.
    Surface(&'a Call),
    /// `deleteSurface("id")`.
    DeleteSurface(&'a Call),
    /// Any other standalone call, compiled to `callRendererFunction`.
    Call(&'a Call),
    /// A standalone expression that is not a call. Upstream ignores these.
    Other(&'a Expr),
}

impl Statement {
    /// Sorts the statement the way the compiler groups statements into units.
    pub fn form(&self) -> StatementForm<'_> {
        match &self.kind {
            StatementKind::Assign { target, value } => StatementForm::Assign(target, value),
            StatementKind::Expr(expr) => match &expr.kind {
                ExprKind::Call(call) if call.name == "surface" => StatementForm::Surface(call),
                ExprKind::Call(call) if call.name == "deleteSurface" => {
                    StatementForm::DeleteSurface(call)
                }
                ExprKind::Call(call) => StatementForm::Call(call),
                _ => StatementForm::Other(expr),
            },
        }
    }
}

/// A value or node.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    /// What the expression is.
    pub kind: ExprKind,
    /// From its first token to its last.
    pub span: Span,
}

/// The expression forms.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A number literal.
    Number(Number),
    /// A string literal of any of the four kinds, unescaped.
    String(String),
    /// `$/a/b`, `$name` or `$`: the text after the `$`.
    Path(String),
    /// A reference to a variable.
    Variable(String),
    /// `_`, which skips a positional argument.
    Skipped,
    /// `?name` or `?name(args)`. Upstream treats `?name` and `?name()` alike.
    Check {
        /// The check name without its `?`.
        name: String,
        /// Its arguments.
        args: Vec<Expr>,
    },
    /// A component, function or helper call.
    Call(Call),
    /// `[a, b]`.
    List(Vec<Expr>),
    /// `{key: value}`. Keys keep their order; a repeated key appears twice
    /// here, and upstream keeps the last one.
    Map(Vec<MapEntry>),
}

/// A number literal: the token text and the value it reads as.
#[derive(Debug, Clone, PartialEq)]
pub struct Number {
    /// The token as written, such as `+3`, `.5` or `1E-2`.
    pub text: String,
    /// The value.
    pub value: NumberValue,
}

/// The value of a number literal.
///
/// Integer text (no `.`, `e` or `E`) that fits an `i64` reads as an integer,
/// as upstream Python reads it as an `int`; anything else is a finite double.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NumberValue {
    /// An integer.
    Int(i64),
    /// A finite double.
    Float(f64),
}

impl Number {
    /// The value as a JSON number.
    pub fn to_json(&self) -> serde_json::Number {
        match self.value {
            NumberValue::Int(i) => i.into(),
            NumberValue::Float(f) => {
                serde_json::Number::from_f64(f).expect("number literals are finite")
            }
        }
    }
}

/// A call: `Name(args)` or `@system(args)`.
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    /// The function name, with its `@` for a system function.
    pub name: String,
    /// The name's span.
    pub name_span: Span,
    /// The arguments in the order written.
    pub args: Vec<Arg>,
}

/// One call argument.
#[derive(Debug, Clone, PartialEq)]
pub struct Arg {
    /// The keyword and its span for `name = value`; `None` when positional.
    pub name: Option<(String, Span)>,
    /// The value.
    pub value: Expr,
}

/// One map entry.
#[derive(Debug, Clone, PartialEq)]
pub struct MapEntry {
    /// The key, whether written as an identifier or a string.
    pub key: String,
    /// The key's span.
    pub key_span: Span,
    /// The value.
    pub value: Expr,
}

/// Calls the compiler reads as language constructs rather than catalog
/// functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallKind<'a> {
    /// `_template($/list, component)`: a child list template.
    Template,
    /// `Event(name, context?, …)`: an action event.
    Event,
    /// `formatString(…)`, the string interpolation function.
    FormatString,
    /// `@name(…)`: a system function; the name is without its `@`.
    System(&'a str),
    /// `surface(…)`.
    Surface,
    /// `deleteSurface(…)`.
    DeleteSurface,
    /// A catalog component or function.
    Catalog,
}

impl Call {
    /// What the call is to the compiler.
    pub fn kind(&self) -> CallKind<'_> {
        match self.name.as_str() {
            "_template" => CallKind::Template,
            "Event" => CallKind::Event,
            "formatString" => CallKind::FormatString,
            "surface" => CallKind::Surface,
            "deleteSurface" => CallKind::DeleteSurface,
            name => match name.strip_prefix('@') {
                Some(system) => CallKind::System(system),
                None => CallKind::Catalog,
            },
        }
    }

    /// The positional arguments, in order.
    pub fn positional(&self) -> impl Iterator<Item = &Expr> {
        self.args
            .iter()
            .filter(|a| a.name.is_none())
            .map(|a| &a.value)
    }

    /// The keyword arguments, in order.
    pub fn keywords(&self) -> impl Iterator<Item = (&str, &Expr)> {
        self.args
            .iter()
            .filter_map(|a| a.name.as_ref().map(|(n, _)| (n.as_str(), &a.value)))
    }

    /// The argument given by keyword `key`, or else at position `index`, as
    /// upstream's `_call_arg` looks one up. The last of repeated keywords wins.
    pub fn arg(&self, index: usize, key: &str) -> Option<&Expr> {
        self.keywords()
            .filter(|(k, _)| *k == key)
            .last()
            .map(|(_, v)| v)
            .or_else(|| self.positional().nth(index))
    }
}
