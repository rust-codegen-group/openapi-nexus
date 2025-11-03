use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use openapi_nexus_core::traits::ToRcDoc;

use super::ts_import_specifier::TsImportSpecifier;

/// TypeScript import statement
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TsImport {
    pub module_path: String,
    pub specifiers: Vec<TsImportSpecifier>,
    pub is_type_only: bool,
}

impl TsImport {
    /// Create a new import
    pub fn new(module_path: impl Into<String>) -> Self {
        Self {
            module_path: module_path.into(),
            specifiers: Vec::new(),
            is_type_only: false,
        }
    }

    /// Create a type-only import
    pub fn new_type_only(module_path: impl Into<String>) -> Self {
        Self {
            module_path: module_path.into(),
            specifiers: Vec::new(),
            is_type_only: true,
        }
    }

    /// Add specifiers
    pub fn with_specifiers(mut self, specifiers: Vec<TsImportSpecifier>) -> Self {
        self.specifiers = specifiers;
        self
    }

    /// Add named imports
    pub fn with_named_imports(
        mut self,
        names: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.specifiers = names
            .into_iter()
            .map(|name| TsImportSpecifier::new(name.into()))
            .collect();
        self
    }

    /// Add type imports
    pub fn with_type_imports(mut self, names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.specifiers = names
            .into_iter()
            .map(|name| TsImportSpecifier::new_type(name.into()))
            .collect();
        self
    }

    /// Add a single specifier
    pub fn with_specifier(mut self, specifier: TsImportSpecifier) -> Self {
        self.specifiers.push(specifier);
        self
    }

    /// Make this import type-only
    pub fn with_type_only(mut self) -> Self {
        self.is_type_only = true;
        self
    }
}

impl ToRcDoc for TsImport {
    fn to_rcdoc(&self) -> RcDoc<'static, ()> {
        // Handle side-effect import (no specifiers)
        if self.specifiers.is_empty() {
            return RcDoc::text("import")
                .append(RcDoc::space())
                .append(RcDoc::text(format!("'{}'", self.module_path)))
                .append(RcDoc::text(";"));
        }

        // Start building import statement with "import" keyword
        let mut doc = RcDoc::text("import");

        // Add "type" keyword if this is a type-only import
        if self.is_type_only {
            doc = doc.append(RcDoc::space()).append(RcDoc::text("type"));
        }

        // Build specifiers
        let specifier_docs: Vec<RcDoc<'static, ()>> = self
            .specifiers
            .iter()
            .map(|spec| {
                let mut spec_doc = RcDoc::nil();

                // Add "type" keyword for individual specifiers if needed
                if spec.is_type && !self.is_type_only {
                    spec_doc = spec_doc.append(RcDoc::text("type")).append(RcDoc::space());
                }

                spec_doc = spec_doc.append(RcDoc::text(spec.name.clone()));

                // Add alias if present
                if let Some(alias) = &spec.alias {
                    spec_doc = spec_doc
                        .append(RcDoc::space())
                        .append(RcDoc::text("as"))
                        .append(RcDoc::space())
                        .append(RcDoc::text(alias.clone()));
                }

                spec_doc
            })
            .collect();

        // Join specifiers with ", "
        let specifiers_content = RcDoc::intersperse(specifier_docs, RcDoc::text(", "));

        // Build complete import statement
        doc = doc
            .append(RcDoc::space())
            .append(
                RcDoc::text("{")
                    .append(RcDoc::space())
                    .append(specifiers_content)
                    .append(RcDoc::space())
                    .append(RcDoc::text("}")),
            )
            .append(RcDoc::space())
            .append(RcDoc::text("from"))
            .append(RcDoc::space())
            .append(RcDoc::text(format!("'{}'", self.module_path)))
            .append(RcDoc::text(";"));

        doc
    }
}
