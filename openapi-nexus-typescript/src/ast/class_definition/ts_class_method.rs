use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use crate::ast::{TsDocComment, TsExpression, TsParameter, TsVisibility};
use crate::emission::error::EmitError;
use openapi_nexus_core::traits::{EmissionContext, ToRcDocWithContext};

/// TypeScript class method for template rendering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsClassMethod {
    pub name: String,
    pub parameters: Vec<TsParameter>,
    pub return_type: Option<TsExpression>,
    pub visibility: TsVisibility,
    pub is_static: bool,
    pub is_async: bool,
    pub is_abstract: bool,
    pub documentation: Option<TsDocComment>,
}

impl TsClassMethod {
    /// Create a new class method
    pub fn new(name: String) -> Self {
        Self {
            name,
            parameters: Vec::new(),
            return_type: None,
            visibility: TsVisibility::Public,
            is_static: false,
            is_async: false,
            is_abstract: false,
            documentation: None,
        }
    }

    /// Add parameters
    pub fn with_parameters(mut self, parameters: Vec<TsParameter>) -> Self {
        self.parameters = parameters;
        self
    }

    /// Set return type
    pub fn with_return_type(mut self, return_type: TsExpression) -> Self {
        self.return_type = Some(return_type);
        self
    }

    /// Set visibility
    pub fn with_visibility(mut self, visibility: TsVisibility) -> Self {
        self.visibility = visibility;
        self
    }

    /// Make static
    pub fn with_static(mut self) -> Self {
        self.is_static = true;
        self
    }

    /// Make async
    pub fn with_async(mut self) -> Self {
        self.is_async = true;
        self
    }

    /// Make abstract
    pub fn with_abstract(mut self) -> Self {
        self.is_abstract = true;
        self
    }

    /// Add documentation
    pub fn with_docs(mut self, documentation: TsDocComment) -> Self {
        self.documentation = Some(documentation);
        self
    }
}

impl ToRcDocWithContext for TsClassMethod {
    type Error = EmitError;

    fn to_rcdoc_with_context(
        &self,
        context: &EmissionContext,
    ) -> Result<RcDoc<'static, ()>, EmitError> {
        let mut parts = Vec::new();

        // Visibility
        match self.visibility {
            TsVisibility::Private => parts.push(RcDoc::text("private")),
            TsVisibility::Protected => parts.push(RcDoc::text("protected")),
            TsVisibility::Public => {}
        }

        // Static
        if self.is_static {
            parts.push(RcDoc::text("static"));
        }

        // Abstract
        if self.is_abstract {
            parts.push(RcDoc::text("abstract"));
        }

        // Async
        if self.is_async {
            parts.push(RcDoc::text("async"));
        }

        // Method name and parameters
        let params_docs: Result<Vec<_>, _> = self
            .parameters
            .iter()
            .map(|p| p.to_rcdoc_with_context(context))
            .collect();
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
                .append(return_type.to_rcdoc_with_context(context)?);
        }

        parts.push(signature_doc);

        Ok(RcDoc::intersperse(parts, RcDoc::space()))
    }
}
