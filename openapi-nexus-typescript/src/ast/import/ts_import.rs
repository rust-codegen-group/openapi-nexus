use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use crate::emission::error::EmitError;
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

    /// Format import as TypeScript string
    pub fn to_typescript_string(&self) -> String {
        if self.specifiers.is_empty() {
            return format!("import '{}';", self.module_path);
        }

        let mut parts = Vec::new();
        parts.push("import".to_string());

        if self.is_type_only {
            parts.push("type".to_string());
        }

        // Format specifiers
        let specifier_strings: Vec<String> = self
            .specifiers
            .iter()
            .map(|spec| {
                let mut s = String::new();
                if spec.is_type && !self.is_type_only {
                    s.push_str("type ");
                }
                s.push_str(&spec.name);
                if let Some(alias) = &spec.alias {
                    s.push_str(" as ");
                    s.push_str(alias);
                }
                s
            })
            .collect();

        if specifier_strings.len() == 1 {
            parts.push(format!("{{ {} }}", specifier_strings[0]));
        } else {
            parts.push(format!("{{ {} }}", specifier_strings.join(", ")));
        }

        parts.push("from".to_string());
        parts.push(format!("'{}'", self.module_path));

        format!("{};", parts.join(" "))
    }
}

impl ToRcDoc for TsImport {
    type Error = EmitError;

    fn to_rcdoc(&self) -> Result<RcDoc<'static, ()>, EmitError> {
        Ok(RcDoc::text(self.to_typescript_string()))
    }
}
