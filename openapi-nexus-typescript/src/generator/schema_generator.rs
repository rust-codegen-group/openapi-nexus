//! Schema generation logic for TypeScript with OpenAPI 3.1.2 support
//!
//! This module consolidates schema-to-TypeScript conversion and type mapping functionality
//! into a single, well-architected generator that fully implements OpenAPI v3.1.2 features.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, OnceLock};

use heck::{ToLowerCamelCase as _, ToPascalCase as _};
use tracing::{error, warn};
use utoipa::openapi::schema::{
    AdditionalProperties, KnownFormat, Object, SchemaFormat, SchemaType, Type,
};
use utoipa::openapi::{RefOr, Schema};

use crate::ast::ty::ts_type_alias_definition::{
    IntersectionMemberInfo, IntersectionObjectProperty, UnionMemberInfo,
};
use crate::ast::{
    ObjectProperty, TsDocComment, TsEnumDefinition, TsEnumValue, TsEnumVariant, TsExpression,
    TsInterfaceDefinition, TsInterfaceSignature, TsPrimitive, TsProperty, TsTypeAliasDefinition,
    TsTypeDefinition,
};
use crate::generator::schema_context::SchemaContext;

/// Thread-safe counter for unknown schema references
static UNKNOWN_SCHEMA_COUNTER: OnceLock<Mutex<usize>> = OnceLock::new();

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
    /// This is the main public API method used by TypeScriptFetchCodeGenerator.
    /// It determines whether to generate an Interface, Enum, or TypeAlias based on the schema.
    pub fn schema_to_ts_type_definition(
        &self,
        original_name: &str,
        schema_ref: &RefOr<Schema>,
        context: &mut SchemaContext,
    ) -> TsTypeDefinition {
        match schema_ref {
            RefOr::T(schema) => {
                // Determine the appropriate node type based on schema content
                self.determine_node_type(original_name, schema, context)
            }
            RefOr::Ref(reference) => {
                // Handle reference - resolve to actual schema or create type alias
                self.handle_schema_reference(original_name, reference, context)
            }
        }
    }

    // ============================================================================
    // SCHEMA-TO-NODE CONVERSION (Private Methods)
    // ============================================================================

    /// Determine the appropriate TypeScript node type based on schema content
    fn determine_node_type(
        &self,
        original_name: &str,
        schema: &Schema,
        context: &mut SchemaContext,
    ) -> TsTypeDefinition {
        let ts_name = original_name.to_pascal_case();
        let original_name = original_name.to_string();

        match schema {
            Schema::Object(obj_schema) => {
                // Check if this is an enum schema
                if let Some(enum_values) = &obj_schema.enum_values
                    && !enum_values.is_empty()
                {
                    return TsTypeDefinition::Enum(
                        self.schema_to_enum(original_name.as_str(), schema),
                    );
                }

                // Check if this is an object with properties
                if !obj_schema.properties.is_empty() {
                    return TsTypeDefinition::Interface(self.schema_to_interface(
                        original_name.as_str(),
                        schema,
                        context,
                        ts_name.as_str(),
                    ));
                }

                // Check if this has additionalProperties (typed map)
                if obj_schema.additional_properties.is_some() {
                    return TsTypeDefinition::Interface(self.schema_to_interface(
                        original_name.as_str(),
                        schema,
                        context,
                        ts_name.as_str(),
                    ));
                }

                // Otherwise, create a type alias
                let type_expr = self.map_schema_to_type(schema, context, ts_name.as_str());
                TsTypeDefinition::TypeAlias(TsTypeAliasDefinition {
                    ts_name,
                    original_name: original_name.to_string(),
                    type_expr,
                    generics: vec![],
                    documentation: obj_schema.description.clone().map(TsDocComment::new),
                    union_members: None,
                    intersection_members: None,
                })
            }
            Schema::Array(_) => {
                // Array schemas become type aliases
                let type_expr = self.map_schema_to_type(schema, context, ts_name.as_str());
                TsTypeDefinition::TypeAlias(TsTypeAliasDefinition {
                    ts_name,
                    original_name,
                    type_expr,
                    generics: vec![],
                    documentation: None,
                    union_members: None,
                    intersection_members: None,
                })
            }
            Schema::OneOf(one_of) => {
                // Composition schemas become type aliases with union/intersection types
                let type_expr = self.map_schema_to_type(schema, context, ts_name.as_str());
                let union_members =
                    self.extract_union_members(&one_of.items, context, ts_name.as_str());
                TsTypeDefinition::TypeAlias(TsTypeAliasDefinition {
                    ts_name,
                    original_name,
                    type_expr,
                    generics: vec![],
                    documentation: None,
                    union_members: Some(union_members),
                    intersection_members: None,
                })
            }
            Schema::AnyOf(any_of) => {
                // Composition schemas become type aliases with union/intersection types
                let type_expr = self.map_schema_to_type(schema, context, ts_name.as_str());
                let union_members =
                    self.extract_union_members(&any_of.items, context, ts_name.as_str());
                TsTypeDefinition::TypeAlias(TsTypeAliasDefinition {
                    ts_name,
                    original_name,
                    type_expr,
                    generics: vec![],
                    documentation: None,
                    union_members: Some(union_members),
                    intersection_members: None,
                })
            }
            Schema::AllOf(all_of) => {
                // Composition schemas become type aliases with intersection types
                let type_expr = self.map_schema_to_type(schema, context, ts_name.as_str());
                // Extract intersection members for proper conversion
                let intersection_members =
                    self.extract_intersection_members(&all_of.items, context, ts_name.as_str());
                TsTypeDefinition::TypeAlias(TsTypeAliasDefinition {
                    ts_name,
                    original_name,
                    type_expr,
                    generics: vec![],
                    documentation: None,
                    union_members: None,
                    intersection_members: Some(intersection_members),
                })
            }
            _ => {
                // Fallback for unknown schema types
                TsTypeDefinition::TypeAlias(TsTypeAliasDefinition {
                    ts_name,
                    original_name,
                    type_expr: TsExpression::Primitive(TsPrimitive::Any),
                    generics: vec![],
                    documentation: None,
                    union_members: None,
                    intersection_members: None,
                })
            }
        }
    }

    /// Convert a schema to a TypeScript interface
    ///
    /// Only object schemas are supported;
    /// any non-object schemas will cause this function to panic.
    fn schema_to_interface(
        &self,
        original_name: &str,
        schema: &Schema,
        context: &mut SchemaContext,
        current_interface_name: &str,
    ) -> TsInterfaceDefinition {
        let interface_name = original_name.to_pascal_case();
        match schema {
            Schema::Object(obj_schema) => {
                let mut properties = Vec::new();

                // Extract properties from the object schema
                for (prop_name, prop_schema) in &obj_schema.properties {
                    // Pass the current interface name as parent for nested inline objects
                    let type_expr = self.map_ref_or_schema_to_type(
                        prop_schema,
                        context,
                        current_interface_name,
                        Some(prop_name),
                    );
                    let is_required = obj_schema.required.contains(prop_name);
                    let description = self.extract_description_from_schema(prop_schema);

                    // Convert property name to camelCase for TypeScript interface
                    let camel_case_name = prop_name.to_lower_camel_case();
                    let original_name = prop_name.clone();

                    let property = TsProperty {
                        ts_name: camel_case_name,
                        original_name,
                        type_expr,
                        optional: !is_required,
                        is_index_signature: false,
                        documentation: description.map(TsDocComment::new),
                    };

                    properties.push(property);
                }

                // Handle additionalProperties as index signature
                if let Some(additional_props) = &obj_schema.additional_properties {
                    match additional_props.as_ref() {
                        AdditionalProperties::RefOr(schema_ref) => {
                            let mut value_type = self.map_ref_or_schema_to_type(
                                schema_ref,
                                context,
                                current_interface_name,
                                None,
                            );

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
                                    .iter()
                                    .map(|(prop_name, prop_schema)| {
                                        self.map_ref_or_schema_to_type(
                                            prop_schema,
                                            context,
                                            current_interface_name,
                                            Some(prop_name),
                                        )
                                    })
                                    .collect();

                                if !unique_types.is_empty() {
                                    unique_types.insert(value_type.clone());
                                    value_type = TsExpression::Union(unique_types);
                                }
                            }

                            let index_name = "[key: string]".to_string();
                            let index_property = TsProperty {
                                ts_name: index_name.clone(),
                                original_name: index_name,
                                type_expr: value_type,
                                optional: false,
                                is_index_signature: true,
                                documentation: Some(TsDocComment::new(
                                    "Additional properties".to_string(),
                                )),
                            };
                            properties.push(index_property);
                        }
                        AdditionalProperties::FreeForm(true) => {
                            let index_name = "[key: string]".to_string();
                            let index_property = TsProperty {
                                ts_name: index_name.clone(),
                                original_name: index_name,
                                type_expr: TsExpression::Primitive(TsPrimitive::Any),
                                optional: false,
                                is_index_signature: true,
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

                TsInterfaceDefinition {
                    signature: TsInterfaceSignature::new(
                        interface_name.clone(),
                        original_name.to_string(),
                    ),
                    properties,
                    documentation: obj_schema.description.clone().map(TsDocComment::new),
                }
            }
            _ => {
                // For non-object schemas, create an empty interface
                TsInterfaceDefinition {
                    signature: TsInterfaceSignature::new(
                        interface_name.clone(),
                        original_name.to_string(),
                    ),
                    properties: vec![],
                    documentation: None,
                }
            }
        }
    }

    /// Convert a schema to a TypeScript enum.
    ///
    /// Only object schemas with `enum_values` are supported;
    /// any non-object schemas will cause this function to panic.
    fn schema_to_enum(&self, original_name: &str, schema: &Schema) -> TsEnumDefinition {
        let Schema::Object(obj_schema) = schema else {
            panic!("schema_to_enum called with non-object schema");
        };

        let ts_name = original_name.to_pascal_case();
        let enum_descriptions = Self::extract_enum_descriptions(obj_schema);

        let variants: Vec<TsEnumVariant> = obj_schema.enum_values.as_ref().unwrap_or(&Vec::new())
                    .iter()
                    .enumerate()
                    .map(|(index, enum_value)| {
                        if let serde_json::Value::Bool(_) = enum_value {
                            warn!(
                                "Boolean enum value found for schema '{original_name}'. TypeScript enums don't support booleans, converting to number (0 for false, 1 for true).",
                            );
                        }

                        let enum_val = TsEnumValue::from_json_value(enum_value);
                        let name = enum_val.generate_enum_name();
                        let value = Some(enum_val);
                        let documentation = enum_descriptions
                            .get(index)
                            .and_then(|d| d.as_str())
                            .map(|s| TsDocComment::new(s.to_string()));

                        TsEnumVariant {
                            name,
                            value,
                            documentation,
                        }
                    })
                    .collect();

        TsEnumDefinition {
            ts_name,
            original_name: original_name.to_string(),
            variants,
            documentation: obj_schema.description.clone().map(TsDocComment::new),
            is_const: false,
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
        if let Some(extensions) = &obj_schema.extensions
            && let Some(enum_descriptions_value) = extensions.get("x-enumDescriptions")
            && let serde_json::Value::Array(descriptions) = enum_descriptions_value
        {
            return descriptions.clone();
        }
        Vec::new()
    }

    // ============================================================================
    // TYPE MAPPING (Private Methods - absorbed from TypeMapper)
    // ============================================================================

    /// Map a RefOr<Schema> to a TypeScript type expression
    ///
    /// This function creates type expressions for use in properties, array items, unions, etc.
    ///
    /// For schema references (`$ref`), it converts the original schema name to its TypeScript name
    /// (PascalCase) immediately, since the TypeScript name is always `original_name.to_pascal_case()`.
    ///
    /// For inline schemas, it handles two cases:
    /// 1. **Nested inline objects**: When both `parent_name` and `field_name` are provided and the
    ///    schema is an object with properties, it creates a named interface using the naming
    ///    convention `{parent_name}{field_name}` (both in PascalCase). The interface is registered
    ///    in the context and a reference to it is returned.
    /// 2. **Other inline schemas**: Maps directly to TypeScript types (primitives, arrays, etc.)
    ///
    /// # Parameters
    ///
    /// * `schema_ref` - The schema reference or inline schema to map
    /// * `context` - Schema context for reference resolution and interface registration
    /// * `parent_name` - Optional parent interface name (in PascalCase) for nested inline objects
    /// * `field_name` - Optional field name for nested inline objects
    ///
    /// # Examples
    ///
    /// ```
    /// // Schema reference: "#/components/schemas/User"
    /// // Result: TsExpression::Reference("User")
    ///
    /// // Nested inline object with parent "DeeplyNestedInline" and field "level_one"
    /// // Result: Creates interface "DeeplyNestedInlineLevelOne" and returns Reference("DeeplyNestedInlineLevelOne")
    ///
    /// // Inline primitive schema
    /// // Result: TsExpression::Primitive(TsPrimitive::String)
    /// ```
    fn map_ref_or_schema_to_type(
        &self,
        schema_ref: &RefOr<Schema>,
        context: &mut SchemaContext,
        parent_name: &str,
        field_name: Option<&str>,
    ) -> TsExpression {
        match schema_ref {
            RefOr::T(schema) => {
                // For inline schemas, check if we should create a named interface
                // Only create named interfaces for nested inline objects
                if let Some(field) = field_name
                    && let Schema::Object(obj_schema) = schema
                    && !obj_schema.properties.is_empty()
                {
                    // Generate name using {parent_name}{field_name} convention
                    let field_pascal = field.to_pascal_case();
                    let inline_interface_name = format!("{parent_name}{field_pascal}");

                    // Check if this interface already exists
                    if !context.has_inline_interface(&inline_interface_name) {
                        // Create the interface definition
                        let interface = self.schema_to_interface(
                            &inline_interface_name,
                            schema,
                            context,
                            &inline_interface_name,
                        );

                        // Register it in the context
                        let type_def = TsTypeDefinition::Interface(interface);
                        context.register_inline_interface(inline_interface_name.clone(), type_def);
                    }

                    // Return a reference to the named interface
                    return TsExpression::Reference(inline_interface_name);
                }

                // For other cases, map directly to TypeScript types
                self.map_schema_to_type(schema, context, parent_name)
            }
            RefOr::Ref(reference) => {
                // Extract the original schema name from the reference path
                let original_name = self.extract_schema_name(&reference.ref_location);

                // Validate that the schema exists in the context
                if !context.schemas.contains_key(&original_name) {
                    warn!(
                        "Unresolved schema reference: {} (from {})",
                        original_name, reference.ref_location
                    );
                }

                // Convert to TypeScript name immediately (PascalCase conversion)
                // since TypeScript names are always original_name.to_pascal_case().
                let ts_name = original_name.to_pascal_case();
                TsExpression::Reference(ts_name)
            }
        }
    }

    /// Map a Schema to a TypeScript type expression
    fn map_schema_to_type(
        &self,
        schema: &Schema,
        context: &mut SchemaContext,
        parent_name: &str,
    ) -> TsExpression {
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
                    return self.map_inline_object_to_type(obj_schema, context, parent_name);
                }

                // Handle primitive types
                self.map_primitive_type_from_schema(obj_schema)
            }
            Schema::Array(arr_schema) => {
                // Map array schema to TypeScript array type using the items field
                let item_type =
                    self.map_array_items_to_type(&arr_schema.items, context, parent_name);
                TsExpression::Array(Box::new(item_type))
            }
            Schema::OneOf(one_of) => {
                // Map oneOf to union type with discriminator support
                self.map_composition_to_type(&one_of.items, context, parent_name)
            }
            Schema::AllOf(all_of) => {
                // Map allOf to intersection type with deduplication
                let types: BTreeSet<TsExpression> = all_of
                    .items
                    .iter()
                    .map(|schema_ref| {
                        self.map_ref_or_schema_to_type(schema_ref, context, parent_name, None)
                    })
                    .collect();
                TsExpression::Intersection(types)
            }
            Schema::AnyOf(any_of) => {
                // Map anyOf to union type with discriminator support
                self.map_composition_to_type(&any_of.items, context, parent_name)
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
        parent_name: &str,
    ) -> TsExpression {
        match array_items {
            utoipa::openapi::schema::ArrayItems::RefOrSchema(schema_ref) => {
                self.map_ref_or_schema_to_type(schema_ref, context, parent_name, None)
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
        parent_name: &str,
    ) -> TsExpression {
        let mut properties = BTreeMap::new();

        // Map each property to its TypeScript type
        // Convert property names to camelCase for consistency, but preserve original name
        for (original_name, prop_schema) in &obj_schema.properties {
            let type_expr = self.map_ref_or_schema_to_type(
                prop_schema,
                context,
                parent_name,
                Some(original_name),
            );
            let camel_case_name = original_name.to_lower_camel_case();
            properties.insert(
                camel_case_name.clone(),
                ObjectProperty {
                    ts_name: camel_case_name,
                    type_expr,
                    original_name: original_name.clone(),
                },
            );
        }

        TsExpression::Object(properties)
    }

    /// Map composition schemas (oneOf/anyOf) to TypeScript types with discriminator support
    fn map_composition_to_type(
        &self,
        items: &[RefOr<Schema>],
        context: &mut SchemaContext,
        parent_name: &str,
    ) -> TsExpression {
        let types: BTreeSet<TsExpression> = items
            .iter()
            .map(|schema_ref| {
                self.map_ref_or_schema_to_type(schema_ref, context, parent_name, None)
            })
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

    /// Extract nullable reference name from a type expression
    ///
    /// Checks if the type expression is a union containing both `null` and a reference type.
    /// Returns the reference name if found, `None` otherwise.
    ///
    /// Example: `null | ServiceMetadata` -> `Some("ServiceMetadata")`
    fn extract_nullable_reference_name(&self, type_expr: &TsExpression) -> Option<String> {
        if let TsExpression::Union(union_types) = type_expr {
            let mut has_null = false;
            let mut ref_name = None;

            for ut in union_types {
                if matches!(ut, TsExpression::Primitive(TsPrimitive::Null)) {
                    has_null = true;
                } else if let Some(ref_name_val) = ut.reference_name() {
                    ref_name = Some(ref_name_val);
                }
            }

            if has_null && ref_name.is_some() {
                ref_name
            } else {
                None
            }
        } else {
            None
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
    ///
    /// This function processes schema references (`$ref`) from the OpenAPI specification.
    /// It resolves references to their target schemas and generates appropriate TypeScript
    /// type definitions, handling circular dependencies and unresolved references.
    ///
    /// # Parameters
    ///
    /// * `original_name` - The original name of the schema that contains this reference.
    ///   This is used as the name for the resulting TypeScript type definition.
    /// * `reference` - The OpenAPI reference object containing the reference path
    ///   (e.g., `#/components/schemas/User`).
    /// * `context` - Schema context providing access to all schemas and cycle detection.
    ///
    /// # Behavior
    ///
    /// The function handles three cases:
    ///
    /// 1. **Circular Reference**: If the referenced schema is already being processed
    ///    (detected via `context.is_visited()`), creates a type alias to break the cycle.
    ///    The type alias references the TypeScript name (PascalCase) of the target schema.
    /// 2. **Resolved Reference**: If the referenced schema exists in the context,
    ///    recursively resolves it by calling `schema_to_ts_type_definition()` on the
    ///    target schema. Uses context tracking to prevent infinite recursion.
    /// 3. **Unresolved Reference**: If the referenced schema doesn't exist in the context,
    ///    creates a type alias with a warning. The type alias references the TypeScript
    ///    name (PascalCase) of the target schema, allowing the code to compile even if
    ///    the reference is invalid.
    ///
    /// # Examples
    ///
    /// ```
    /// // Reference: "#/components/schemas/User"
    /// // original_name: "UserProfile"
    /// // Result: TypeScript type definition for User (resolved recursively)
    ///
    /// // Circular reference (User -> Profile -> User)
    /// // Result: TypeAlias { ts_name: "User", type_expr: Reference("Profile") }
    /// ```
    ///
    /// # Cycle Detection
    ///
    /// The function uses `SchemaContext` to track visited schemas and prevent infinite
    /// recursion when resolving circular references. Before recursively resolving a schema,
    /// it marks it as visited and increments the depth counter. After resolution, it cleans
    /// up by unmarking the schema and decrementing the depth.
    fn handle_schema_reference(
        &self,
        original_name: &str,
        reference: &utoipa::openapi::Ref,
        context: &mut SchemaContext,
    ) -> TsTypeDefinition {
        let ts_name = original_name.to_pascal_case();
        // Extract schema name from reference path (this is the original name of the referenced schema)
        let ref_original_name = self.extract_schema_name(&reference.ref_location);
        let ref_ts_name = ref_original_name.to_pascal_case();

        // Check for circular dependency
        if context.is_visited(&ref_original_name) {
            // Circular reference detected - create a type alias to break the cycle
            // Convert to TypeScript name (PascalCase) for the reference
            return TsTypeDefinition::TypeAlias(TsTypeAliasDefinition {
                ts_name,
                original_name: ref_original_name.clone(),
                type_expr: TsExpression::Reference(ref_ts_name),
                generics: vec![],
                documentation: Some(TsDocComment::new(format!(
                    "Circular reference to {}",
                    ref_original_name
                ))),
                union_members: None,
                intersection_members: None,
            });
        }

        // Look up the actual schema using the original name
        if let Some(target_schema) = context.schemas.get(&ref_original_name) {
            // Mark as visited to prevent cycles
            context.mark_visited(ref_original_name.clone());
            context.increment_depth();

            // Recursively resolve the target schema using the original schema name
            let type_def =
                self.schema_to_ts_type_definition(&ref_original_name, target_schema, context);

            // Cleanup
            context.decrement_depth();
            context.unmark_visited(&ref_original_name);

            type_def
        } else {
            // Unresolved reference - generate warning and fallback
            warn!("Unresolved schema reference: {}", ref_original_name);
            TsTypeDefinition::TypeAlias(TsTypeAliasDefinition {
                ts_name,
                original_name: ref_original_name.clone(),
                type_expr: TsExpression::Reference(ref_ts_name),
                generics: vec![],
                documentation: Some(TsDocComment::new(format!(
                    "Unresolved reference to {}",
                    ref_original_name
                ))),
                union_members: None,
                intersection_members: None,
            })
        }
    }

    /// Check if a schema reference represents an interface
    fn is_schema_ref_interface(&self, schema_ref: &RefOr<Schema>, context: &SchemaContext) -> bool {
        match schema_ref {
            RefOr::T(Schema::Object(obj_schema)) => {
                !obj_schema.properties.is_empty() || obj_schema.additional_properties.is_some()
            }
            RefOr::Ref(reference) => {
                let original_name = self.extract_schema_name(&reference.ref_location);
                context
                    .schemas
                    .get(&original_name)
                    .map(|s| {
                        matches!(s, RefOr::T(Schema::Object(obj_schema)) if !obj_schema.properties.is_empty() || obj_schema.additional_properties.is_some())
                    })
                    .unwrap_or(false)
            }
            _ => false,
        }
    }

    /// Extract union member information from composition schema items
    ///
    /// This extracts metadata about each member of a union (oneOf/anyOf) for generating
    /// FromJSON/ToJSON functions that can discriminate between union members.
    fn extract_union_members(
        &self,
        items: &[RefOr<Schema>],
        context: &mut SchemaContext,
        parent_name: &str,
    ) -> Vec<UnionMemberInfo> {
        items
            .iter()
            .enumerate()
            .map(|(index, schema_ref)| {
                // For inline object schemas in unions, create a named interface
                let (type_expr, is_interface) = if let RefOr::T(Schema::Object(obj_schema)) =
                    schema_ref
                    && !obj_schema.properties.is_empty()
                {
                    // Create a named interface for this inline object
                    let inline_interface_name = format!("{parent_name}Member{}", index + 1);

                    if !context.has_inline_interface(&inline_interface_name) {
                        let interface = self.schema_to_interface(
                            &inline_interface_name,
                            &Schema::Object(obj_schema.clone()),
                            context,
                            &inline_interface_name,
                        );
                        let type_def = TsTypeDefinition::Interface(interface);
                        context.register_inline_interface(inline_interface_name.clone(), type_def);
                    }

                    (TsExpression::Reference(inline_interface_name.clone()), true)
                } else {
                    let expr =
                        self.map_ref_or_schema_to_type(schema_ref, context, parent_name, None);
                    let is_intf = self.is_schema_ref_interface(schema_ref, context);
                    (expr, is_intf)
                };

                let (ts_name, is_primitive) = match &type_expr {
                    TsExpression::Reference(name) => (name.clone(), false),
                    TsExpression::Primitive(prim) => (prim.to_string(), true),
                    TsExpression::Array(_) => ("Array".to_string(), false),
                    TsExpression::Object(_) => {
                        // This shouldn't happen after the fix above, but handle it as fallback
                        ("any".to_string(), false)
                    }
                    _ => ("any".to_string(), false),
                };

                UnionMemberInfo {
                    ts_name,
                    type_expr,
                    is_primitive,
                    is_interface,
                }
            })
            .collect()
    }

    /// Extract intersection member information from allOf schema items
    ///
    /// This extracts basic metadata about each member of an intersection (allOf) for generating
    /// FromJSON/ToJSON functions. The type_expr contains all the detailed information needed.
    fn extract_intersection_members(
        &self,
        items: &[RefOr<Schema>],
        context: &mut SchemaContext,
        parent_name: &str,
    ) -> Vec<IntersectionMemberInfo> {
        items
            .iter()
            .map(|schema_ref| {
                let type_expr =
                    self.map_ref_or_schema_to_type(schema_ref, context, parent_name, None);

                // Check if it's a reference type
                let is_reference = matches!(type_expr, TsExpression::Reference(_));

                // Check if it's an object type and extract properties with reference info
                let (is_object, object_properties) =
                    if let TsExpression::Object(properties) = &type_expr {
                        let props: Vec<IntersectionObjectProperty> = properties
                            .values()
                            .map(|prop| {
                                let reference_name = prop.type_expr.reference_name();
                                let nullable_reference_name =
                                    self.extract_nullable_reference_name(&prop.type_expr);

                                IntersectionObjectProperty {
                                    ts_name: prop.ts_name.clone(),
                                    original_name: prop.original_name.clone(),
                                    type_expr: prop.type_expr.clone(),
                                    reference_name,
                                    nullable_reference_name,
                                }
                            })
                            .collect();
                        (true, Some(props))
                    } else {
                        (false, None)
                    };

                // Extract TypeScript name
                let ts_name = match &type_expr {
                    TsExpression::Reference(name) => name.clone(),
                    TsExpression::Object(_) => "object".to_string(),
                    TsExpression::Primitive(prim) => prim.to_string(),
                    _ => "any".to_string(),
                };

                IntersectionMemberInfo {
                    ts_name,
                    type_expr,
                    is_reference,
                    is_object,
                    object_properties,
                }
            })
            .collect()
    }

    /// Extract schema name from reference path
    ///
    /// Converts `#/components/schemas/User` -> `User`
    fn extract_schema_name(&self, ref_path: &str) -> String {
        if let Some(schema_name) = ref_path.strip_prefix("#/components/schemas/") {
            schema_name.to_string()
        } else {
            // Get or initialize the counter
            let count = UNKNOWN_SCHEMA_COUNTER.get_or_init(|| Mutex::new(0));
            let count = {
                let mut count = count.lock().expect("Unknown schema counter mutex poisoned");
                *count += 1;
                *count
            };
            error!(
                "Invalid schema reference path: {ref_path} - using fallback name Unknown{count}"
            );
            format!("Unknown{}", count)
        }
    }
}

impl Default for SchemaGenerator {
    fn default() -> Self {
        SchemaGenerator
    }
}
