//! Generic template filter for formatting any type implementing ToRcDocWithContext

use minijinja::value::ViaDeserialize;

use crate::ast::{
    TsEnumDefinition, TsInterfaceDefinition, TsTypeAliasDefinition, TsTypeDefinition,
};
use crate::config::MAX_LINE_WIDTH;
use openapi_nexus_core::traits::{EmissionContext, ToRcDocWithContext};

/// Input type for generic rc_doc filter
/// Supports all types that implement ToRcDocWithContext
#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub enum RcDocInput {
    /// TypeScript type definition
    TypeDefinition(TsTypeDefinition),
    /// TypeScript interface definition
    Interface(TsInterfaceDefinition),
    /// TypeScript type alias definition
    TypeAlias(TsTypeAliasDefinition),
    /// TypeScript enum definition
    Enum(TsEnumDefinition),
}

/// Generic template filter for formatting any value as TypeScript string
/// Works with any type implementing ToRcDocWithContext trait
pub fn format_rc_doc_filter(
    value: ViaDeserialize<RcDocInput>,
    indent_level: Option<usize>,
) -> Result<String, minijinja::Error> {
    let ctx = EmissionContext {
        indent: indent_level.unwrap_or(0),
        max_line_width: MAX_LINE_WIDTH,
    };

    match value.0 {
        RcDocInput::TypeDefinition(type_def) => type_def
            .to_rcdoc_with_context(&ctx)
            .map(|doc| doc.pretty(MAX_LINE_WIDTH).to_string())
            .map_err(|e| {
                minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    format!("Failed to render type definition: {:?}", e),
                )
            }),

        RcDocInput::Interface(interface) => interface
            .to_rcdoc_with_context(&ctx)
            .map(|doc| doc.pretty(MAX_LINE_WIDTH).to_string())
            .map_err(|e| {
                minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    format!("Failed to render interface: {:?}", e),
                )
            }),

        RcDocInput::TypeAlias(type_alias) => type_alias
            .to_rcdoc_with_context(&ctx)
            .map(|doc| doc.pretty(MAX_LINE_WIDTH).to_string())
            .map_err(|e| {
                minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    format!("Failed to render type alias: {:?}", e),
                )
            }),

        RcDocInput::Enum(enum_def) => enum_def
            .to_rcdoc_with_context(&ctx)
            .map(|doc| doc.pretty(MAX_LINE_WIDTH).to_string())
            .map_err(|e| {
                minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    format!("Failed to render enum: {:?}", e),
                )
            }),
    }
}
