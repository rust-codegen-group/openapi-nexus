use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use crate::ast::{TsDocComment, TsExpression};
use openapi_nexus_core::traits::ToRcDoc;

/// TypeScript property definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsProperty {
    pub name: String,
    pub type_expr: TsExpression,
    pub optional: bool,
    pub documentation: Option<TsDocComment>,
}

impl TsProperty {
    /// Create a new property
    pub fn new(name: String, type_expr: TsExpression) -> Self {
        Self {
            name,
            type_expr,
            optional: false,
            documentation: None,
        }
    }

    /// Set whether the property is optional
    pub fn with_optional(mut self, optional: bool) -> Self {
        self.optional = optional;
        self
    }

    /// Add documentation
    pub fn with_docs(mut self, documentation: TsDocComment) -> Self {
        self.documentation = Some(documentation);
        self
    }
}

impl ToRcDoc for TsProperty {
    fn to_rcdoc(&self) -> RcDoc<'static, ()> {
        let mut doc = RcDoc::text(self.name.clone());

        if self.optional {
            doc = doc.append(RcDoc::text("?"));
        }

        doc.append(RcDoc::text(":"))
            .append(RcDoc::space())
            .append(self.type_expr.to_rcdoc())
    }
}
