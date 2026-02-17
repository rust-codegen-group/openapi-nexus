//! Model interface data for template generation

use heck::ToLowerCamelCase as _;
use serde::Serialize;

use crate::ast::common::TsDocComment;
use crate::ast::common::TsProperty;
use crate::ast::ty::TsInterfaceDefinition;
use crate::ast::ty::TsInterfaceSignature;
use crate::ast::{ObjectProperty, TsExpression};
use crate::templating::data::ApiImportStatements;

/// Required property name information for type checking
#[derive(Debug, Clone, Serialize)]
pub struct RequiredPropertyName {
    /// The camelCase property name used in the TypeScript interface
    pub ts_name: String,
    /// The original property name from the OpenAPI spec (used in JSON)
    pub original_name: String,
    /// Optional enum discriminator value - if this property is an enum discriminator,
    /// this contains the expected enum value for this variant
    pub enum_value: Option<String>,
}

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
    /// Whether the property is a record type (e.g., { [key: string]: T })
    pub is_record_type: bool,
    /// For record types, the reference name of the value type (for recursive FromJSON/ToJSON)
    pub record_value_reference_name: Option<String>,
    /// The reference name if this is a reference type (for FromJSON calls)
    pub reference_name: Option<String>,
    /// The array item type expression (if this is an array)
    pub array_item_type: Option<TsExpression>,
    /// Property mappings for inline objects (camelCase -> original name)
    pub inline_object_properties: Vec<ObjectProperty>,
}

/// Model interface data for template context
///
/// This struct contains all the information needed to generate TypeScript model interfaces
/// and their associated helper functions (FromJSON, ToJSON, instanceOf, etc.) from Jinja2 templates.
#[derive(Debug, Clone, Serialize)]
pub struct ModelInterfaceData {
    /// The interface signature (name, generics, extends clauses)
    pub signature: TsInterfaceSignature,
    /// The full list of properties in the interface
    pub properties: Vec<TsProperty>,
    /// Optional documentation/comment for the interface
    pub documentation: Option<TsDocComment>,
    /// Required property names with both TypeScript names and original JSON names.
    ///
    /// Used by the `instanceOf` function to validate type assertions. The function checks
    /// both `original_name` (for JSON input) and `ts_name` (for TypeScript objects) to
    /// determine if a value matches the interface type.
    pub required_prop_names: Vec<RequiredPropertyName>,
    /// Simplified metadata for each property, used by template helpers for code generation.
    ///
    /// Contains information about property types, optionality, and transformation needs
    /// for generating FromJSON/ToJSON functions.
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
        let required_prop_names: Vec<RequiredPropertyName> = interface
            .properties
            .iter()
            .filter(|p| !p.optional && !p.is_index_signature)
            .map(|p| RequiredPropertyName {
                ts_name: p.ts_name.clone(),
                original_name: p.original_name.clone(),
                enum_value: None, // Will be set by schema generator for tagged enum variants
            })
            .collect();

        // Extract property metadata
        let property_metadata: Vec<PropertyMetadata> = interface
            .properties
            .iter()
            .map(|p| PropertyMetadata {
                ts_name: p.ts_name.clone(),
                original_name: p.original_name.clone(),
                optional: p.optional,
                is_index_signature: p.is_index_signature,
                type_expr: p.type_expr.clone(),
                is_array: p.type_expr.is_array(),
                is_array_of_objects: p.type_expr.is_array_of_objects(),
                is_object_reference: p.type_expr.is_object_reference(),
                is_inline_object: p.type_expr.is_inline_object(),
                is_record_type: p.type_expr.is_record_type(),
                record_value_reference_name: p.type_expr.record_value_reference_name(),
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

    /// Update enum discriminator values from context
    pub fn update_enum_discriminators(
        &mut self,
        interface_name: &str,
        enum_discriminators: &std::collections::HashMap<String, (String, String)>,
    ) {
        if let Some((prop_name, enum_val)) = enum_discriminators.get(interface_name) {
            // Find the property in required_prop_names and set its enum_value
            for req_prop in &mut self.required_prop_names {
                if req_prop.original_name == *prop_name
                    || req_prop.ts_name == prop_name.to_lower_camel_case()
                {
                    req_prop.enum_value = Some(enum_val.clone());
                    break;
                }
            }
        }
    }
}
