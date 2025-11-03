//! API class signature data for template rendering

use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use crate::ast::TsGeneric;
use crate::emission::error::EmitError;
use crate::templating::data::api_class_data::ApiClassData;
use openapi_nexus_core::traits::ToRcDoc;

/// API class signature for template rendering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiClassSignature {
    pub is_export: bool,
    pub name: String,
    pub generics: Vec<TsGeneric>,
    pub extends: Option<String>,
    pub implements: Vec<String>,
}

impl ApiClassSignature {
    /// Create a signature from a class data
    pub fn from_class(class: &ApiClassData) -> Self {
        Self {
            is_export: class.is_export,
            name: class.name.clone(),
            generics: class.generics.clone(),
            extends: class.extends.clone(),
            implements: class.implements.clone(),
        }
    }
}

impl ToRcDoc for ApiClassSignature {
    type Error = EmitError;

    fn to_rcdoc(&self) -> Result<RcDoc<'static, ()>, EmitError> {
        let mut doc = RcDoc::nil();

        if self.is_export {
            doc = doc.append(RcDoc::text("export")).append(RcDoc::space());
        }

        doc = doc
            .append(RcDoc::text("class"))
            .append(RcDoc::space())
            .append(RcDoc::text(self.name.clone()));

        if !self.generics.is_empty() {
            let generics_docs = self
                .generics
                .iter()
                .map(|g| g.to_rcdoc())
                .collect::<Result<Vec<_>, _>>()?;
            doc = doc
                .append(RcDoc::text("<"))
                .append(RcDoc::intersperse(
                    generics_docs,
                    RcDoc::text(",").append(RcDoc::space()),
                ))
                .append(RcDoc::text(">"));
        }

        if let Some(ext) = &self.extends {
            doc = doc
                .append(RcDoc::space())
                .append(RcDoc::text("extends"))
                .append(RcDoc::space())
                .append(RcDoc::text(ext.clone()));
        }

        if !self.implements.is_empty() {
            doc = doc
                .append(RcDoc::space())
                .append(RcDoc::text("implements"))
                .append(RcDoc::space())
                .append(RcDoc::text(self.implements.join(",")));
        }

        Ok(doc)
    }
}
