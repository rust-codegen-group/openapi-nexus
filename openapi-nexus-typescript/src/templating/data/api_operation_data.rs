//! API operation data for template generation

use serde::{Deserialize, Serialize};

use crate::ast::TsInterfaceDefinition;
use crate::ast::class_definition::TsClassDefinition;
use crate::ast::class_definition::TsImportStatement;

/// API operation data for template context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiOperationData {
    pub class: TsClassDefinition,
    pub imports: Vec<TsImportStatement>,
    pub api_interface: TsInterfaceDefinition,
}

impl ApiOperationData {
    /// Create new API operation data
    pub fn new(
        class: TsClassDefinition,
        imports: Vec<TsImportStatement>,
        api_interface: TsInterfaceDefinition,
    ) -> Self {
        Self {
            class,
            imports,
            api_interface,
        }
    }
}
