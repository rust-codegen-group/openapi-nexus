use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use crate::ast::TsExpression;
use crate::emission::error::EmitError;
use openapi_nexus_core::traits::ToRcDoc;

/// TypeScript parameter definition
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TsParameter {
    pub name: String,
    pub type_expr: Option<TsExpression>,
    pub optional: bool,
    pub default_value: Option<String>,
}

impl TsParameter {
    /// Create a new parameter
    pub fn new(name: String) -> Self {
        Self {
            name,
            type_expr: None,
            optional: false,
            default_value: None,
        }
    }

    /// Create a parameter with type
    pub fn with_type(name: String, type_expr: TsExpression) -> Self {
        Self {
            name,
            type_expr: Some(type_expr),
            optional: false,
            default_value: None,
        }
    }

    /// Create an optional parameter
    pub fn optional(name: String, type_expr: Option<TsExpression>) -> Self {
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
    type Error = EmitError;

    fn to_rcdoc(&self) -> Result<RcDoc<'static, ()>, EmitError> {
        let mut doc = RcDoc::text(self.name.clone());

        if self.optional {
            doc = doc.append(RcDoc::text("?"));
        }

        if let Some(type_expr) = &self.type_expr {
            doc = doc
                .append(RcDoc::text(":"))
                .append(RcDoc::space())
                .append(type_expr.to_rcdoc()?);
        }

        if let Some(default_value) = &self.default_value {
            doc = doc
                .append(RcDoc::space())
                .append(RcDoc::text("="))
                .append(RcDoc::space())
                .append(RcDoc::text(default_value.clone()));
        }

        Ok(doc)
    }
}
