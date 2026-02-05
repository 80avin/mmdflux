//! Validation/linting for Mermaid input.
//!
//! Provides structured diagnostic output suitable for LLM repair loops
//! and CI/CD integration.

use std::fmt;

use serde::Serialize;

use crate::parser::{ParseError, detect_diagram_type, parse_flowchart};

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

    /// Serialize the lint result to JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("LintResult serialization should not fail")
    }
}

/// Lint/validate Mermaid input, returning structured diagnostics.
///
/// Attempts to parse the input and reports any errors or warnings.
/// This is designed for LLM repair loops: the output provides enough
/// context for an LLM to fix syntax errors.
pub fn lint(input: &str) -> LintResult {
    if detect_diagram_type(input).is_none() {
        return LintResult {
            valid: false,
            errors: vec![LintDiagnostic {
                severity: Severity::Error,
                line: Some(1),
                column: Some(1),
                message: "Unknown or missing diagram type. Expected 'graph' or 'flowchart' header."
                    .to_string(),
            }],
            warnings: vec![],
        };
    }

    match parse_flowchart(input) {
        Ok(_) => {
            let warnings = collect_unsupported_warnings(input);
            LintResult {
                valid: true,
                errors: vec![],
                warnings,
            }
        }
        Err(parse_error) => {
            let diagnostic = parse_error_to_diagnostic(&parse_error);
            LintResult {
                valid: false,
                errors: vec![diagnostic],
                warnings: vec![],
            }
        }
    }
}

fn collect_unsupported_warnings(input: &str) -> Vec<LintDiagnostic> {
    let mut warnings = Vec::new();

    for (line_num, line) in input.lines().enumerate() {
        let trimmed = line.trim();

        if trimmed.starts_with("classDef ") {
            warnings.push(LintDiagnostic {
                severity: Severity::Warning,
                line: Some(line_num + 1),
                column: Some(1),
                message: "classDef statements are parsed but ignored in rendering".to_string(),
            });
        } else if trimmed.starts_with("style ") {
            warnings.push(LintDiagnostic {
                severity: Severity::Warning,
                line: Some(line_num + 1),
                column: Some(1),
                message: "style statements are parsed but ignored in rendering".to_string(),
            });
        } else if trimmed.starts_with("click ") {
            warnings.push(LintDiagnostic {
                severity: Severity::Warning,
                line: Some(line_num + 1),
                column: Some(1),
                message: "click statements are not applicable in text/ASCII output".to_string(),
            });
        } else if trimmed.starts_with("linkStyle ") {
            warnings.push(LintDiagnostic {
                severity: Severity::Warning,
                line: Some(line_num + 1),
                column: Some(1),
                message: "linkStyle statements are parsed but ignored in rendering".to_string(),
            });
        } else if trimmed.starts_with("class ") && !trimmed.starts_with("classDef") {
            warnings.push(LintDiagnostic {
                severity: Severity::Warning,
                line: Some(line_num + 1),
                column: Some(1),
                message: "class statements are parsed but ignored in rendering".to_string(),
            });
        }
    }

    warnings
}

fn parse_error_to_diagnostic(err: &ParseError) -> LintDiagnostic {
    match err {
        ParseError::Syntax {
            line,
            column,
            message,
        } => LintDiagnostic {
            severity: Severity::Error,
            line: Some(*line),
            column: Some(*column),
            message: message.clone(),
        },
        ParseError::UnexpectedEof => LintDiagnostic {
            severity: Severity::Error,
            line: None,
            column: None,
            message: "Unexpected end of input".to_string(),
        },
        ParseError::Other(msg) => LintDiagnostic {
            severity: Severity::Error,
            line: None,
            column: None,
            message: msg.clone(),
        },
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

    #[test]
    fn test_lint_valid_input() {
        let result = lint("graph TD\nA --> B\n");
        assert!(result.is_valid());
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_lint_invalid_syntax() {
        let result = lint("graph TD\nA --> --> B\n");
        assert!(!result.is_valid());
        assert!(!result.errors.is_empty());
        assert_eq!(result.errors[0].severity, Severity::Error);
        assert!(result.errors[0].line.is_some());
    }

    #[test]
    fn test_lint_empty_input() {
        let result = lint("");
        assert!(!result.is_valid());
    }

    #[test]
    fn test_lint_no_header() {
        let result = lint("A --> B\n");
        assert!(!result.is_valid());
    }

    #[test]
    fn test_lint_valid_complex() {
        let result = lint("graph LR\nA[Start] -->|yes| B(Process)\nB -.-> C{Decision}\n");
        assert!(result.is_valid());
    }

    #[test]
    fn test_lint_warns_on_classdef() {
        let result = lint("graph TD\nA --> B\nclassDef warning fill:#ff0\n");
        assert!(result.is_valid());
        assert!(result.has_warnings());
        let w = &result.warnings[0];
        assert_eq!(w.severity, Severity::Warning);
        assert!(w.message.contains("classDef"));
    }

    #[test]
    fn test_lint_warns_on_style() {
        let result = lint("graph TD\nA --> B\nstyle A fill:#f9f\n");
        assert!(result.is_valid());
        assert!(result.has_warnings());
        assert!(result.warnings[0].message.contains("style"));
    }

    #[test]
    fn test_lint_warns_on_click() {
        let result = lint("graph TD\nA --> B\nclick A callback\n");
        assert!(result.is_valid());
        assert!(result.has_warnings());
        assert!(result.warnings[0].message.contains("click"));
    }

    #[test]
    fn test_lint_warns_on_linkstyle() {
        let result = lint("graph TD\nA --> B\nlinkStyle 0 stroke:#ff3\n");
        assert!(result.is_valid());
        assert!(result.has_warnings());
        assert!(result.warnings[0].message.contains("linkStyle"));
    }

    #[test]
    fn test_lint_no_warnings_for_clean_input() {
        let result = lint("graph TD\nA[Start] --> B[End]\n");
        assert!(result.is_valid());
        assert!(!result.has_warnings());
    }

    #[test]
    fn test_lint_multiple_warnings() {
        let result =
            lint("graph TD\nA --> B\nstyle A fill:#f9f\nclassDef x fill:#0f0\nclass A x\n");
        assert!(result.is_valid());
        assert!(result.warnings.len() >= 2);
    }
}
