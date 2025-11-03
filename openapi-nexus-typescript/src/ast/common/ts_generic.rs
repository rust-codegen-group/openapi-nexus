use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use openapi_nexus_core::traits::ToRcDoc;

/// TypeScript generic parameter definition
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TsGeneric {
    pub name: String,
    pub constraint: Option<String>,
    pub default: Option<String>,
}

impl TsGeneric {
    /// Create a new generic parameter
    pub fn new(name: String) -> Self {
        Self {
            name,
            constraint: None,
            default: None,
        }
    }

    /// Add constraint (extends clause)
    pub fn with_constraint(mut self, constraint: String) -> Self {
        self.constraint = Some(constraint);
        self
    }

    /// Add default type
    pub fn with_default(mut self, default: String) -> Self {
        self.default = Some(default);
        self
    }
}

impl ToRcDoc for TsGeneric {
    fn to_rcdoc(&self) -> RcDoc<'static, ()> {
        let mut doc = RcDoc::text(self.name.clone());

        if let Some(constraint) = &self.constraint {
            doc = doc
                .append(RcDoc::space())
                .append(RcDoc::text("extends"))
                .append(RcDoc::space())
                .append(RcDoc::text(constraint.clone()));
        }

        if let Some(default) = &self.default {
            doc = doc
                .append(RcDoc::space())
                .append(RcDoc::text("="))
                .append(RcDoc::space())
                .append(RcDoc::text(default.clone()));
        }

        doc
    }
}
