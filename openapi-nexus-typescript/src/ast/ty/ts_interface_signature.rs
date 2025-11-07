use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use crate::ast::TsGeneric;
use openapi_nexus_core::traits::ToRcDoc;

/// TypeScript interface signature (single-line declaration header)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsInterfaceSignature {
    pub is_export: bool,
    pub ts_name: String,
    pub original_name: String,
    pub generics: Vec<TsGeneric>,
    pub extends: Vec<String>,
}

impl TsInterfaceSignature {
    pub fn new(ts_name: String, original_name: String) -> Self {
        Self {
            is_export: true,
            ts_name,
            original_name,
            generics: Vec::new(),
            extends: Vec::new(),
        }
    }

    pub fn with_generics(mut self, generics: Vec<TsGeneric>) -> Self {
        self.generics = generics;
        self
    }

    pub fn with_extends(mut self, extends: Vec<String>) -> Self {
        self.extends = extends;
        self
    }

    pub fn with_export(mut self, is_export: bool) -> Self {
        self.is_export = is_export;
        self
    }
}

impl ToRcDoc for TsInterfaceSignature {
    fn to_rcdoc(&self) -> RcDoc<'static, ()> {
        let mut doc = RcDoc::nil();

        if self.is_export {
            doc = doc.append(RcDoc::text("export")).append(RcDoc::space());
        }

        doc = doc
            .append(RcDoc::text("interface"))
            .append(RcDoc::space())
            .append(RcDoc::text(self.ts_name.clone()));

        if !self.generics.is_empty() {
            let generics_docs: Vec<_> = self.generics.iter().map(|g| g.to_rcdoc()).collect();
            doc = doc
                .append(RcDoc::text("<"))
                .append(RcDoc::intersperse(
                    generics_docs,
                    RcDoc::text(",").append(RcDoc::space()),
                ))
                .append(RcDoc::text(">"));
        }

        if !self.extends.is_empty() {
            doc = doc
                .append(RcDoc::space())
                .append(RcDoc::text("extends"))
                .append(RcDoc::space())
                .append(RcDoc::text(self.extends.join(",")));
        }

        doc
    }
}
