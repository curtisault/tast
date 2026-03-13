use crate::util::span::Span;

/// A secondary annotation attached to a primary error.
///
/// Points to a related source location (e.g. the first definition of a
/// duplicate name, or the opening brace of an unclosed block).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecondaryLabel {
    /// Description of why this location is relevant.
    pub message: String,
    /// Source location to highlight.
    pub span: Span,
}

/// An error encountered during parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// Human-readable error description.
    pub message: String,
    /// Source location where the error occurred.
    pub span: Span,
    /// Optional secondary annotations for related source locations.
    pub secondary: Vec<SecondaryLabel>,
    /// Optional help text suggesting how to fix the error.
    pub help: Option<String>,
}

impl ParseError {
    /// Create a new error with no secondary labels or help text.
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
            secondary: vec![],
            help: None,
        }
    }

    /// Add a secondary label pointing to a related source location.
    #[must_use]
    pub fn with_secondary(mut self, message: impl Into<String>, span: Span) -> Self {
        self.secondary.push(SecondaryLabel {
            message: message.into(),
            span,
        });
        self
    }

    /// Add help text suggesting how to fix the error.
    #[must_use]
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.span.line, self.span.col, self.message)
    }
}

impl std::error::Error for ParseError {}
