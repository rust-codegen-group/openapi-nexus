use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use crate::config::MAX_LINE_WIDTH;
use openapi_nexus_core::traits::ToRcDoc;

/// TypeScript documentation comment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsDocComment(pub String);

impl TsDocComment {
    /// Create a new documentation comment
    pub fn new(content: impl Into<String>) -> Self {
        Self(content.into())
    }
}

/// Helper functions for formatting documentation
pub fn format_param_doc(name: &str, description: &str) -> String {
    format!("@param {} {}", name, description)
}

pub fn format_return_doc(description: &str) -> String {
    format!("@returns {}", description)
}

pub fn format_throws_doc(error_type: &str, description: &str) -> String {
    format!("@throws {{{}}} {}", error_type, description)
}

pub fn format_example_doc(example: &str) -> String {
    format!("@example\n{}", example)
}

/// Create a complete method documentation comment
pub fn create_method_doc(
    description: &str,
    params: &[(String, String)],
    return_desc: Option<&str>,
    throws: &[(String, String)],
    example: Option<&str>,
) -> TsDocComment {
    let mut lines = vec![description.to_string()];

    if !params.is_empty() {
        lines.push(String::new()); // Empty line
        for (name, desc) in params {
            lines.push(format_param_doc(name, desc));
        }
    }

    if let Some(return_desc) = return_desc {
        lines.push(String::new()); // Empty line
        lines.push(format_return_doc(return_desc));
    }

    if !throws.is_empty() {
        lines.push(String::new()); // Empty line
        for (error_type, desc) in throws {
            lines.push(format_throws_doc(error_type, desc));
        }
    }

    if let Some(example) = example {
        lines.push(String::new()); // Empty line
        lines.push(format_example_doc(example));
    }

    TsDocComment::new(lines.join("\n"))
}

/// Create a simple class or interface documentation comment
pub fn create_type_doc(description: &str, additional_info: Option<&str>) -> TsDocComment {
    if let Some(info) = additional_info {
        TsDocComment::new(format!("{}\n\n{}", description, info))
    } else {
        TsDocComment::new(description.to_string())
    }
}

// ToRcDoc implementations
impl ToRcDoc for TsDocComment {
    fn to_rcdoc(&self) -> RcDoc<'static, ()> {
        const INDENT: usize = 0;
        let indent_str = " ".repeat(INDENT);

        // Determine if we need multiline format based on:
        // 1. Content contains newlines (explicit multiline)
        // 2. Single-line format (including indentation) would exceed max_line_width
        let has_newlines = self.0.contains('\n');
        let single_line_length = indent_str.len() + 7 + self.0.len(); // indent + "/** " + content + " */"
        let needs_multiline = has_newlines || single_line_length > MAX_LINE_WIDTH;

        if needs_multiline {
            let lines: Vec<&str> = self.0.lines().collect();
            let mut parts = vec![RcDoc::text(format!("{}/**", indent_str))];
            for line in lines {
                parts.push(RcDoc::hardline());
                if line.is_empty() {
                    parts.push(RcDoc::text(format!("{} *", indent_str)));
                } else {
                    parts.push(RcDoc::text(format!("{} * {}", indent_str, line)));
                }
            }
            parts.push(RcDoc::hardline());
            parts.push(RcDoc::text(format!("{} */", indent_str)));
            RcDoc::concat(parts)
        } else {
            RcDoc::text(format!("{}/** {} */", indent_str, self.0))
        }
    }
}
