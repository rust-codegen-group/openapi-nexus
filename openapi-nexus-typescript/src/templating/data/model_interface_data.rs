//! Model interface data for template generation

use serde::Serialize;

use crate::ast::common::TsDocComment;
use crate::ast::common::TsProperty;
use crate::ast::ty::TsInterfaceDefinition;
use crate::ast::ty::TsInterfaceSignature;

/// Simplified property metadata for template helpers
#[derive(Debug, Clone, Serialize)]
pub struct PropertyMetadata {
    /// The camelCase property name used in the TypeScript interface
    pub name: String,
    /// The original property name from the OpenAPI spec (used for JSON serialization)
    pub original_name: String,
    /// Whether the property is optional in the TypeScript interface
    pub optional: bool,
    /// Whether the property is an index signature (e.g., `[key: string]: ValueType`)
    pub is_index_signature: bool,
}

/// Model interface data for template context
#[derive(Debug, Clone, Serialize)]
pub struct ModelInterfaceData {
    pub signature: TsInterfaceSignature,
    pub properties: Vec<TsProperty>,
    pub documentation: Option<TsDocComment>,
    pub required_prop_names: Vec<String>,
    pub property_metadata: Vec<PropertyMetadata>,
}

impl ModelInterfaceData {
    /// Create new model interface data from a TsInterfaceDefinition
    pub fn from_interface(interface: &TsInterfaceDefinition) -> Self {
        // Extract required property names
        let required_prop_names: Vec<String> = interface
            .properties
            .iter()
            .filter(|p| !p.optional)
            .map(|p| p.name.clone())
            .filter(|name| !name.starts_with('['))
            .collect();

        // Extract property metadata
        let property_metadata: Vec<PropertyMetadata> = interface
            .properties
            .iter()
            .map(|p| PropertyMetadata {
                name: p.name.clone(),
                original_name: p.original_name.clone(),
                optional: p.optional,
                is_index_signature: p.name.starts_with('['),
            })
            .collect();

        Self {
            signature: interface.signature.clone(),
            properties: interface.properties.clone(),
            documentation: interface.documentation.clone(),
            required_prop_names,
            property_metadata,
        }
    }
}
