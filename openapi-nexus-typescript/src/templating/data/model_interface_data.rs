//! Model interface data for template generation

use serde::Serialize;

use crate::ast::common::TsDocComment;
use crate::ast::common::TsProperty;
use crate::ast::ty::TsInterfaceDefinition;
use crate::ast::ty::TsInterfaceSignature;
use crate::ast::{ObjectProperty, TsExpression};
use crate::templating::data::ApiImportStatements;

/// Simplified property metadata for template helpers
#[derive(Debug, Clone, Serialize)]
pub struct PropertyMetadata {
    /// The camelCase property name used in the TypeScript interface
    pub ts_name: String,
    /// The original property name from the OpenAPI spec (used in JSON)
    pub original_name: String,
    /// Whether the property is optional in the TypeScript interface
    pub optional: bool,
    /// Whether the property is an index signature (e.g., `[key: string]: ValueType`)
    pub is_index_signature: bool,
    /// The TypeScript type expression representing the property type
    pub type_expr: TsExpression,
    /// Whether the property is an array type
    pub is_array: bool,
    /// Whether the property is an array of objects (inline or reference)
    pub is_array_of_objects: bool,
    /// Whether the property is a reference to a named type
    pub is_object_reference: bool,
    /// Whether the property is an inline object type
    pub is_inline_object: bool,
    /// The reference name if this is a reference type (for FromJSON calls)
    pub reference_name: Option<String>,
    /// The array item type expression (if this is an array)
    pub array_item_type: Option<TsExpression>,
    /// Property mappings for inline objects (camelCase -> original name)
    pub inline_object_properties: Vec<ObjectProperty>,
}

/// Model interface data for template context
#[derive(Debug, Clone, Serialize)]
pub struct ModelInterfaceData {
    pub signature: TsInterfaceSignature,
    pub properties: Vec<TsProperty>,
    pub documentation: Option<TsDocComment>,
    pub required_prop_names: Vec<String>,
    pub property_metadata: Vec<PropertyMetadata>,
    /// Map of imports needed by the generated model template (types and functions).
    /// Keyed by module_path for easy lookup and modification.
    /// Each import statement can contain both types (with inline `type` keyword) and values.
    pub imports: ApiImportStatements,
}

impl ModelInterfaceData {
    /// Create new model interface data from a TsInterfaceDefinition
    pub fn from_interface(interface: &TsInterfaceDefinition) -> Self {
        // Extract required property names
        let required_prop_names: Vec<String> = interface
            .properties
            .iter()
            .filter(|p| !p.optional)
            .map(|p| p.ts_name.clone())
            .filter(|name| !name.starts_with('['))
            .collect();

        // Extract property metadata
        let property_metadata: Vec<PropertyMetadata> = interface
            .properties
            .iter()
            .map(|p| PropertyMetadata {
                ts_name: p.ts_name.clone(),
                original_name: p.original_name.clone(),
                optional: p.optional,
                is_index_signature: p.ts_name.starts_with('['),
                type_expr: p.type_expr.clone(),
                is_array: p.type_expr.is_array(),
                is_array_of_objects: p.type_expr.is_array_of_objects(),
                is_object_reference: p.type_expr.is_object_reference(),
                is_inline_object: p.type_expr.is_inline_object(),
                reference_name: p.type_expr.reference_name(),
                array_item_type: p.type_expr.array_item_type(),
                inline_object_properties: p.type_expr.object_properties(),
            })
            .collect();

        Self {
            signature: interface.signature.clone(),
            properties: interface.properties.clone(),
            documentation: interface.documentation.clone(),
            required_prop_names,
            property_metadata,
            imports: ApiImportStatements::new(),
        }
    }
}
