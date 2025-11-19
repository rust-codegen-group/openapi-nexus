//! Model struct data for template rendering

use serde::{Deserialize, Serialize};

use crate::ast::GoStruct;
use crate::templating::data::CommonFileHeaderData;

/// Model struct data for template rendering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStructData {
    pub struct_definition: GoStruct,
    pub imports: Vec<String>,
    pub required_fields: Vec<String>,
    pub common_file_header: CommonFileHeaderData,
}

impl ModelStructData {
    pub fn new(struct_definition: GoStruct, common_file_header: CommonFileHeaderData) -> Self {
        Self {
            struct_definition,
            imports: Vec::new(),
            required_fields: Vec::new(),
            common_file_header,
        }
    }

    pub fn with_imports(mut self, imports: Vec<String>) -> Self {
        self.imports = imports;
        self
    }

    pub fn with_required_fields(mut self, required_fields: Vec<String>) -> Self {
        self.required_fields = required_fields;
        self
    }
}
