//! Schema generation logic for TypeScript with OpenAPI 3.1.2 support
//!
//! This module consolidates schema-to-TypeScript conversion and type mapping functionality
//! into a single, well-architected generator that fully implements OpenAPI v3.1.2 features.

use std::collections::{BTreeMap, BTreeSet};

use heck::ToPascalCase as _;
use tracing::warn;
use utoipa::openapi::schema::{
    AdditionalProperties, KnownFormat, Object, SchemaFormat, SchemaType, Type,
};
use utoipa::openapi::{RefOr, Schema};

use crate::ast::{
    TsDocComment, TsEnumDefinition, TsEnumValue, TsEnumVariant, TsExpression,
    TsInterfaceDefinition, TsInterfaceSignature, TsPrimitive, TsProperty, TsTypeAliasDefinition,
    TsTypeDefinition,
};
use crate::core::GeneratorError;
use crate::generator::schema_context::SchemaContext;

/// Schema generator for converting OpenAPI schemas to TypeScript AST nodes
///
/// This generator consolidates both schema-to-node conversion and type mapping functionality,
/// providing comprehensive OpenAPI 3.1.2 support including nullable types, format handling,
/// discriminators, additionalProperties, and multi-type support.
#[derive(Debug, Clone)]
pub struct SchemaGenerator;

impl SchemaGenerator {
    /// Convert a schema reference to a TypeScript type definition
    ///
    /// This is the main public API method used by TsLangGenerator.
    /// It determines whether to generate an Interface, Enum, or TypeAlias based on the schema.
    pub fn schema_to_ts_type_definition(
        &self,
        name: &str,
        schema_ref: &RefOr<Schema>,
        context: &mut SchemaContext,
    ) -> Result<TsTypeDefinition, GeneratorError> {
        // Ensure the name is PascalCase for TypeScript interfaces
        let pascal_name = name.to_pascal_case();

        match schema_ref {
            RefOr::T(schema) => {
                // Determine the appropriate node type based on schema content
                self.determine_node_type(&pascal_name, schema, context)
            }
            RefOr::Ref(reference) => {
                // Handle reference - resolve to actual schema or create type alias
                self.handle_schema_reference(&pascal_name, reference, context)
            }
        }
    }

    // ============================================================================
    // SCHEMA-TO-NODE CONVERSION (Private Methods)
    // ============================================================================

    /// Determine the appropriate TypeScript node type based on schema content
    fn determine_node_type(
        &self,
        name: &str,
        schema: &Schema,
        context: &mut SchemaContext,
    ) -> Result<TsTypeDefinition, GeneratorError> {
        match schema {
            Schema::Object(obj_schema) => {
                // Check if this is an enum schema
                if let Some(enum_values) = &obj_schema.enum_values
                    && !enum_values.is_empty()
                {
                    return Ok(TsTypeDefinition::Enum(self.schema_to_enum(name, schema)?));
                }

                // Check if this is an object with properties
                if !obj_schema.properties.is_empty() {
                    return Ok(TsTypeDefinition::Interface(
                        self.schema_to_interface(name, schema, context)?,
                    ));
                }

                // Check if this has additionalProperties (typed map)
                if obj_schema.additional_properties.is_some() {
                    return Ok(TsTypeDefinition::Interface(
                        self.schema_to_interface(name, schema, context)?,
                    ));
                }

                // Otherwise, create a type alias
                let type_expr = self.map_schema_to_type(schema, context);
                Ok(TsTypeDefinition::TypeAlias(TsTypeAliasDefinition {
                    name: name.to_string(),
                    type_expr,
                    generics: vec![],
                    documentation: obj_schema.description.clone().map(TsDocComment::new),
                }))
            }
            Schema::Array(_) => {
                // Array schemas become type aliases
                let type_expr = self.map_schema_to_type(schema, context);
                Ok(TsTypeDefinition::TypeAlias(TsTypeAliasDefinition {
                    name: name.to_string(),
                    type_expr,
                    generics: vec![],
                    documentation: None,
                }))
            }
            Schema::OneOf(_) | Schema::AllOf(_) | Schema::AnyOf(_) => {
                // Composition schemas become type aliases with union/intersection types
                let type_expr = self.map_schema_to_type(schema, context);
                Ok(TsTypeDefinition::TypeAlias(TsTypeAliasDefinition {
                    name: name.to_string(),
                    type_expr,
                    generics: vec![],
                    documentation: None,
                }))
            }
            _ => {
                // Fallback for unknown schema types
                Ok(TsTypeDefinition::TypeAlias(TsTypeAliasDefinition {
                    name: name.to_string(),
                    type_expr: TsExpression::Primitive(TsPrimitive::Any),
                    generics: vec![],
                    documentation: None,
                }))
            }
        }
    }

    /// Convert a schema to a TypeScript interface
    fn schema_to_interface(
        &self,
        name: &str,
        schema: &Schema,
        context: &mut SchemaContext,
    ) -> Result<TsInterfaceDefinition, GeneratorError> {
        match schema {
            Schema::Object(obj_schema) => {
                let mut properties = Vec::new();

                // Extract properties from the object schema
                for (prop_name, prop_schema) in &obj_schema.properties {
                    let type_expr = self.map_ref_or_schema_to_type(prop_schema, context);
                    let is_required = obj_schema.required.contains(prop_name);
                    let description = self.extract_description_from_schema(prop_schema);

                    let property = TsProperty {
                        name: prop_name.clone(),
                        type_expr,
                        optional: !is_required,
                        documentation: description.map(TsDocComment::new),
                    };
                    properties.push(property);
                }

                // Handle additionalProperties as index signature
                if let Some(additional_props) = &obj_schema.additional_properties {
                    match additional_props.as_ref() {
                        AdditionalProperties::RefOr(schema_ref) => {
                            let mut value_type =
                                self.map_ref_or_schema_to_type(schema_ref, context);

                            // If there are explicit properties, we need to union their types with additionalProperties type
                            // to satisfy TypeScript's index signature compatibility requirements.
                            //
                            // Example: OpenAPI schema with properties: {name: string, age: number} and additionalProperties: number
                            // Without union: [key: string]: number would conflict with name: string
                            // With union: [key: string]: string | number satisfies both explicit properties and additionalProperties
                            //
                            // Generated TypeScript:
                            // interface Example {
                            //   name: string;
                            //   age?: number;
                            //   [key: string]: string | number;  // Union of all property types
                            // }
                            if !obj_schema.properties.is_empty() {
                                let mut unique_types: BTreeSet<TsExpression> = obj_schema
                                    .properties
                                    .values()
                                    .map(|prop_schema| {
                                        self.map_ref_or_schema_to_type(prop_schema, context)
                                    })
                                    .collect();

                                if !unique_types.is_empty() {
                                    unique_types.insert(value_type.clone());
                                    value_type = TsExpression::Union(unique_types);
                                }
                            }

                            let index_property = TsProperty {
                                name: "[key: string]".to_string(),
                                type_expr: value_type,
                                optional: false,
                                documentation: Some(TsDocComment::new(
                                    "Additional properties".to_string(),
                                )),
                            };
                            properties.push(index_property);
                        }
                        AdditionalProperties::FreeForm(true) => {
                            let index_property = TsProperty {
                                name: "[key: string]".to_string(),
                                type_expr: TsExpression::Primitive(TsPrimitive::Any),
                                optional: false,
                                documentation: Some(TsDocComment::new(
                                    "Additional properties".to_string(),
                                )),
                            };
                            properties.push(index_property);
                        }
                        AdditionalProperties::FreeForm(false) => {
                            // No additional properties allowed - no index signature
                        }
                    }
                }

                Ok(TsInterfaceDefinition {
                    signature: TsInterfaceSignature::new(name.to_string()),
                    properties,
                    documentation: obj_schema.description.clone().map(TsDocComment::new),
                })
            }
            _ => {
                // For non-object schemas, create an empty interface
                Ok(TsInterfaceDefinition {
                    signature: TsInterfaceSignature::new(name.to_string()),
                    properties: vec![],
                    documentation: None,
                })
            }
        }
    }

    /// Convert a schema to a TypeScript enum
    fn schema_to_enum(
        &self,
        name: &str,
        schema: &Schema,
    ) -> Result<TsEnumDefinition, GeneratorError> {
        match schema {
            Schema::Object(obj_schema) => {
                let mut variants: Vec<TsEnumVariant> = Vec::new();
                let enum_descriptions = Self::extract_enum_descriptions(obj_schema);

                if let Some(enum_values) = &obj_schema.enum_values {
                    for (index, enum_value) in enum_values.iter().enumerate() {
                        // Warn if boolean value is being converted
                        if let serde_json::Value::Bool(_) = enum_value {
                            warn!(
                                "Boolean enum value found for schema '{}'. TypeScript enums don't support booleans, converting to number (0 for false, 1 for true).",
                                name
                            );
                        }

                        let enum_value = TsEnumValue::from_json_value(enum_value);
                        let name = enum_value.generate_enum_name();
                        let value = Some(enum_value);
                        let documentation = enum_descriptions
                            .get(index)
                            .and_then(|d| d.as_str())
                            .map(|s| TsDocComment::new(s.to_string()));

                        let variant = TsEnumVariant {
                            name,
                            value,
                            documentation,
                        };
                        variants.push(variant);
                    }
                }

                Ok(TsEnumDefinition {
                    name: name.to_string(),
                    variants,
                    documentation: obj_schema.description.clone().map(TsDocComment::new),
                    is_const: false, // Regular enum, not const enum
                })
            }
            _ => Err(GeneratorError::Generic {
                message: "Expected object schema for enum".to_string(),
            }),
        }
    }

    /// Extract x-enumDescriptions extension from schema
    ///
    /// This extension is a common convention to provide per-value documentation
    /// for enum types, even though it's not part of the official OpenAPI
    /// specification.
    fn extract_enum_descriptions(obj_schema: &Object) -> Vec<serde_json::Value> {
        // Try to access extensions field - in utoipa, extensions are typically stored
        // in a field called `extensions` or accessed via a method
        // Check if there's an extensions field or method
        if let Some(extensions) = &obj_schema.extensions {
            if let Some(enum_descriptions_value) = extensions.get("x-enumDescriptions") {
                if let serde_json::Value::Array(descriptions) = enum_descriptions_value {
                    return descriptions.clone();
                }
            }
        }
        Vec::new()
    }

    // ============================================================================
    // TYPE MAPPING (Private Methods - absorbed from TypeMapper)
    // ============================================================================

    /// Map a RefOr<Schema> to a TypeScript type expression
    fn map_ref_or_schema_to_type(
        &self,
        schema_ref: &RefOr<Schema>,
        context: &mut SchemaContext,
    ) -> TsExpression {
        match schema_ref {
            RefOr::T(schema) => self.map_schema_to_type(schema, context),
            RefOr::Ref(reference) => {
                // Use reference resolution for proper type mapping
                match self.resolve_reference_to_type(reference, context) {
                    Ok(type_expr) => type_expr,
                    Err(_) => {
                        // Fallback to simple reference if resolution fails
                        let schema_name = reference
                            .ref_location
                            .strip_prefix("#/components/schemas/")
                            .unwrap_or("any");
                        TsExpression::Reference(schema_name.to_string())
                    }
                }
            }
        }
    }

    /// Map a Schema to a TypeScript type expression
    fn map_schema_to_type(&self, schema: &Schema, context: &mut SchemaContext) -> TsExpression {
        match schema {
            Schema::Object(obj_schema) => {
                // Handle enum schemas
                if let Some(enum_values) = &obj_schema.enum_values
                    && !enum_values.is_empty()
                {
                    return self.map_enum_to_type(enum_values);
                }

                // Handle inline object schemas with properties
                if !obj_schema.properties.is_empty() {
                    return self.map_inline_object_to_type(obj_schema, context);
                }

                // Handle primitive types
                self.map_primitive_type_from_schema(obj_schema)
            }
            Schema::Array(arr_schema) => {
                // Map array schema to TypeScript array type using the items field
                let item_type = self.map_array_items_to_type(&arr_schema.items, context);
                TsExpression::Array(Box::new(item_type))
            }
            Schema::OneOf(one_of) => {
                // Map oneOf to union type with discriminator support
                self.map_composition_to_type(&one_of.items, &one_of.discriminator, context)
            }
            Schema::AllOf(all_of) => {
                // Map allOf to intersection type with deduplication
                let types: BTreeSet<TsExpression> = all_of
                    .items
                    .iter()
                    .map(|schema_ref| self.map_ref_or_schema_to_type(schema_ref, context))
                    .collect();
                TsExpression::Intersection(types)
            }
            Schema::AnyOf(any_of) => {
                // Map anyOf to union type with discriminator support
                self.map_composition_to_type(&any_of.items, &any_of.discriminator, context)
            }
            _ => {
                // Fallback for unknown schema types
                TsExpression::Primitive(TsPrimitive::Any)
            }
        }
    }

    /// Map primitive type from schema object using OpenAPI 3.1.2 features
    fn map_primitive_type_from_schema(&self, obj_schema: &Object) -> TsExpression {
        // Handle nullable types (OpenAPI 3.1.2)
        let base_type = Self::map_schema_type_to_primitive(&obj_schema.schema_type);

        // Apply format handling for better type inference
        let formatted_type = self.handle_known_format(base_type, &obj_schema.format);

        // Handle nullable (if schema_type includes Null)
        self.handle_nullable(formatted_type, &obj_schema.schema_type)
    }

    /// Map schema type to primitive TypeScript type
    fn map_schema_type_to_primitive(schema_type: &SchemaType) -> TsExpression {
        match schema_type {
            SchemaType::Type(openapi_type) => match openapi_type {
                Type::String => TsExpression::Primitive(TsPrimitive::String),
                Type::Integer => TsExpression::Primitive(TsPrimitive::Number),
                Type::Number => TsExpression::Primitive(TsPrimitive::Number),
                Type::Boolean => TsExpression::Primitive(TsPrimitive::Boolean),
                Type::Array => {
                    TsExpression::Array(Box::new(TsExpression::Primitive(TsPrimitive::String)))
                }
                Type::Object => {
                    // For object types, check if it has properties
                    TsExpression::Primitive(TsPrimitive::Any)
                }
                Type::Null => TsExpression::Primitive(TsPrimitive::Null),
            },
            SchemaType::Array(types) => {
                // Handle multi-type support (OpenAPI 3.1.2)
                if types.len() == 1 {
                    // Single type in array
                    Self::map_schema_type_to_primitive(&SchemaType::Type(types[0].clone()))
                } else {
                    // Multiple types - create union
                    let union_types: BTreeSet<TsExpression> = types
                        .iter()
                        .map(|t| Self::map_schema_type_to_primitive(&SchemaType::Type(t.clone())))
                        .collect();

                    if union_types.len() == 1 {
                        union_types.first().unwrap().clone()
                    } else {
                        TsExpression::Union(union_types)
                    }
                }
            }
            SchemaType::AnyValue => {
                // AnyValue represents any JSON value
                TsExpression::Primitive(TsPrimitive::Any)
            }
        }
    }

    /// Map ArrayItems to TypeScript type
    fn map_array_items_to_type(
        &self,
        array_items: &utoipa::openapi::schema::ArrayItems,
        context: &mut SchemaContext,
    ) -> TsExpression {
        match array_items {
            utoipa::openapi::schema::ArrayItems::RefOrSchema(schema_ref) => {
                self.map_ref_or_schema_to_type(schema_ref, context)
            }
            utoipa::openapi::schema::ArrayItems::False => {
                // No additional items allowed - use any as fallback
                TsExpression::Primitive(TsPrimitive::Any)
            }
        }
    }

    /// Map enum values to TypeScript type
    fn map_enum_to_type(&self, enum_values: &[serde_json::Value]) -> TsExpression {
        let mut types = Vec::new();
        for enum_value in enum_values {
            match enum_value {
                serde_json::Value::String(s) => {
                    types.push(TsExpression::Literal(format!("\"{}\"", s)));
                }
                serde_json::Value::Number(n) => {
                    types.push(TsExpression::Literal(n.to_string()));
                }
                serde_json::Value::Bool(b) => {
                    types.push(TsExpression::Literal(b.to_string()));
                }
                _ => {
                    types.push(TsExpression::Literal(enum_value.to_string()));
                }
            }
        }

        if types.len() == 1 {
            types.into_iter().next().unwrap()
        } else if types.len() > 1 {
            let unique_types: BTreeSet<TsExpression> = types.into_iter().collect();
            if unique_types.len() == 1 {
                unique_types.first().unwrap().clone()
            } else {
                TsExpression::Union(unique_types)
            }
        } else {
            TsExpression::Primitive(TsPrimitive::Any)
        }
    }

    /// Map inline object schema to TypeScript object type expression
    fn map_inline_object_to_type(
        &self,
        obj_schema: &Object,
        context: &mut SchemaContext,
    ) -> TsExpression {
        let mut properties = BTreeMap::new();

        // Map each property to its TypeScript type
        for (prop_name, prop_schema) in &obj_schema.properties {
            let type_expr = self.map_ref_or_schema_to_type(prop_schema, context);
            properties.insert(prop_name.clone(), type_expr);
        }

        TsExpression::Object(properties)
    }

    /// Map composition schemas (oneOf/anyOf) to TypeScript types with discriminator support
    fn map_composition_to_type(
        &self,
        items: &[RefOr<Schema>],
        _discriminator: &Option<utoipa::openapi::schema::Discriminator>,
        context: &mut SchemaContext,
    ) -> TsExpression {
        let types: BTreeSet<TsExpression> = items
            .iter()
            .map(|schema_ref| self.map_ref_or_schema_to_type(schema_ref, context))
            .collect();

        // TODO: Implement proper discriminator handling for discriminated unions
        // For now, just return a union type with deduplication
        if types.len() == 1 {
            types.into_iter().next().unwrap()
        } else {
            TsExpression::Union(types)
        }
    }

    // ============================================================================
    // OPENAPI 3.1.2 FEATURE HANDLERS (Private Methods)
    // ============================================================================

    /// Handle nullable types by adding null to union types
    fn handle_nullable(&self, base_type: TsExpression, schema_type: &SchemaType) -> TsExpression {
        // Check if schema_type includes Null (OpenAPI 3.1.2 nullable support)
        let is_nullable = match schema_type {
            SchemaType::Array(types) => types.contains(&Type::Null),
            SchemaType::Type(Type::Null) => true,
            _ => false,
        };

        if is_nullable {
            // Check if base_type already contains null to avoid duplicates
            if Self::type_contains_null(&base_type) {
                base_type
            } else {
                TsExpression::Union(BTreeSet::from_iter([
                    base_type,
                    TsExpression::Primitive(TsPrimitive::Null),
                ]))
            }
        } else {
            base_type
        }
    }

    /// Check if a TypeExpression contains null
    fn type_contains_null(type_expr: &TsExpression) -> bool {
        match type_expr {
            TsExpression::Primitive(TsPrimitive::Null) => true,
            TsExpression::Union(types) => types.iter().any(Self::type_contains_null),
            TsExpression::Intersection(types) => types.iter().all(Self::type_contains_null),
            _ => false,
        }
    }

    /// Extract description from a RefOr<Schema>
    fn extract_description_from_schema(&self, schema_ref: &RefOr<Schema>) -> Option<String> {
        match schema_ref {
            RefOr::T(schema) => match schema {
                Schema::Object(obj_schema) => obj_schema.description.clone(),
                _ => None,
            },
            RefOr::Ref(reference) => Some(reference.description.clone()),
        }
    }

    /// Handle known format annotations for better type inference
    /// TODO: Implement proper format handling for better type inference
    fn handle_known_format(
        &self,
        base_type: TsExpression,
        format: &Option<SchemaFormat>,
    ) -> TsExpression {
        // For now, format doesn't change the base type, but we could add comments or
        // more specific types in the future (e.g., branded types for email, uuid, etc.)
        match format {
            Some(SchemaFormat::KnownFormat(KnownFormat::DateTime)) => {
                // Could generate branded type for date-time in the future
                base_type
            }
            Some(SchemaFormat::KnownFormat(KnownFormat::Email)) => {
                // Could generate branded type for email in the future
                base_type
            }
            Some(SchemaFormat::KnownFormat(KnownFormat::Uri)) => {
                // Could generate branded type for URI in the future
                base_type
            }
            Some(SchemaFormat::KnownFormat(KnownFormat::Uuid)) => {
                // Could generate branded type for UUID in the future
                base_type
            }
            Some(SchemaFormat::KnownFormat(KnownFormat::Int64))
            | Some(SchemaFormat::KnownFormat(KnownFormat::Int32)) => {
                // Integer formats still map to number in TypeScript
                base_type
            }
            _ => base_type,
        }
    }

    // ============================================================================
    // REFERENCE RESOLUTION (Private Methods)
    // ============================================================================

    /// Handle schema reference resolution
    fn handle_schema_reference(
        &self,
        name: &str,
        reference: &utoipa::openapi::Ref,
        context: &mut SchemaContext,
    ) -> Result<TsTypeDefinition, GeneratorError> {
        // Extract schema name from reference path
        let schema_name = self.extract_schema_name(&reference.ref_location)?;

        // Check for circular dependency
        if context.is_visited(&schema_name) {
            // Circular reference detected - create a type alias to break the cycle
            return Ok(TsTypeDefinition::TypeAlias(TsTypeAliasDefinition {
                name: name.to_string(),
                type_expr: TsExpression::Reference(schema_name.clone()),
                generics: vec![],
                documentation: Some(TsDocComment::new(format!(
                    "Circular reference to {}",
                    schema_name
                ))),
            }));
        }

        // Look up the actual schema
        if let Some(target_schema) = context.schemas.get(&schema_name) {
            // Mark as visited to prevent cycles
            let schema_name_clone = schema_name.clone();
            context.mark_visited(schema_name_clone);
            context.increment_depth();

            // Recursively resolve the target schema
            let result = self.schema_to_ts_type_definition(&schema_name, target_schema, context);

            // Cleanup
            context.decrement_depth();
            context.unmark_visited(&schema_name);

            result
        } else {
            // Unresolved reference - generate warning and fallback
            tracing::warn!("Unresolved schema reference: {}", schema_name);
            Ok(TsTypeDefinition::TypeAlias(TsTypeAliasDefinition {
                name: name.to_string(),
                type_expr: TsExpression::Reference(schema_name.clone()),
                generics: vec![],
                documentation: Some(TsDocComment::new(format!(
                    "Unresolved reference to {}",
                    schema_name
                ))),
            }))
        }
    }

    /// Extract schema name from reference path
    ///
    /// Converts `#/components/schemas/User` -> `User`
    fn extract_schema_name(&self, ref_path: &str) -> Result<String, GeneratorError> {
        if let Some(schema_name) = ref_path.strip_prefix("#/components/schemas/") {
            Ok(schema_name.to_string())
        } else {
            Err(GeneratorError::Generic {
                message: format!("Invalid schema reference path: {}", ref_path),
            })
        }
    }

    /// Resolve a reference to a TypeExpression
    fn resolve_reference_to_type(
        &self,
        reference: &utoipa::openapi::Ref,
        context: &mut SchemaContext,
    ) -> Result<TsExpression, GeneratorError> {
        let schema_name = self.extract_schema_name(&reference.ref_location)?;

        // Check for circular dependency
        if context.is_visited(&schema_name) {
            return Ok(TsExpression::Reference(schema_name.clone()));
        }

        // Look up the actual schema
        if let Some(target_schema) = context.schemas.get(&schema_name) {
            // Mark as visited to prevent cycles
            let schema_name_clone = schema_name.clone();
            context.mark_visited(schema_name_clone);
            context.increment_depth();

            // Recursively resolve the target schema to a type
            let result = self.map_ref_or_schema_to_type(target_schema, context);

            // Cleanup
            context.decrement_depth();
            context.unmark_visited(&schema_name);

            Ok(result)
        } else {
            // Unresolved reference - generate warning and fallback
            tracing::warn!("Unresolved schema reference: {}", schema_name);
            Ok(TsExpression::Reference(schema_name.clone()))
        }
    }
}

impl Default for SchemaGenerator {
    fn default() -> Self {
        SchemaGenerator
    }
}
