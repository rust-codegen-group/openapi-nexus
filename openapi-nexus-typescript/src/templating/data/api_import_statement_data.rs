//! API import statement data for template rendering

use std::collections::BTreeSet;

use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use crate::templating::data::ApiImportSpecifier;
use openapi_nexus_core::traits::ToRcDoc;

/// Import statement for template rendering
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ApiImportStatement {
    pub module_path: String,
    pub imports: BTreeSet<ApiImportSpecifier>,
}

impl ApiImportStatement {
    /// Create a new import statement
    pub fn new(module_path: String) -> Self {
        Self {
            module_path,
            imports: BTreeSet::new(),
        }
    }

    /// Add import specifier
    pub fn with_import(mut self, name: String, alias: Option<String>) -> Self {
        self.imports.insert(ApiImportSpecifier {
            name,
            alias,
            is_type: false,
        });
        self
    }

    /// Add type import specifier
    pub fn with_type_import(mut self, name: String, alias: Option<String>) -> Self {
        self.imports.insert(ApiImportSpecifier {
            name,
            alias,
            is_type: true,
        });
        self
    }

    /// Add multiple type imports at once
    pub fn with_type_imports<I>(mut self, names: I) -> Self
    where
        I: IntoIterator<Item = String>,
    {
        for name in names {
            self.imports.insert(ApiImportSpecifier {
                name,
                alias: None,
                is_type: true,
            });
        }
        self
    }

    /// Add multiple value imports at once
    pub fn with_imports<I>(mut self, names: I) -> Self
    where
        I: IntoIterator<Item = String>,
    {
        for name in names {
            self.imports.insert(ApiImportSpecifier {
                name,
                alias: None,
                is_type: false,
            });
        }
        self
    }
}

impl ToRcDoc for ApiImportStatement {
    fn to_rcdoc(&self) -> RcDoc<'static, ()> {
        // Side-effect only import
        if self.imports.is_empty() {
            return RcDoc::text(format!("import '{}';", self.module_path));
        }

        // Check if all imports are types (for cleaner output: `import type { ... }`)
        // Auto-detect: if all specifiers are types, use `import type { ... }` instead of inline `type` keywords
        let all_types = self.imports.iter().all(|spec| spec.is_type);
        let use_type_only_import = all_types;

        let mut parts = vec![RcDoc::text("import")];

        // Type-only import (when explicitly set or when all imports are types)
        if use_type_only_import {
            parts.push(RcDoc::space());
            parts.push(RcDoc::text("type"));
        }

        // Format specifiers
        let specifier_docs: Vec<RcDoc<()>> = self
            .imports
            .iter()
            .map(|spec| {
                let mut spec_parts = Vec::new();
                // Add inline `type` keyword only if:
                // - This specifier is a type AND
                // - We're not using `import type { ... }` (which already marks all as types)
                if spec.is_type && !use_type_only_import {
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

        RcDoc::concat(parts)
    }
}
