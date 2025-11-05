//! Generic template filter for formatting any type implementing ToRcDoc

use minijinja::value::ViaDeserialize;

use crate::ast::{
    TsDocComment, TsEnumDefinition, TsExpression, TsInterfaceDefinition, TsInterfaceSignature,
    TsParameter, TsProperty, TsTypeAliasDefinition, TsTypeDefinition,
};
use crate::config::MAX_LINE_WIDTH;
use crate::templating::data::{ApiClassSignature, ApiImportStatement, ApiMethodData};
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
    /// TypeScript documentation comment
    DocComment(TsDocComment),
    /// TypeScript expression
    Expression(TsExpression),
    /// TypeScript type definition
    TypeDefinition(TsTypeDefinition),
    /// TypeScript interface definition
    Interface(TsInterfaceDefinition),
    /// TypeScript type alias definition
    TypeAlias(TsTypeAliasDefinition),
    /// TypeScript enum definition
    Enum(TsEnumDefinition),
    /// TypeScript property
    Property(TsProperty),
    /// TypeScript parameter
    Parameter(TsParameter),
    /// TypeScript interface signature
    InterfaceSignature(TsInterfaceSignature),
    /// API import statement
    ImportStatement(ApiImportStatement),
    /// API class signature
    ClassSignature(ApiClassSignature),
    /// API method data
    MethodSignature(ApiMethodData),
}

/// Generic template filter for formatting any value as TypeScript string
/// Works with any type implementing ToRcDoc trait
pub fn fmt_filter(value: ViaDeserialize<RcDocInput>) -> String {
    match value.0 {
        RcDocInput::DocComment(doc_comment) => fmt_input!(doc_comment, "doc comment"),
        RcDocInput::Expression(expr) => fmt_input!(expr, "expression"),
        RcDocInput::TypeDefinition(type_def) => fmt_input!(type_def, "type definition"),
        RcDocInput::Interface(interface) => fmt_input!(interface, "interface"),
        RcDocInput::TypeAlias(type_alias) => fmt_input!(type_alias, "type alias"),
        RcDocInput::Enum(enum_def) => fmt_input!(enum_def, "enum"),
        RcDocInput::Property(property) => fmt_input!(property, "property"),
        RcDocInput::Parameter(parameter) => fmt_input!(parameter, "parameter"),
        RcDocInput::InterfaceSignature(signature) => fmt_input!(signature, "interface signature"),
        RcDocInput::ImportStatement(import) => fmt_input!(import, "import statement"),
        RcDocInput::ClassSignature(signature) => fmt_input!(signature, "class signature"),
        RcDocInput::MethodSignature(method) => fmt_input!(method, "method signature"),
    }
}
