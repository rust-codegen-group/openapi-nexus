//! Model enum data for template generation

use serde::Serialize;

use crate::ast::ty::TsEnumDefinition;

/// Model enum data for template context
#[derive(Debug, Clone, Serialize)]
pub struct ModelEnumData {
    pub enum_definition: TsEnumDefinition,
}
