use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use crate::ast::{TsDocComment, TsEnumVariant};
use openapi_nexus_core::traits::ToRcDoc;

/// TypeScript enum definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsEnumDefinition {
    pub ts_name: String,
    pub original_name: String,
    pub variants: Vec<TsEnumVariant>,
    pub is_const: bool,
    pub documentation: Option<TsDocComment>,
}

impl TsEnumDefinition {
    /// Create a new enum
    pub fn new(ts_name: String, original_name: String) -> Self {
        Self {
            ts_name,
            original_name,
            variants: Vec::new(),
            is_const: false,
            documentation: None,
        }
    }

    /// Create a const enum
    pub fn new_const(ts_name: String, original_name: String) -> Self {
        Self {
            ts_name,
            original_name,
            variants: Vec::new(),
            is_const: true,
            documentation: None,
        }
    }

    /// Add a variant
    pub fn with_variant(mut self, variant: TsEnumVariant) -> Self {
        self.variants.push(variant);
        self
    }

    /// Add multiple variants
    pub fn with_variants(mut self, variants: Vec<TsEnumVariant>) -> Self {
        self.variants.extend(variants);
        self
    }

    /// Add documentation
    pub fn with_docs(mut self, documentation: TsDocComment) -> Self {
        self.documentation = Some(documentation);
        self
    }
}

impl ToRcDoc for TsEnumDefinition {
    fn to_rcdoc(&self) -> RcDoc<'static, ()> {
        let mut doc = RcDoc::text("export ")
            .append(RcDoc::text(if self.is_const {
                "const enum"
            } else {
                "enum"
            }))
            .append(RcDoc::space())
            .append(RcDoc::text(self.ts_name.clone()));

        // Add enum body
        if self.variants.is_empty() {
            doc = doc.append(RcDoc::space()).append(RcDoc::text("{}"));
        } else {
            let variant_docs: Vec<RcDoc<'static, ()>> = self
                .variants
                .iter()
                .map(|variant| variant.to_rcdoc())
                .collect();

            let force_multiline = self.variants.len() > 2;

            let body_content = if force_multiline {
                // Indent each variant when in multiline mode
                let indented_variants: Vec<RcDoc<'static, ()>> = variant_docs
                    .into_iter()
                    .map(|variant_doc| RcDoc::text("  ").append(variant_doc))
                    .collect();
                RcDoc::intersperse(indented_variants, RcDoc::text(",").append(RcDoc::line()))
            } else {
                RcDoc::intersperse(variant_docs, RcDoc::text(", "))
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
