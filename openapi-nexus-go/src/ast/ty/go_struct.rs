//! Go struct definitions

use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use crate::ast::common::{GoDocComment, GoField};
use openapi_nexus_core::traits::ToRcDoc;

/// Go struct definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoStruct {
    pub name: String,
    pub fields: Vec<GoField>,
    pub doc: Option<GoDocComment>,
}

impl GoStruct {
    pub fn new(name: String) -> Self {
        Self {
            name,
            fields: Vec::new(),
            doc: None,
        }
    }

    pub fn with_field(mut self, field: GoField) -> Self {
        self.fields.push(field);
        self
    }

    pub fn with_fields(mut self, fields: Vec<GoField>) -> Self {
        self.fields = fields;
        self
    }

    pub fn with_doc(mut self, doc: GoDocComment) -> Self {
        self.doc = Some(doc);
        self
    }
}

impl ToRcDoc for GoStruct {
    fn to_rcdoc(&self) -> RcDoc<'static, ()> {
        let mut doc = RcDoc::nil();

        if let Some(comment) = &self.doc {
            doc = doc.append(comment.to_rcdoc()).append(RcDoc::hardline());
        }

        doc = doc
            .append(RcDoc::text("type"))
            .append(RcDoc::space())
            .append(RcDoc::text(self.name.clone()))
            .append(RcDoc::space())
            .append(RcDoc::text("struct"))
            .append(RcDoc::space())
            .append(RcDoc::text("{"));

        if !self.fields.is_empty() {
            doc = doc.append(RcDoc::hardline());
            for field in &self.fields {
                if let Some(comment) = &field.doc {
                    doc = doc
                        .append(RcDoc::text("\t"))
                        .append(comment.to_rcdoc())
                        .append(RcDoc::hardline());
                }
                doc = doc
                    .append(RcDoc::text("\t"))
                    .append(field.to_rcdoc())
                    .append(RcDoc::hardline());
            }
        }

        doc.append(RcDoc::text("}"))
    }
}
