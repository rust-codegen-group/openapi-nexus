//! Source location information for error reporting

use std::path::PathBuf;

/// Source location information for error reporting
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation {
    pub file_path: Option<PathBuf>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub openapi_path: Option<String>,
}

impl SourceLocation {
    pub fn new() -> Self {
        Self {
            file_path: None,
            line: None,
            column: None,
            openapi_path: None,
        }
    }

    pub fn with_file_path(mut self, path: PathBuf) -> Self {
        self.file_path = Some(path);
        self
    }

    pub fn with_line_column(mut self, line: u32, column: u32) -> Self {
        self.line = Some(line);
        self.column = Some(column);
        self
    }

    pub fn with_openapi_path(mut self, path: String) -> Self {
        self.openapi_path = Some(path);
        self
    }
}

impl Default for SourceLocation {
    fn default() -> Self {
        Self::new()
    }
}
