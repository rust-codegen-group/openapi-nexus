use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use crate::ast::TsDocComment;
use crate::emission::error::EmitError;
use openapi_nexus_core::traits::ToRcDoc;

/// TypeScript enum variant definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsEnumVariant {
    pub name: String,
    pub value: Option<String>,
    pub documentation: Option<TsDocComment>,
}

impl TsEnumVariant {
    /// Create a new enum variant
    pub fn new(name: String) -> Self {
        Self {
            name,
            value: None,
            documentation: None,
        }
    }

    /// Create an enum variant with explicit value
    pub fn with_value(name: String, value: String) -> Self {
        Self {
            name,
            value: Some(value),
            documentation: None,
        }
    }

    /// Add documentation
    pub fn with_docs(mut self, documentation: TsDocComment) -> Self {
        self.documentation = Some(documentation);
        self
    }
}

impl ToRcDoc for TsEnumVariant {
    type Error = EmitError;

    fn to_rcdoc(&self) -> Result<RcDoc<'static, ()>, EmitError> {
        let mut doc = RcDoc::text(self.name.clone());

        if let Some(value) = &self.value {
            doc = doc
                .append(RcDoc::space())
                .append(RcDoc::text("="))
                .append(RcDoc::space())
                .append(RcDoc::text(value.clone()));
        }

        Ok(doc)
    }
}
