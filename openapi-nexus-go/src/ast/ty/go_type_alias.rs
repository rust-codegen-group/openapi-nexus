//! Go type alias definitions

use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use crate::ast::common::GoDocComment;
use crate::ast::go_expression::GoExpression;
use openapi_nexus_core::traits::ToRcDoc;

/// Go type alias definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoTypeAlias {
    pub name: String,
    pub type_expr: GoExpression,
    pub doc: Option<GoDocComment>,
}

impl GoTypeAlias {
    pub fn new(name: String, type_expr: GoExpression) -> Self {
        Self {
            name,
            type_expr,
            doc: None,
        }
    }

    pub fn with_doc(mut self, doc: GoDocComment) -> Self {
        self.doc = Some(doc);
        self
    }
}

impl ToRcDoc for GoTypeAlias {
    fn to_rcdoc(&self) -> RcDoc<'static, ()> {
        let mut doc = RcDoc::nil();

        if let Some(comment) = &self.doc {
            doc = doc.append(comment.to_rcdoc()).append(RcDoc::hardline());
        }

        doc.append(RcDoc::text("type"))
            .append(RcDoc::space())
            .append(RcDoc::text(self.name.clone()))
            .append(RcDoc::space())
            .append(RcDoc::text("="))
            .append(RcDoc::space())
            .append(self.type_expr.to_rcdoc())
    }
}
