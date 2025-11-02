//! Interface data for template generation

use serde::{Deserialize, Serialize};

use crate::ast::common::TsDocComment;
use crate::ast::common::TsProperty;
use crate::ast::ty::TsInterfaceDefinition;
use crate::ast::ty::TsInterfaceSignature;

/// Simplified property metadata for template helpers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyMetadata {
    pub name: String,
    pub optional: bool,
    pub is_index_signature: bool,
}

/// Interface data for template context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceData {
    pub signature: TsInterfaceSignature,
    pub properties: Vec<TsProperty>,
    pub documentation: Option<TsDocComment>,
    pub required_prop_names: Vec<String>,
    pub property_metadata: Vec<PropertyMetadata>,
}

impl InterfaceData {
    /// Create new interface data from a TsInterfaceDefinition
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
