//! Compile errors, typed and worded as upstream's `errors.ts` words them.

use thiserror::Error;

use crate::syntax::{ParseError, ParseErrorKind};

/// The suite's two error categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    /// The block cannot be read (`ExpressParseError`).
    Parse,
    /// The block reads, but the catalog refuses it (`ExpressValidationError`).
    Validation,
}

/// What went wrong, mirroring upstream's error classes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileErrorKind {
    /// A lexer or parser error (`ExpressSyntaxError` and the parse errors it
    /// is wrapped in).
    Syntax {
        /// 1-based line.
        line: u32,
        /// 0-based column.
        column: u32,
    },
    /// Another error in reading the block (`ExpressParseError`), such as a
    /// malformed `_template` or an out-of-range number.
    Parse,
    /// No component and no data in the block, or no `root` to update
    /// (`ExpressUndefinedRootError`, a parse error upstream).
    UndefinedRoot {
        /// The missing id.
        target: String,
    },
    /// Any other validation error (`ExpressValidationError`).
    Validation,
    /// A component no active catalog defines.
    UnknownComponent {
        /// The component.
        component: String,
    },
    /// A function no active catalog defines.
    UnknownFunction {
        /// The function.
        function: String,
    },
    /// A required property is not given.
    MissingRequiredProperty {
        /// The component.
        component: String,
        /// The property.
        property: String,
    },
    /// A property the component does not declare.
    UnknownProperty {
        /// The component.
        component: String,
        /// The property.
        property: String,
    },
    /// A property given twice (positionally and by keyword).
    DuplicateProperty {
        /// The component.
        component: String,
        /// The property.
        property: String,
    },
    /// An argument the function does not declare.
    InvalidParam {
        /// The function.
        function: String,
        /// The argument.
        param: String,
    },
    /// An argument given twice.
    DuplicateParam {
        /// The function.
        function: String,
        /// The argument.
        param: String,
    },
    /// A `$path` where the schema admits none (a static property).
    ForbiddenDatabinding {
        /// The component.
        component: String,
        /// The top-level property the binding sits in.
        property: String,
    },
    /// Two components with one id.
    IdCollision {
        /// The id.
        id: String,
    },
    /// While streaming, a value refers to a variable that is not assigned
    /// yet. The streaming compiler holds the statement back; it never
    /// reaches a caller.
    Pending {
        /// The variable.
        variable: String,
    },
}

/// An error compiling an Express block.
///
/// `message` is upstream's message; `help`, when set, is upstream's
/// `helpMessage`, a hint for the model on how to repair the block.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
pub struct CompileError {
    /// What went wrong.
    pub kind: CompileErrorKind,
    /// The message.
    pub message: String,
    /// A repair hint.
    pub help: Option<String>,
}

impl CompileError {
    pub(crate) fn new(kind: CompileErrorKind, message: impl Into<String>) -> Self {
        CompileError {
            kind,
            message: message.into(),
            help: None,
        }
    }

    pub(crate) fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// A plain validation error.
    pub(crate) fn validation(message: impl Into<String>) -> Self {
        CompileError::new(CompileErrorKind::Validation, message)
    }

    /// A plain parse error.
    pub(crate) fn parse(message: impl Into<String>) -> Self {
        CompileError::new(CompileErrorKind::Parse, message)
    }

    pub(crate) fn undefined_root(target: &str) -> Self {
        CompileError::new(
            CompileErrorKind::UndefinedRoot {
                target: target.to_owned(),
            },
            format!("Root target '{target}' is not defined."),
        )
        .with_help(format!(
            "Ensure root component '{target}' is assigned in your Express DSL."
        ))
    }

    pub(crate) fn missing_required(component: &str, property: &str) -> Self {
        CompileError::new(
            CompileErrorKind::MissingRequiredProperty {
                component: component.to_owned(),
                property: property.to_owned(),
            },
            format!("Component '{component}' is missing required property '{property}'."),
        )
    }

    pub(crate) fn unknown_property(component: &str, property: &str, valid: &[String]) -> Self {
        let help = if valid.is_empty() {
            format!("Component '{component}' has no configurable properties.")
        } else {
            let mut sorted = valid.to_vec();
            sorted.sort();
            format!(
                "Component '{component}' accepts properties: {}.",
                sorted.join(", ")
            )
        };
        CompileError::new(
            CompileErrorKind::UnknownProperty {
                component: component.to_owned(),
                property: property.to_owned(),
            },
            format!("Property '{property}' is not a valid property of component '{component}'."),
        )
        .with_help(help)
    }

    pub(crate) fn duplicate_property(component: &str, property: &str) -> Self {
        CompileError::new(
            CompileErrorKind::DuplicateProperty {
                component: component.to_owned(),
                property: property.to_owned(),
            },
            format!(
                "Property '{property}' of component '{component}' was provided multiple times."
            ),
        )
        .with_help(format!(
            "Remove duplicate specification of property '{property}' for component '{component}'."
        ))
    }

    pub(crate) fn invalid_param(function: &str, param: &str, valid: &[String]) -> Self {
        let help = if valid.is_empty() {
            format!("Function '{function}' accepts no parameters.")
        } else {
            let mut sorted = valid.to_vec();
            sorted.sort();
            format!(
                "Function '{function}' accepts parameters: {}.",
                sorted.join(", ")
            )
        };
        CompileError::new(
            CompileErrorKind::InvalidParam {
                function: function.to_owned(),
                param: param.to_owned(),
            },
            format!("Argument '{param}' is not a valid parameter of function '{function}'."),
        )
        .with_help(help)
    }

    pub(crate) fn duplicate_param(function: &str, param: &str) -> Self {
        CompileError::new(
            CompileErrorKind::DuplicateParam {
                function: function.to_owned(),
                param: param.to_owned(),
            },
            format!("Argument '{param}' of function '{function}' was provided multiple times."),
        )
        .with_help(format!(
            "Remove duplicate specification of parameter '{param}' for function '{function}'."
        ))
    }

    pub(crate) fn forbidden_binding(component: &str, property: &str) -> Self {
        CompileError::new(
            CompileErrorKind::ForbiddenDatabinding {
                component: component.to_owned(),
                property: property.to_owned(),
            },
            format!(
                "Property '{property}' of component '{component}' does not support dynamic data bindings (paths)."
            ),
        )
        .with_help(format!(
            "Property '{property}' on '{component}' must be a static literal value, not a dynamic databinding ($path)."
        ))
    }

    pub(crate) fn pending(variable: &str) -> Self {
        CompileError::new(
            CompileErrorKind::Pending {
                variable: variable.to_owned(),
            },
            format!("Variable '{variable}' is not assigned yet."),
        )
    }

    /// Whether this is a pending variable (streaming only).
    pub(crate) fn is_pending(&self) -> bool {
        matches!(self.kind, CompileErrorKind::Pending { .. })
    }

    pub(crate) fn id_collision(id: &str) -> Self {
        CompileError::new(
            CompileErrorKind::IdCollision { id: id.to_owned() },
            format!(
                "Generated inline component ID '{id}' collides with an existing component or variable ID."
            ),
        )
    }

    /// The suite category: parse or validation.
    pub fn category(&self) -> Category {
        match self.kind {
            CompileErrorKind::Syntax { .. }
            | CompileErrorKind::Parse
            | CompileErrorKind::UndefinedRoot { .. } => Category::Parse,
            _ => Category::Validation,
        }
    }
}

impl From<ParseError> for CompileError {
    fn from(e: ParseError) -> Self {
        let message = e.upstream_message();
        let kind = match e.kind {
            ParseErrorKind::Lexer | ParseErrorKind::Syntax => CompileErrorKind::Syntax {
                line: e.line(),
                column: e.column(),
            },
            ParseErrorKind::NumberOutOfRange | ParseErrorKind::Template => CompileErrorKind::Parse,
        };
        CompileError::new(kind, message)
    }
}

impl From<CompileError> for crate::Error {
    fn from(e: CompileError) -> Self {
        match e.category() {
            Category::Parse => crate::Error::Parse(e.message),
            Category::Validation => crate::Error::Validation(e.message),
        }
    }
}

/// A validation error from the schema module, typed where upstream types it.
pub(crate) fn from_schema_error(e: crate::Error) -> CompileError {
    let message = match e {
        crate::Error::Parse(m) => return CompileError::parse(m),
        crate::Error::Validation(m) => m,
    };
    let unknown = |label: &str| {
        message
            .strip_prefix(&format!("{label} '"))?
            .strip_suffix("' is not defined in the catalog.")
            .map(str::to_owned)
    };
    if let Some(component) = unknown("Component") {
        return CompileError::new(CompileErrorKind::UnknownComponent { component }, message);
    }
    if let Some(function) = unknown("Function") {
        return CompileError::new(CompileErrorKind::UnknownFunction { function }, message);
    }
    CompileError::validation(message)
}
