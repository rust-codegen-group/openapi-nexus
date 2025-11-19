//! Go function parameters

use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use super::go_doc_comment::GoDocComment;
use crate::ast::go_expression::GoExpression;
use openapi_nexus_core::traits::ToRcDoc;

/// Go function parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoParameter {
    pub name: String,
    pub param_type: GoExpression,
    pub variadic: bool,
    pub doc: Option<GoDocComment>,
}

impl GoParameter {
    pub fn new(name: String, param_type: GoExpression) -> Self {
        Self {
            name,
            param_type,
            variadic: false,
            doc: None,
        }
    }

    pub fn variadic(name: String, param_type: GoExpression) -> Self {
        Self {
            name,
            param_type,
            variadic: true,
            doc: None,
        }
    }

    pub fn with_doc(mut self, doc: GoDocComment) -> Self {
        self.doc = Some(doc);
        self
    }
}

impl ToRcDoc for GoParameter {
    fn to_rcdoc(&self) -> RcDoc<'static, ()> {
        let mut doc = RcDoc::nil();

        if self.variadic {
            doc = doc.append(RcDoc::text("..."));
        }

        doc = doc
            .append(RcDoc::text(self.name.clone()))
            .append(RcDoc::space())
            .append(self.param_type.to_rcdoc());

        doc
    }
}
