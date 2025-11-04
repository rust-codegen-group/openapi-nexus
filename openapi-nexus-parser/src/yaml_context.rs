//! YAML context extraction utilities for error reporting

/// YAML context extractor for error reporting
pub struct YamlContextExtractor<'a> {
    lines: Vec<&'a str>,
}

impl<'a> YamlContextExtractor<'a> {
    /// Create a new YAML context extractor from content
    pub fn new(content: &'a str) -> Self {
        let lines: Vec<&str> = content.lines().collect();
        Self { lines }
    }

    /// Extract YAML context around an error location
    /// line_number is 1-indexed (as reported by serde errors)
    pub fn extract_context(&self, line_number: usize, column: usize) -> Vec<String> {
        let total_lines = self.lines.len();

        if line_number == 0 || line_number > total_lines {
            return vec![format!(
                "Unable to extract context (line {} out of range, file has {} lines)",
                line_number, total_lines
            )];
        }

        // Extract 3-5 lines before and after the error
        let context_before = 3;
        let context_after = 3;
        // Convert 1-indexed line_number to 0-indexed array index
        let error_line_idx = line_number - 1;
        let start_line = error_line_idx.saturating_sub(context_before);
        let end_line = (error_line_idx + context_after + 1).min(total_lines);

        let mut context = Vec::new();
        context.push("Error location:".to_string());
        context.push(String::new());

        // Show context lines
        for i in start_line..end_line {
            let line_num = i + 1;
            let line = self.lines[i];
            let is_error_line = line_num == line_number;

            if is_error_line {
                context.push(format!("> {} | {}", line_num, line));
                // Show column indicator if column is valid
                if column > 0 && column <= line.len() {
                    let indent = format!("> {} | ", line_num).len() + column - 1;
                    context.push(format!("  {}^", " ".repeat(indent)));
                }
            } else {
                context.push(format!("  {} | {}", line_num, line));
            }
        }

        // Extract schema name if possible (look for patterns like "SchemaName:" before the error)
        if let Some(schema_name) = self.extract_schema_name(error_line_idx) {
            context.push(String::new());
            context.push(format!("Schema name: {}", schema_name));
        }

        // Extract and show the actual value found at the error location
        if let Some(actual_value) = self.extract_actual_value(error_line_idx, column) {
            context.push(String::new());
            context.push(format!(
                "Actual content at error location: {}",
                actual_value
            ));
        }

        // Analyze the error line to provide specific guidance
        let error_line = if error_line_idx < self.lines.len() {
            self.lines[error_line_idx].trim()
        } else {
            ""
        };

        // Check for common issues and provide specific suggestions
        let issue_suggestion = Self::detect_common_issues(error_line);
        if !issue_suggestion.is_empty() {
            context.push(String::new());
            context.push(format!("Issue detected: {}", issue_suggestion));
        }

        // Add helpful suggestion with context about what was expected
        context.push(String::new());
        context.push("Expected: components.schemas entries must be either:".to_string());
        context
            .push("  - A $ref string (e.g., $ref: '#/components/schemas/OtherSchema')".to_string());
        context.push(
            "  - A schema object with 'type' field (e.g., type: object, type: string)".to_string(),
        );
        context.push("  - null values are not allowed".to_string());

        // Add indentation information if relevant
        let error_indent = if error_line_idx < self.lines.len() {
            self.lines[error_line_idx]
                .chars()
                .take_while(|c| *c == ' ')
                .count()
        } else {
            0
        };
        if error_indent > 0 {
            context.push(String::new());
            context.push(format!(
                "Error at indentation level: {} spaces",
                error_indent
            ));
        }

        context
    }

    /// Try to extract schema name from YAML context
    /// error_line_idx is 0-indexed
    pub fn extract_schema_name(&self, error_line_idx: usize) -> Option<String> {
        // Look backwards from error line to find a schema name
        // We need to find the key that's directly under components.schemas
        let error_line = if error_line_idx < self.lines.len() {
            self.lines[error_line_idx]
        } else {
            return None;
        };

        // Count indentation of error line (spaces at start)
        let error_indent = error_line.chars().take_while(|c| *c == ' ').count();

        // First, find the "schemas:" key to determine the expected indentation level
        // for schema names (should be schemas_indent + 2)
        let mut schemas_indent = None;
        for i in (0..error_line_idx).rev() {
            let line = self.lines[i];
            let trimmed = line.trim();
            if trimmed == "schemas:" || trimmed.starts_with("schemas:") {
                schemas_indent = Some(line.chars().take_while(|c| *c == ' ').count());
                break;
            }
        }

        // If we found schemas:, look for the schema name at the correct indentation level
        // Schema names should be at schemas_indent + 2 spaces (one level deeper than schemas:)
        let expected_schema_indent = schemas_indent.map(|indent| indent + 2);

        // Look backwards for a schema name at the correct indentation level
        for i in (0..error_line_idx).rev() {
            let line = self.lines[i];
            let line_indent = line.chars().take_while(|c| *c == ' ').count();

            // Skip if indentation is too high (nested inside a schema)
            if line_indent >= error_indent {
                continue;
            }

            // If we have a schemas: indent, prefer keys at that exact level
            // Otherwise, look for keys with less indentation than error line
            let is_correct_level = if let Some(expected_indent) = expected_schema_indent {
                line_indent == expected_indent
            } else {
                line_indent < error_indent
            };

            if is_correct_level {
                let trimmed = line.trim();
                // Look for YAML key pattern (word followed by colon)
                if let Some(name_end) = trimmed.find(':') {
                    let name = trimmed[..name_end].trim();
                    // Check if it looks like a schema name (not empty, not a special key)
                    // Exclude common OpenAPI top-level keys that might appear before schemas
                    if !name.is_empty()
                        && !name.starts_with('$')
                        && !name.starts_with('-')
                        && name
                            .chars()
                            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
                        && name != "schemas"
                        && name != "components"
                        && name != "paths"
                        && name != "openapi"
                        && name != "info"
                        && name != "servers"
                        && name != "security"
                        && name != "tags"
                        && name != "externalDocs"
                    {
                        return Some(name.to_string());
                    }
                }
            }

            // Stop searching if we've gone too far back (hit a top-level key or components:)
            if let Some(schemas_indent_val) = schemas_indent {
                if line_indent < schemas_indent_val {
                    // We've gone back past the schemas: level, stop
                    break;
                }
            } else if line_indent == 0 {
                // Hit top-level, stop
                break;
            }
        }
        None
    }

    /// Extract the actual value/content found at the error location
    /// error_line_idx is 0-indexed
    pub fn extract_actual_value(&self, error_line_idx: usize, _column: usize) -> Option<String> {
        if error_line_idx >= self.lines.len() {
            return None;
        }

        let error_line = self.lines[error_line_idx];
        let trimmed_line = error_line.trim();

        // If the line is just a key with colon, the value might be on the next line
        if trimmed_line.ends_with(':') && error_line_idx + 1 < self.lines.len() {
            let next_line = self.lines[error_line_idx + 1].trim();
            if !next_line.is_empty() {
                return Some(format!(
                    "Key '{}' with value on next line: '{}'",
                    trimmed_line, next_line
                ));
            }
        }

        // Try to extract the value after the colon on the same line
        if let Some(colon_pos) = trimmed_line.find(':') {
            let value_part = trimmed_line[colon_pos + 1..].trim();
            if !value_part.is_empty() {
                return Some(format!("Found value: '{}'", value_part));
            }
        }

        // If no value found, describe what we see
        if trimmed_line.is_empty() {
            Some("Empty line".to_string())
        } else if trimmed_line.ends_with(':') {
            Some("Key with no value (might be expecting a nested object)".to_string())
        } else {
            Some(format!("Line content: '{}'", trimmed_line))
        }
    }

    /// Detect common issues in schema definitions and provide specific suggestions
    pub fn detect_common_issues(error_line: &str) -> String {
        let trimmed = error_line.trim();

        // Check for null value
        if trimmed == "null" || trimmed.starts_with("null:") {
            return "Found 'null' value. Schema entries cannot be null. Either remove this entry or provide a valid schema object or $ref.".to_string();
        }

        // Check for empty value
        if trimmed.is_empty() || trimmed == ":" {
            return "Found empty schema entry. Provide a valid schema object or $ref.".to_string();
        }

        // Check if it's just a plain string (not a $ref)
        if trimmed.starts_with('"') && trimmed.ends_with('"') && !trimmed.contains("$ref") {
            return "Found plain string value. If this is a reference, use $ref format: $ref: '#/components/schemas/SchemaName'".to_string();
        }

        // Check if it looks like a scalar value that's not a $ref
        if !trimmed.starts_with("$ref")
            && !trimmed.starts_with("type:")
            && !trimmed.starts_with("-")
            && !trimmed.contains(":")
            && trimmed
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            return "Found scalar value that's not a valid schema. Schema entries must be objects with 'type' field or $ref strings.".to_string();
        }

        // Check for malformed $ref
        if trimmed.contains("$ref") && !trimmed.contains("#/components/schemas/") {
            return "Found $ref but it doesn't match expected format. Use: $ref: '#/components/schemas/SchemaName'".to_string();
        }

        String::new()
    }
}
