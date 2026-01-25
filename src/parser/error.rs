//! Parser error types with line/column information.

use pest::RuleType;
use pest::error::Error as PestError;
use thiserror::Error;

/// Error that occurred during parsing.
#[derive(Debug, Error)]
pub enum ParseError {
    /// Syntax error from the pest parser.
    #[error("Parse error at line {line}, column {column}: {message}")]
    Syntax {
        line: usize,
        column: usize,
        message: String,
    },

    /// Unexpected end of input.
    #[error("Unexpected end of input")]
    UnexpectedEof,

    /// Other parser error.
    #[error("Parse error: {0}")]
    Other(String),
}

impl ParseError {
    /// Create a ParseError from a pest error.
    pub fn from_pest_error<R: RuleType>(err: PestError<R>) -> Self {
        let (line, column) = match err.line_col {
            pest::error::LineColLocation::Pos((l, c)) => (l, c),
            pest::error::LineColLocation::Span((l, c), _) => (l, c),
        };

        ParseError::Syntax {
            line,
            column,
            message: err.variant.message().to_string(),
        }
    }
}
