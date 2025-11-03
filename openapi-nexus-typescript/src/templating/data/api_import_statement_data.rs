//! API import statement data for template rendering

use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use crate::emission::error::EmitError;
use crate::templating::data::ApiImportSpecifier;
use openapi_nexus_core::traits::ToRcDoc;

/// Import statement for template rendering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiImportStatement {
    pub module_path: String,
    pub imports: Vec<ApiImportSpecifier>,
    pub is_type_only: bool,
}

impl ApiImportStatement {
    /// Create a new import statement
    pub fn new(module_path: String) -> Self {
        Self {
            module_path,
            imports: Vec::new(),
            is_type_only: false,
        }
    }

    /// Add import specifier
    pub fn with_import(mut self, name: String, alias: Option<String>) -> Self {
        self.imports.push(ApiImportSpecifier {
            name,
            alias,
            is_type: false,
        });
        self
    }

    /// Add type import specifier
    pub fn with_type_import(mut self, name: String, alias: Option<String>) -> Self {
        self.imports.push(ApiImportSpecifier {
            name,
            alias,
            is_type: true,
        });
        self
    }

    /// Make type-only import
    pub fn with_type_only(mut self) -> Self {
        self.is_type_only = true;
        self
    }
}

impl ToRcDoc for ApiImportStatement {
    type Error = EmitError;

    fn to_rcdoc(&self) -> Result<RcDoc<'static, ()>, EmitError> {
        // Side-effect only import
        if self.imports.is_empty() {
            return Ok(RcDoc::text(format!("import '{}';", self.module_path)));
        }

        let mut parts = vec![RcDoc::text("import")];

        // Type-only import
        if self.is_type_only {
            parts.push(RcDoc::space());
            parts.push(RcDoc::text("type"));
        }

        // Format specifiers
        let specifier_docs: Vec<RcDoc<()>> = self
            .imports
            .iter()
            .map(|spec| {
                let mut spec_parts = Vec::new();
                if spec.is_type && !self.is_type_only {
                    spec_parts.push(RcDoc::text("type"));
                    spec_parts.push(RcDoc::space());
                }
                spec_parts.push(RcDoc::text(spec.name.clone()));
                if let Some(alias) = &spec.alias {
                    spec_parts.push(RcDoc::space());
                    spec_parts.push(RcDoc::text("as"));
                    spec_parts.push(RcDoc::space());
                    spec_parts.push(RcDoc::text(alias.clone()));
                }
                RcDoc::concat(spec_parts)
            })
            .collect();

        parts.push(RcDoc::space());
        parts.push(RcDoc::text("{"));
        parts.push(RcDoc::space());
        parts.push(RcDoc::intersperse(
            specifier_docs,
            RcDoc::text(",").append(RcDoc::space()),
        ));
        parts.push(RcDoc::space());
        parts.push(RcDoc::text("}"));
        parts.push(RcDoc::space());
        parts.push(RcDoc::text("from"));
        parts.push(RcDoc::space());
        parts.push(RcDoc::text(format!("'{}'", self.module_path)));
        parts.push(RcDoc::text(";"));

        Ok(RcDoc::concat(parts))
    }
}
