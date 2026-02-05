//! Validation/linting for Mermaid input.
//!
//! Provides structured diagnostic output suitable for LLM repair loops
//! and CI/CD integration.

use std::fmt;

use serde::Serialize;

/// Severity level of a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Fatal error - diagram cannot be parsed.
    Error,
    /// Warning - diagram is valid but has issues.
    Warning,
}

/// A single diagnostic message.
#[derive(Debug, Clone, Serialize)]
pub struct LintDiagnostic {
    /// Error or warning.
    pub severity: Severity,
    /// Source line number (1-based), if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    /// Source column number (1-based), if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
    /// Human-readable message.
    pub message: String,
}

impl fmt::Display for LintDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let severity = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        match (self.line, self.column) {
            (Some(line), Some(col)) => {
                write!(
                    f,
                    "{}: line {}, column {}: {}",
                    severity, line, col, self.message
                )
            }
            (Some(line), None) => {
                write!(f, "{}: line {}: {}", severity, line, self.message)
            }
            _ => {
                write!(f, "{}: {}", severity, self.message)
            }
        }
    }
}

/// Result of linting an input.
#[derive(Debug, Clone, Serialize)]
pub struct LintResult {
    /// Whether the input is valid (no errors).
    pub valid: bool,
    /// Parse errors.
    pub errors: Vec<LintDiagnostic>,
    /// Warnings (valid but has issues).
    pub warnings: Vec<LintDiagnostic>,
}

impl LintResult {
    /// Check if the input is valid (no errors).
    pub fn is_valid(&self) -> bool {
        self.valid
    }

    /// Check if there are any warnings.
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }

    /// Suggested exit code for CLI.
    /// 0 = valid (with or without warnings)
    /// 1 = invalid (parse errors)
    pub fn exit_code(&self) -> i32 {
        if self.valid { 0 } else { 1 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lint_result_valid() {
        let result = LintResult {
            valid: true,
            errors: vec![],
            warnings: vec![],
        };
        assert!(result.is_valid());
        assert!(!result.has_warnings());
        assert_eq!(result.exit_code(), 0);
    }

    #[test]
    fn test_lint_result_with_errors() {
        let result = LintResult {
            valid: false,
            errors: vec![LintDiagnostic {
                severity: Severity::Error,
                line: Some(3),
                column: Some(5),
                message: "expected identifier".to_string(),
            }],
            warnings: vec![],
        };
        assert!(!result.is_valid());
        assert!(!result.has_warnings());
        assert_eq!(result.exit_code(), 1);
    }

    #[test]
    fn test_lint_result_with_warnings() {
        let result = LintResult {
            valid: true,
            errors: vec![],
            warnings: vec![LintDiagnostic {
                severity: Severity::Warning,
                line: Some(5),
                column: Some(1),
                message: "classDef statements are parsed but ignored".to_string(),
            }],
        };
        assert!(result.is_valid());
        assert!(result.has_warnings());
        assert_eq!(result.exit_code(), 0);
    }

    #[test]
    fn test_lint_diagnostic_display() {
        let d = LintDiagnostic {
            severity: Severity::Error,
            line: Some(3),
            column: Some(5),
            message: "expected identifier".to_string(),
        };
        let display = format!("{}", d);
        assert!(display.contains("error"));
        assert!(display.contains("line 3"));
        assert!(display.contains("column 5"));
    }

    #[test]
    fn test_lint_diagnostic_display_no_position() {
        let d = LintDiagnostic {
            severity: Severity::Warning,
            line: None,
            column: None,
            message: "something".to_string(),
        };
        let display = format!("{}", d);
        assert!(display.contains("warning"));
        assert!(display.contains("something"));
    }

    #[test]
    fn test_lint_result_json_serialization() {
        let result = LintResult {
            valid: false,
            errors: vec![LintDiagnostic {
                severity: Severity::Error,
                line: Some(3),
                column: Some(5),
                message: "expected identifier".to_string(),
            }],
            warnings: vec![],
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"valid\":false"));
        assert!(json.contains("\"severity\":\"error\""));
    }
}
