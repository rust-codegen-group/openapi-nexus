use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use crate::ast::TsDocComment;
use crate::ast::{TsInterfaceSignature, TsProperty};
use crate::emission::ts_type_emitter::TsTypeEmitter;
use openapi_nexus_core::traits::ToRcDoc;

/// TypeScript interface definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsInterfaceDefinition {
    /// Single-line interface header (export/interface name/generics/extends)
    pub signature: TsInterfaceSignature,
    /// Members of the interface body. Methods are represented as function-typed properties.
    pub properties: Vec<TsProperty>,
    /// Optional documentation attached to the interface
    pub documentation: Option<TsDocComment>,
}

impl TsInterfaceDefinition {
    /// Create a new interface from a structured signature
    pub fn new(signature: TsInterfaceSignature) -> Self {
        Self {
            properties: Vec::new(),
            documentation: None,
            signature,
        }
    }

    /// Add a property
    pub fn with_property(mut self, property: TsProperty) -> Self {
        self.properties.push(property);
        self
    }

    /// Add multiple properties
    pub fn with_properties(mut self, properties: Vec<TsProperty>) -> Self {
        self.properties.extend(properties);
        self
    }

    /// Add documentation
    pub fn with_docs(mut self, documentation: TsDocComment) -> Self {
        self.documentation = Some(documentation);
        self
    }

    /// Get the names of the required properties.
    /// Used for template generation.
    pub fn required_prop_names(&self) -> Vec<String> {
        self.properties
            .iter()
            .filter(|p| !p.optional)
            .map(|p| p.prop_name.clone())
            .collect()
    }
}

impl ToRcDoc for TsInterfaceDefinition {
    fn to_rcdoc(&self) -> RcDoc<'static, ()> {
        // Start with the signature header (export interface Name<...> extends ...)
        let mut doc = self.signature.to_rcdoc();

        // Add body with properties
        if self.properties.is_empty() {
            doc = doc.append(RcDoc::space()).append(RcDoc::text("{}"));
        } else {
            // Render each property using its ToRcDoc
            let properties: Vec<_> = self.properties.iter().map(|p| p.to_rcdoc()).collect();

            let force_multiline = self.properties.len() > 2
                || self
                    .properties
                    .iter()
                    .any(|p| TsTypeEmitter::is_complex_type(&p.type_expr));

            let body_content = if force_multiline {
                RcDoc::intersperse(properties, RcDoc::text(",").append(RcDoc::line()))
            } else {
                RcDoc::intersperse(properties, RcDoc::text(", "))
            };

            doc = doc.append(RcDoc::space()).append(
                RcDoc::text("{")
                    .append(RcDoc::line())
                    .append(body_content)
                    .append(RcDoc::line())
                    .append(RcDoc::text("}")),
            );
        }

        // Add documentation if present
        if let Some(docs) = &self.documentation {
            doc = docs.to_rcdoc().append(RcDoc::line()).append(doc);
        }

        doc
    }
}
