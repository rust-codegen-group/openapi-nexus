//! Schema generation logic for TypeScript with OpenAPI 3.1.2 support
//!
//! This module consolidates schema-to-TypeScript conversion and type mapping functionality
//! into a single, well-architected generator that fully implements OpenAPI v3.1.2 features.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, OnceLock};

use heck::{ToLowerCamelCase as _, ToPascalCase as _};
use tracing::{error, warn};

use crate::ast::ty::ts_type_alias_definition::{
    IntersectionMemberInfo, IntersectionObjectProperty, UnionMemberInfo,
};
use crate::ast::{
    ObjectProperty, TsDocComment, TsEnumDefinition, TsEnumValue, TsEnumVariant, TsExpression,
    TsInterfaceDefinition, TsInterfaceSignature, TsPrimitive, TsProperty, TsTypeAliasDefinition,
    TsTypeDefinition,
};
use crate::generator::schema_context::SchemaContext;
use openapi_nexus_core::TaggedEnumPattern;
use openapi_nexus_spec::oas31::spec::{
    BooleanSchema, ObjectOrReference, ObjectSchema, Schema, SchemaType, SchemaTypeSet,
};

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
        schema_ref: &ObjectOrReference<ObjectSchema>,
        context: &mut SchemaContext,
    ) -> TsTypeDefinition {
        match schema_ref {
            ObjectOrReference::Object(object_schema) => {
                self.determine_node_type(original_name, object_schema, context)
            }
            ObjectOrReference::Ref { ref_path, .. } => {
                self.handle_schema_reference(original_name, ref_path, context)
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
        obj_schema: &ObjectSchema,
        context: &mut SchemaContext,
    ) -> TsTypeDefinition {
        let ts_name = original_name.to_pascal_case();
        let original_name = original_name.to_string();

        // Check if this is an enum schema
        if !obj_schema.enum_values.is_empty() {
            return TsTypeDefinition::Enum(self.schema_to_enum(original_name.as_str(), obj_schema));
        }

        // Check if this is an object with properties or additionalProperties
        if !obj_schema.properties.is_empty() || obj_schema.additional_properties.is_some() {
            return TsTypeDefinition::Interface(self.schema_to_interface(
                original_name.as_str(),
                obj_schema,
                context,
                ts_name.as_str(),
            ));
        }

        // oneOf -> type alias with union
        if !obj_schema.one_of.is_empty() {
            let type_expr = self.map_object_schema_to_type(obj_schema, context, ts_name.as_str());
            let union_members =
                self.extract_union_members(&obj_schema.one_of, context, ts_name.as_str());
            return TsTypeDefinition::TypeAlias(TsTypeAliasDefinition {
                ts_name,
                original_name: original_name.clone(),
                type_expr,
                generics: vec![],
                documentation: obj_schema.description.clone().map(TsDocComment::new),
                union_members: Some(union_members),
                intersection_members: None,
            });
        }
        // anyOf -> type alias with union
        if !obj_schema.any_of.is_empty() {
            let type_expr = self.map_object_schema_to_type(obj_schema, context, ts_name.as_str());
            let union_members =
                self.extract_union_members(&obj_schema.any_of, context, ts_name.as_str());
            return TsTypeDefinition::TypeAlias(TsTypeAliasDefinition {
                ts_name,
                original_name: original_name.clone(),
                type_expr,
                generics: vec![],
                documentation: obj_schema.description.clone().map(TsDocComment::new),
                union_members: Some(union_members),
                intersection_members: None,
            });
        }
        // allOf -> type alias with intersection
        if !obj_schema.all_of.is_empty() {
            let type_expr = self.map_object_schema_to_type(obj_schema, context, ts_name.as_str());
            let intersection_members =
                self.extract_intersection_members(&obj_schema.all_of, context, ts_name.as_str());
            return TsTypeDefinition::TypeAlias(TsTypeAliasDefinition {
                ts_name,
                original_name: original_name.clone(),
                type_expr,
                generics: vec![],
                documentation: obj_schema.description.clone().map(TsDocComment::new),
                union_members: None,
                intersection_members: Some(intersection_members),
            });
        }

        // Otherwise, create a type alias (array or primitive)
        let type_expr = self.map_object_schema_to_type(obj_schema, context, ts_name.as_str());
        TsTypeDefinition::TypeAlias(TsTypeAliasDefinition {
            ts_name,
            original_name,
            type_expr,
            generics: vec![],
            documentation: obj_schema.description.clone().map(TsDocComment::new),
            union_members: None,
            intersection_members: None,
        })
    }

    /// Convert a schema to a TypeScript interface
    fn schema_to_interface(
        &self,
        original_name: &str,
        obj_schema: &ObjectSchema,
        context: &mut SchemaContext,
        current_interface_name: &str,
    ) -> TsInterfaceDefinition {
        let interface_name = original_name.to_pascal_case();
        let mut properties = Vec::new();

        for (prop_name, prop_schema) in &obj_schema.properties {
            let type_expr = self.map_object_or_ref_schema_to_type(
                prop_schema,
                context,
                current_interface_name,
                Some(prop_name),
            );
            let is_required = obj_schema.required.contains(prop_name);
            let description = self.extract_description_from_schema_ref(prop_schema);

            let camel_case_name = prop_name.to_lower_camel_case();
            let prop_original_name = prop_name.clone();

            let property = TsProperty {
                ts_name: camel_case_name,
                original_name: prop_original_name,
                type_expr,
                optional: !is_required,
                is_index_signature: false,
                documentation: description.map(TsDocComment::new),
            };

            properties.push(property);
        }

        // Handle additionalProperties
        if let Some(additional_props) = &obj_schema.additional_properties {
            match additional_props {
                Schema::Object(schema_ref) => {
                    let mut value_type = self.map_object_or_ref_schema_to_type(
                        schema_ref.as_ref(),
                        context,
                        current_interface_name,
                        None,
                    );

                    if !obj_schema.properties.is_empty() {
                        let mut unique_types: BTreeSet<TsExpression> = obj_schema
                            .properties
                            .iter()
                            .map(|(prop_name, prop_schema)| {
                                self.map_object_or_ref_schema_to_type(
                                    prop_schema,
                                    context,
                                    current_interface_name,
                                    Some(prop_name),
                                )
                            })
                            .collect();
                        unique_types.insert(value_type.clone());
                        value_type = TsExpression::Union(unique_types);
                    }

                    let index_name = "[key: string]".to_string();
                    properties.push(TsProperty {
                        ts_name: index_name.clone(),
                        original_name: index_name,
                        type_expr: value_type,
                        optional: false,
                        is_index_signature: true,
                        documentation: Some(TsDocComment::new("Additional properties".to_string())),
                    });
                }
                Schema::Boolean(BooleanSchema(true)) => {
                    let index_name = "[key: string]".to_string();
                    properties.push(TsProperty {
                        ts_name: index_name.clone(),
                        original_name: index_name,
                        type_expr: TsExpression::Primitive(TsPrimitive::Any),
                        optional: false,
                        is_index_signature: true,
                        documentation: Some(TsDocComment::new("Additional properties".to_string())),
                    });
                }
                Schema::Boolean(BooleanSchema(false)) => {}
            }
        }

        TsInterfaceDefinition {
            signature: TsInterfaceSignature::new(interface_name.clone(), original_name.to_string()),
            properties,
            documentation: obj_schema.description.clone().map(TsDocComment::new),
        }
    }

    /// Convert a schema to a TypeScript enum
    fn schema_to_enum(&self, original_name: &str, obj_schema: &ObjectSchema) -> TsEnumDefinition {
        let ts_name = original_name.to_pascal_case();
        let enum_descriptions = Self::extract_enum_descriptions(obj_schema);

        let variants: Vec<TsEnumVariant> = obj_schema
            .enum_values
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
    fn extract_enum_descriptions(obj_schema: &ObjectSchema) -> Vec<serde_json::Value> {
        if let Some(enum_descriptions_value) = obj_schema.extensions.get("x-enumDescriptions")
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
    /// - Schema reference `#/components/schemas/User` yields `TsExpression::Reference("User")`
    /// - Nested inline object with parent and field yields a generated interface reference
    fn map_object_schema_to_type(
        &self,
        obj_schema: &ObjectSchema,
        context: &mut SchemaContext,
        parent_name: &str,
    ) -> TsExpression {
        if !obj_schema.enum_values.is_empty() {
            return self.map_enum_to_type(&obj_schema.enum_values);
        }
        if !obj_schema.properties.is_empty() {
            return self.map_inline_object_to_type(obj_schema, context, parent_name);
        }
        if obj_schema
            .schema_type
            .as_ref()
            .is_some_and(|t| t.is_array_or_nullable_array())
            && obj_schema.items.is_some()
        {
            let item_type = self.map_schema_to_ts_type(
                obj_schema.items.as_ref().unwrap().as_ref(),
                context,
                parent_name,
            );
            return TsExpression::Array(Box::new(item_type));
        }
        if !obj_schema.one_of.is_empty() {
            return self.map_composition_to_type(&obj_schema.one_of, context, parent_name);
        }
        if !obj_schema.any_of.is_empty() {
            return self.map_composition_to_type(&obj_schema.any_of, context, parent_name);
        }
        if !obj_schema.all_of.is_empty() {
            let types: BTreeSet<TsExpression> = obj_schema
                .all_of
                .iter()
                .map(|schema_ref| {
                    self.map_object_or_ref_schema_to_type(schema_ref, context, parent_name, None)
                })
                .collect();
            return TsExpression::Intersection(types);
        }
        self.map_primitive_type_from_schema(obj_schema)
    }

    /// Map ObjectOrReference<ObjectSchema> to TypeScript type expression
    fn map_object_or_ref_schema_to_type(
        &self,
        schema_ref: &ObjectOrReference<ObjectSchema>,
        context: &mut SchemaContext,
        parent_name: &str,
        field_name: Option<&str>,
    ) -> TsExpression {
        match schema_ref {
            ObjectOrReference::Object(object_schema) => {
                if let Some(field) = field_name
                    && !object_schema.properties.is_empty()
                {
                    let nested_name = format!("{}{}", parent_name, field.to_pascal_case());
                    if !context.has_inline_interface(&nested_name) {
                        let iface = self.schema_to_interface(
                            &nested_name,
                            object_schema,
                            context,
                            &nested_name,
                        );
                        context.register_inline_interface(
                            nested_name.clone(),
                            TsTypeDefinition::Interface(iface),
                        );
                    }
                    return TsExpression::Reference(nested_name.to_pascal_case());
                }
                self.map_object_schema_to_type(
                    object_schema,
                    context,
                    field_name
                        .map(|f| format!("{}{}", parent_name, f.to_pascal_case()))
                        .as_deref()
                        .unwrap_or(parent_name),
                )
            }
            ObjectOrReference::Ref { ref_path, .. } => {
                let schema_name = self.extract_schema_name(ref_path);
                TsExpression::Reference(schema_name.to_pascal_case())
            }
        }
    }

    /// Map schema (Boolean | Object) to TypeScript type - used for array items and additionalProperties
    fn map_schema_to_ts_type(
        &self,
        schema: &Schema,
        context: &mut SchemaContext,
        parent_name: &str,
    ) -> TsExpression {
        match schema {
            Schema::Object(schema_ref) => self.map_object_or_ref_schema_to_type(
                schema_ref.as_ref(),
                context,
                parent_name,
                None,
            ),
            Schema::Boolean(_) => TsExpression::Primitive(TsPrimitive::Any),
        }
    }

    /// Map primitive type from schema object
    fn map_primitive_type_from_schema(&self, obj_schema: &ObjectSchema) -> TsExpression {
        let base_type = Self::map_schema_type_to_primitive(&obj_schema.schema_type);
        let formatted_type = self.handle_known_format_str(base_type, &obj_schema.format);
        self.handle_nullable(formatted_type, &obj_schema.schema_type)
    }

    /// Map schema type set to primitive TypeScript type
    fn map_schema_type_to_primitive(schema_type: &Option<SchemaTypeSet>) -> TsExpression {
        let type_set = match schema_type {
            Some(ts) => ts,
            None => return TsExpression::Primitive(TsPrimitive::Any),
        };
        match type_set {
            SchemaTypeSet::Single(t) => Self::map_single_type_to_primitive(*t),
            SchemaTypeSet::Multiple(types) => {
                let non_null: Vec<_> = types.iter().filter(|t| **t != SchemaType::Null).collect();
                if non_null.is_empty() {
                    TsExpression::Primitive(TsPrimitive::Null)
                } else if non_null.len() == 1 {
                    Self::map_single_type_to_primitive(*non_null[0])
                } else {
                    let union_types: BTreeSet<TsExpression> = non_null
                        .iter()
                        .map(|t| Self::map_single_type_to_primitive(**t))
                        .collect();
                    TsExpression::Union(union_types)
                }
            }
        }
    }

    fn map_single_type_to_primitive(t: SchemaType) -> TsExpression {
        match t {
            SchemaType::String => TsExpression::Primitive(TsPrimitive::String),
            SchemaType::Integer => TsExpression::Primitive(TsPrimitive::Number),
            SchemaType::Number => TsExpression::Primitive(TsPrimitive::Number),
            SchemaType::Boolean => TsExpression::Primitive(TsPrimitive::Boolean),
            SchemaType::Array => {
                TsExpression::Array(Box::new(TsExpression::Primitive(TsPrimitive::String)))
            }
            SchemaType::Object => TsExpression::Primitive(TsPrimitive::Any),
            SchemaType::Null => TsExpression::Primitive(TsPrimitive::Null),
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
        obj_schema: &ObjectSchema,
        context: &mut SchemaContext,
        parent_name: &str,
    ) -> TsExpression {
        let mut properties = BTreeMap::new();

        for (original_name, prop_schema) in &obj_schema.properties {
            let type_expr = self.map_object_or_ref_schema_to_type(
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

    /// Map composition schemas (oneOf/anyOf) to TypeScript types
    fn map_composition_to_type(
        &self,
        items: &[ObjectOrReference<ObjectSchema>],
        context: &mut SchemaContext,
        parent_name: &str,
    ) -> TsExpression {
        let types: BTreeSet<TsExpression> = items
            .iter()
            .map(|schema_ref| {
                self.map_object_or_ref_schema_to_type(schema_ref, context, parent_name, None)
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
    fn handle_nullable(
        &self,
        base_type: TsExpression,
        schema_type: &Option<SchemaTypeSet>,
    ) -> TsExpression {
        let is_nullable = schema_type
            .as_ref()
            .is_some_and(|t| t.contains(SchemaType::Null));

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

    /// Extract description from ObjectOrReference<ObjectSchema>
    fn extract_description_from_schema_ref(
        &self,
        schema_ref: &ObjectOrReference<ObjectSchema>,
    ) -> Option<String> {
        match schema_ref {
            ObjectOrReference::Object(obj_schema) => obj_schema.description.clone(),
            ObjectOrReference::Ref { description, .. } => description.clone(),
        }
    }

    /// Handle known format annotations
    fn handle_known_format_str(
        &self,
        base_type: TsExpression,
        format: &Option<String>,
    ) -> TsExpression {
        match format.as_deref() {
            Some("date-time") | Some("date") => base_type,
            Some("email") | Some("uri") | Some("uuid") => base_type,
            Some("int64") | Some("int32") => base_type,
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
        ref_path: &str,
        context: &mut SchemaContext,
    ) -> TsTypeDefinition {
        let ts_name = original_name.to_pascal_case();
        let ref_original_name = self.extract_schema_name(ref_path);
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
    fn is_schema_ref_interface(
        &self,
        schema_ref: &ObjectOrReference<ObjectSchema>,
        context: &SchemaContext,
    ) -> bool {
        match schema_ref {
            ObjectOrReference::Object(obj_schema) => {
                !obj_schema.properties.is_empty() || obj_schema.additional_properties.is_some()
            }
            ObjectOrReference::Ref { ref_path, .. } => {
                let original_name = self.extract_schema_name(ref_path);
                context.schemas.get(&original_name).map(|s| {
                    matches!(s, ObjectOrReference::Object(obj_schema) if !obj_schema.properties.is_empty() || obj_schema.additional_properties.is_some())
                }).unwrap_or(false)
            }
        }
    }

    /// Extract union member information from composition schema items
    ///
    /// This extracts metadata about each member of a union (oneOf/anyOf) for generating
    /// FromJSON/ToJSON functions that can discriminate between union members.
    fn extract_union_members(
        &self,
        items: &[ObjectOrReference<ObjectSchema>],
        context: &mut SchemaContext,
        parent_name: &str,
    ) -> Vec<UnionMemberInfo> {
        items
            .iter()
            .enumerate()
            .map(|(index, schema_ref)| {
                let tagged_enum_pattern = TaggedEnumPattern::detect_from_schema(schema_ref);

                let inline_interface_name = tagged_enum_pattern
                    .as_ref()
                    .map(|pattern| pattern.to_interface_name(parent_name))
                    .unwrap_or_else(|| format!("{parent_name}Member{}", index + 1));

                let (type_expr, is_interface) = if let ObjectOrReference::Object(obj_schema) =
                    schema_ref
                    && !obj_schema.properties.is_empty()
                {
                    if !context.has_inline_interface(&inline_interface_name) {
                        let enum_discriminator = tagged_enum_pattern
                            .as_ref()
                            .and_then(|pattern| pattern.tag_field())
                            .and_then(|tag_field| {
                                obj_schema.properties.get(tag_field).and_then(|tag_prop| {
                                    if let ObjectOrReference::Object(tag_obj) = tag_prop {
                                        tag_obj.enum_values.first().and_then(|v| v.as_str()).map(
                                            |enum_val| {
                                                (tag_field.to_string(), enum_val.to_string())
                                            },
                                        )
                                    } else {
                                        None
                                    }
                                })
                            });

                        let interface = self.schema_to_interface(
                            &inline_interface_name,
                            obj_schema,
                            context,
                            &inline_interface_name,
                        );

                        if let Some((prop_name, enum_val)) = enum_discriminator {
                            context.register_enum_discriminator(
                                inline_interface_name.clone(),
                                prop_name,
                                enum_val,
                            );
                        }

                        let type_def = TsTypeDefinition::Interface(interface);
                        context.register_inline_interface(inline_interface_name.clone(), type_def);
                    }

                    (TsExpression::Reference(inline_interface_name.clone()), true)
                } else if let ObjectOrReference::Object(obj_schema) = schema_ref
                    && !obj_schema.all_of.is_empty()
                {
                    if !context.has_inline_interface(&inline_interface_name) {
                        let tag_field_name = tagged_enum_pattern
                            .as_ref()
                            .and_then(|pattern| pattern.tag_field().map(|s| s.to_string()));

                        let mut all_properties = Vec::new();
                        let mut seen_properties = BTreeSet::new();
                        let mut enum_discriminator: Option<(String, String)> = None;

                        for item in &obj_schema.all_of {
                            if let ObjectOrReference::Object(item_schema) = item {
                                for (prop_name, prop_schema) in &item_schema.properties {
                                    if seen_properties.insert(prop_name.clone()) {
                                        let type_expr = self.map_object_or_ref_schema_to_type(
                                            prop_schema,
                                            context,
                                            &inline_interface_name,
                                            Some(prop_name),
                                        );
                                        let is_required = item_schema.required.contains(prop_name);
                                        let description =
                                            self.extract_description_from_schema_ref(prop_schema);

                                        if let Some(ref tag_field) = tag_field_name
                                            && prop_name == tag_field
                                            && let ObjectOrReference::Object(ty_obj) = prop_schema
                                            && !ty_obj.enum_values.is_empty()
                                            && let Some(serde_json::Value::String(enum_val)) =
                                                ty_obj.enum_values.first()
                                        {
                                            enum_discriminator =
                                                Some((prop_name.clone(), enum_val.clone()));
                                        }

                                        let camel_case_name = prop_name.to_lower_camel_case();
                                        let original_name = prop_name.clone();

                                        all_properties.push(TsProperty {
                                            ts_name: camel_case_name,
                                            original_name,
                                            type_expr,
                                            optional: !is_required,
                                            is_index_signature: false,
                                            documentation: description.map(TsDocComment::new),
                                        });
                                    }
                                }
                            } else if let ObjectOrReference::Ref { ref_path, .. } = item
                                && let Some(schema_name) =
                                    ref_path.strip_prefix("#/components/schemas/")
                                && let Some(ref_schema_ref) = context.schemas.get(schema_name)
                                && let ObjectOrReference::Object(ref_obj_schema) = ref_schema_ref
                            {
                                for (prop_name, prop_schema) in &ref_obj_schema.properties {
                                    if seen_properties.insert(prop_name.clone()) {
                                        let type_expr = self.map_object_or_ref_schema_to_type(
                                            prop_schema,
                                            context,
                                            &inline_interface_name,
                                            Some(prop_name),
                                        );
                                        let is_required =
                                            ref_obj_schema.required.contains(prop_name);
                                        let description =
                                            self.extract_description_from_schema_ref(prop_schema);

                                        all_properties.push(TsProperty {
                                            ts_name: prop_name.to_lower_camel_case(),
                                            original_name: prop_name.clone(),
                                            type_expr,
                                            optional: !is_required,
                                            is_index_signature: false,
                                            documentation: description.map(TsDocComment::new),
                                        });
                                    }
                                }
                            }
                        }

                        if let Some((prop_name, enum_val)) = enum_discriminator {
                            context.register_enum_discriminator(
                                inline_interface_name.clone(),
                                prop_name,
                                enum_val,
                            );
                        }

                        let interface = TsInterfaceDefinition {
                            signature: TsInterfaceSignature::new(
                                inline_interface_name.clone(),
                                inline_interface_name.clone(),
                            ),
                            properties: all_properties,
                            documentation: None,
                        };
                        context.register_inline_interface(
                            inline_interface_name.clone(),
                            TsTypeDefinition::Interface(interface),
                        );
                    }

                    (TsExpression::Reference(inline_interface_name.clone()), true)
                } else {
                    let expr = self.map_object_or_ref_schema_to_type(
                        schema_ref,
                        context,
                        parent_name,
                        None,
                    );
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
        items: &[ObjectOrReference<ObjectSchema>],
        context: &mut SchemaContext,
        parent_name: &str,
    ) -> Vec<IntersectionMemberInfo> {
        items
            .iter()
            .map(|schema_ref| {
                let type_expr =
                    self.map_object_or_ref_schema_to_type(schema_ref, context, parent_name, None);

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
