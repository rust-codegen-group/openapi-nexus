//! Go struct fields

use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use super::go_doc_comment::GoDocComment;
use crate::ast::go_expression::GoExpression;
use openapi_nexus_core::traits::ToRcDoc;

// Re-export for convenience
pub use crate::ast::go_expression::GoExpression as GoType;

/// Go struct field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoField {
    pub name: String,
    pub field_type: GoExpression,
    pub json_tag: Option<String>,
    pub doc: Option<GoDocComment>,
}

impl GoField {
    pub fn new(name: String, field_type: GoExpression) -> Self {
        Self {
            name,
            field_type,
            json_tag: None,
            doc: None,
        }
    }

    pub fn with_json_tag(mut self, tag: String) -> Self {
        self.json_tag = Some(tag);
        self
    }

    pub fn with_doc(mut self, doc: GoDocComment) -> Self {
        self.doc = Some(doc);
        self
    }
}

impl ToRcDoc for GoField {
    fn to_rcdoc(&self) -> RcDoc<'static, ()> {
        let mut doc = RcDoc::text(self.name.clone())
            .append(RcDoc::space())
            .append(self.field_type.to_rcdoc());

        if let Some(tag) = &self.json_tag {
            doc = doc
                .append(RcDoc::space())
                .append(RcDoc::text(format!("`{}`", tag)));
        }

        doc
    }
}
