use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use crate::ast::TsDocComment;
use crate::ast::{TsExpression, TsGeneric};
use crate::emission::error::EmitError;
use crate::emission::ts_type_emitter::TsTypeEmitter;
use openapi_nexus_core::traits::ToRcDoc;

/// TypeScript type alias definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsTypeAliasDefinition {
    pub name: String,
    pub type_expr: TsExpression,
    pub generics: Vec<TsGeneric>,
    pub documentation: Option<TsDocComment>,
}

impl TsTypeAliasDefinition {
    /// Create a new type alias
    pub fn new(name: String, type_expr: TsExpression) -> Self {
        Self {
            name,
            type_expr,
            generics: Vec::new(),
            documentation: None,
        }
    }

    /// Add generics
    pub fn with_generics(mut self, generics: Vec<TsGeneric>) -> Self {
        self.generics = generics;
        self
    }

    /// Add documentation
    pub fn with_docs(mut self, documentation: TsDocComment) -> Self {
        self.documentation = Some(documentation);
        self
    }
}

impl ToRcDoc for TsTypeAliasDefinition {
    type Error = EmitError;

    fn to_rcdoc(&self) -> Result<RcDoc<'static, ()>, EmitError> {
        let type_emitter = TsTypeEmitter;

        let mut doc = RcDoc::text("export ")
            .append(RcDoc::text("type"))
            .append(RcDoc::space())
            .append(RcDoc::text(self.name.clone()));

        // Add generics
        if !self.generics.is_empty() {
            let generic_docs: Result<Vec<_>, _> =
                self.generics.iter().map(|g| g.to_rcdoc()).collect();
            let generic_strings: Vec<String> = generic_docs?
                .iter()
                .map(|doc| format!("{}", doc.pretty(80)))
                .collect();
            doc = doc.append(RcDoc::text(format!("<{}>", generic_strings.join(", "))));
        }

        // Add type expression
        let type_doc = type_emitter.emit_type_expression_doc(&self.type_expr)?;
        doc = doc.append(RcDoc::text(" = ")).append(type_doc);

        // Add documentation if present
        if let Some(docs) = &self.documentation {
            doc = docs.to_rcdoc()?.append(RcDoc::line()).append(doc);
        }

        Ok(doc)
    }
}
