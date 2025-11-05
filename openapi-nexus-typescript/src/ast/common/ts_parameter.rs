use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use crate::ast::TsExpression;
use openapi_nexus_core::traits::ToRcDoc;

/// TypeScript parameter definition
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TsParameter {
    pub name: String,
    pub type_expr: TsExpression,
    pub optional: bool,
    pub default_value: Option<String>,
}

impl TsParameter {
    /// Create a new parameter
    pub fn new(name: String, type_expr: TsExpression) -> Self {
        Self {
            name,
            type_expr,
            optional: false,
            default_value: None,
        }
    }

    /// Create an optional parameter
    pub fn optional(name: String, type_expr: TsExpression) -> Self {
        Self {
            name,
            type_expr,
            optional: true,
            default_value: None,
        }
    }

    /// Set default value
    pub fn with_default(mut self, default_value: String) -> Self {
        self.default_value = Some(default_value);
        self
    }
}

impl ToRcDoc for TsParameter {
    fn to_rcdoc(&self) -> RcDoc<'static, ()> {
        let mut doc = RcDoc::text(self.name.clone());

        if self.optional {
            doc = doc.append(RcDoc::text("?"));
        }

        doc = doc
            .append(RcDoc::text(":"))
            .append(RcDoc::space())
            .append(self.type_expr.to_rcdoc());

        if let Some(default_value) = &self.default_value {
            doc = doc
                .append(RcDoc::space())
                .append(RcDoc::text("="))
                .append(RcDoc::space())
                .append(RcDoc::text(default_value.clone()));
        }

        doc
    }
}
