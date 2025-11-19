//! Model type alias data for template rendering

use serde::{Deserialize, Serialize};

use crate::ast::GoTypeAlias;
use crate::templating::data::CommonFileHeaderData;

/// Model type alias data for template rendering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelTypeAliasData {
    pub type_alias_definition: GoTypeAlias,
    pub imports: Vec<String>,
    pub common_file_header: CommonFileHeaderData,
}

impl ModelTypeAliasData {
    pub fn new(
        type_alias_definition: GoTypeAlias,
        common_file_header: CommonFileHeaderData,
    ) -> Self {
        Self {
            type_alias_definition,
            imports: Vec::new(),
            common_file_header,
        }
    }

    pub fn with_imports(mut self, imports: Vec<String>) -> Self {
        self.imports = imports;
        self
    }
}
