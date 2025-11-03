//! API method data for template rendering

use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use crate::ast::{TsDocComment, TsExpression, TsParameter};
use crate::emission::error::EmitError;
use openapi_nexus_core::traits::ToRcDoc;

/// API method data for template rendering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiMethodData {
    pub name: String,
    pub parameters: Vec<TsParameter>,
    pub return_type: Option<TsExpression>,
    pub is_async: bool,
    pub documentation: Option<TsDocComment>,
}

impl ToRcDoc for ApiMethodData {
    type Error = EmitError;

    fn to_rcdoc(&self) -> Result<RcDoc<'static, ()>, EmitError> {
        let mut parts = Vec::new();

        // Async
        if self.is_async {
            parts.push(RcDoc::text("async"));
        }

        // Method name and parameters
        let params_docs: Result<Vec<_>, _> = self.parameters.iter().map(|p| p.to_rcdoc()).collect();
        let params_doc = RcDoc::text("(")
            .append(RcDoc::intersperse(
                params_docs?,
                RcDoc::text(",").append(RcDoc::space()),
            ))
            .append(RcDoc::text(")"));

        let mut signature_doc = RcDoc::text(self.name.clone()).append(params_doc);

        // Return type
        if let Some(return_type) = &self.return_type {
            signature_doc = signature_doc
                .append(RcDoc::text(":"))
                .append(RcDoc::space())
                .append(return_type.to_rcdoc()?);
        }

        parts.push(signature_doc);

        Ok(RcDoc::intersperse(parts, RcDoc::space()))
    }
}
