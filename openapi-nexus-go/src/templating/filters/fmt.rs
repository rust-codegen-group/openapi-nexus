//! Generic template filter for formatting any type implementing ToRcDoc

use minijinja::value::ViaDeserialize;

use crate::ast::{
    GoDocComment, GoExpression, GoField, GoParameter, GoStruct, GoTypeAlias, GoTypeDefinition,
};
use crate::consts::MAX_LINE_WIDTH;
use openapi_nexus_core::traits::ToRcDoc;

/// Macro to simplify fmt filter match arms
/// Converts a value implementing ToRcDoc to a formatted string
macro_rules! fmt_input {
    ($value:expr, $type_name:literal) => {
        $value.to_rcdoc().pretty(MAX_LINE_WIDTH).to_string()
    };
}

/// Input type for generic fmt filter
/// Supports all types that implement ToRcDoc
#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub enum RcDocInput {
    /// Go documentation comment
    DocComment(GoDocComment),
    /// Go expression
    Expression(GoExpression),
    /// Go type definition
    TypeDefinition(GoTypeDefinition),
    /// Go struct definition
    Struct(GoStruct),
    /// Go type alias definition
    TypeAlias(GoTypeAlias),
    /// Go field
    Field(GoField),
    /// Go parameter
    Parameter(GoParameter),
}

/// Generic template filter for formatting any value as Go string
/// Works with any type implementing ToRcDoc trait
pub fn fmt_filter(value: ViaDeserialize<RcDocInput>) -> String {
    match value.0 {
        RcDocInput::DocComment(doc_comment) => fmt_input!(doc_comment, "doc comment"),
        RcDocInput::Expression(expr) => fmt_input!(expr, "expression"),
        RcDocInput::TypeDefinition(type_def) => fmt_input!(type_def, "type definition"),
        RcDocInput::Struct(struct_def) => fmt_input!(struct_def, "struct"),
        RcDocInput::TypeAlias(type_alias) => fmt_input!(type_alias, "type alias"),
        RcDocInput::Field(field) => fmt_input!(field, "field"),
        RcDocInput::Parameter(parameter) => fmt_input!(parameter, "parameter"),
    }
}
