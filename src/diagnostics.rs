use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Warning,
    Error,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Warning => write!(f, "warning"),
            Severity::Error => write!(f, "error"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceLocation {
    pub line: usize,
    pub column: usize,
    pub offset: usize,
    pub length: usize,
}

impl SourceLocation {
    pub fn new(line: usize, column: usize, offset: usize, length: usize) -> Self {
        Self {
            line,
            column,
            offset,
            length,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixSuggestion {
    pub message: String,
    pub replacement: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub rule: String,
    pub message: String,
    pub severity: Severity,
    pub file_path: PathBuf,
    pub location: Option<SourceLocation>,
    pub suggestion: Option<FixSuggestion>,
    pub code_snippet: Option<String>,
}

impl Diagnostic {
    pub fn error(
        rule: impl Into<String>,
        message: impl Into<String>,
        file_path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            rule: rule.into(),
            message: message.into(),
            severity: Severity::Error,
            file_path: file_path.into(),
            location: None,
            suggestion: None,
            code_snippet: None,
        }
    }

    pub fn warning(
        rule: impl Into<String>,
        message: impl Into<String>,
        file_path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            rule: rule.into(),
            message: message.into(),
            severity: Severity::Warning,
            file_path: file_path.into(),
            location: None,
            suggestion: None,
            code_snippet: None,
        }
    }

    pub fn with_location(mut self, location: SourceLocation) -> Self {
        self.location = Some(location);
        self
    }

    pub fn with_suggestion(
        mut self,
        message: impl Into<String>,
        replacement: Option<String>,
    ) -> Self {
        self.suggestion = Some(FixSuggestion {
            message: message.into(),
            replacement,
        });
        self
    }

    pub fn with_snippet(mut self, snippet: impl Into<String>) -> Self {
        self.code_snippet = Some(snippet.into());
        self
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct LintReport {
    pub total_files_scanned: usize,
    pub diagnostics: Vec<Diagnostic>,
    pub error_count: usize,
    pub warning_count: usize,
    pub elapsed_ns: u128,
}

impl LintReport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_diagnostic(&mut self, diagnostic: Diagnostic) {
        match diagnostic.severity {
            Severity::Error => self.error_count += 1,
            Severity::Warning => self.warning_count += 1,
        }
        self.diagnostics.push(diagnostic);
    }

    pub fn extend(&mut self, other: LintReport) {
        self.total_files_scanned += other.total_files_scanned;
        self.error_count += other.error_count;
        self.warning_count += other.warning_count;
        self.diagnostics.extend(other.diagnostics);
        self.elapsed_ns += other.elapsed_ns;
    }

    pub fn has_errors(&self) -> bool {
        self.error_count > 0
    }
}
