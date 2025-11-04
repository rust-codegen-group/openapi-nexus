//! Serde error extraction utilities

/// Serde error extractor for parsing error messages
pub struct SerdeErrorExtractor<'a> {
    error_msg: &'a str,
}

impl<'a> SerdeErrorExtractor<'a> {
    /// Create a new serde error extractor from an error message
    pub fn new(error_msg: &'a str) -> Self {
        Self { error_msg }
    }

    /// Extract line and column numbers from serde error message
    /// Returns (line, column) where line and column are 1-indexed
    pub fn extract_location(&self) -> (usize, usize) {
        // Try to parse patterns like "at line 760 column 5" or "line 760, column 5"
        let mut line = 0;
        let mut column = 0;

        // Find "line" followed by a number
        if let Some(line_pos) = self.error_msg.find("line") {
            let after_line = &self.error_msg[line_pos + 4..];
            // Skip whitespace
            let after_whitespace = after_line.trim_start();
            // Find the number
            let num_end = after_whitespace
                .char_indices()
                .find(|(_, c)| !c.is_ascii_digit())
                .map(|(i, _)| i)
                .unwrap_or(after_whitespace.len());
            if let Ok(parsed_line) = after_whitespace[..num_end].parse::<usize>() {
                line = parsed_line;
            }
        }

        // Find "column" followed by a number
        if let Some(col_pos) = self.error_msg.find("column") {
            let after_column = &self.error_msg[col_pos + 6..];
            // Skip whitespace
            let after_whitespace = after_column.trim_start();
            // Find the number
            let num_end = after_whitespace
                .char_indices()
                .find(|(_, c)| !c.is_ascii_digit())
                .map(|(i, _)| i)
                .unwrap_or(after_whitespace.len());
            if let Ok(parsed_column) = after_whitespace[..num_end].parse::<usize>() {
                column = parsed_column;
            }
        }

        (line, column)
    }
}
