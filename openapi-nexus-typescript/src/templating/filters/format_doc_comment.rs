//! Template filter for formatting documentation comments

use minijinja::value::ViaDeserialize;

use crate::ast::TsDocComment;
use crate::config::MAX_LINE_WIDTH;
use openapi_nexus_core::traits::{EmissionContext, ToRcDocWithContext};

/// Input type for documentation comment filter
#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub enum DocCommentInput {
    /// Structured documentation comment
    DocComment(TsDocComment),
    /// Raw string documentation
    String(String),
}

impl DocCommentInput {
    /// Convert to TsDocComment
    fn into_doc_comment(self) -> TsDocComment {
        match self {
            Self::String(s) => TsDocComment::new(s),
            Self::DocComment(doc) => doc,
        }
    }
}

/// Template filter for formatting documentation comments
/// Accepts either a String or a serialized TsDocComment Value
pub fn format_doc_comment_filter(
    value: ViaDeserialize<DocCommentInput>,
    indent: Option<usize>,
) -> Result<String, minijinja::Error> {
    let ctx = EmissionContext {
        indent: indent.unwrap_or(0),
        max_line_width: MAX_LINE_WIDTH,
    };

    let doc_comment = value.0.into_doc_comment();

    doc_comment
        .to_rcdoc_with_context(&ctx)
        .map(|doc| doc.pretty(MAX_LINE_WIDTH).to_string())
        .map_err(|e| {
            minijinja::Error::new(
                minijinja::ErrorKind::InvalidOperation,
                format!("Failed to render doc comment: {:?}", e),
            )
        })
}
