//! Type mapping from OpenAPI schemas to Go types

use heck::ToPascalCase as _;
use utoipa::openapi;
use utoipa::openapi::schema::ArrayItems;
use utoipa::openapi::schema::{AdditionalProperties, KnownFormat, SchemaFormat, SchemaType, Type};

use crate::ast::common::GoDocComment;
use crate::ast::go_expression::GoExpression;
use crate::ast::ty::{GoPrimitive, GoStruct, GoTypeAlias, GoTypeDefinition};
use crate::config::GoHttpConfig;
use crate::consts::escape_go_keyword;
use crate::errors::GeneratorError;
use openapi_nexus_core::traits::OpenApiRefExt as _;

/// Convert an OpenAPI schema to a Go type expression
pub fn schema_to_go_expression(
    schema_ref: &openapi::RefOr<openapi::Schema>,
    components: Option<&openapi::Components>,
) -> Result<GoExpression, GeneratorError> {
    match schema_ref {
        openapi::RefOr::T(schema) => schema_to_go_expression_inner(schema, components),
        openapi::RefOr::Ref(reference) => {
            // Extract schema name from reference
            let schema_name =
                reference
                    .schema_name()
                    .ok_or_else(|| GeneratorError::TypeMapping {
                        source: Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("Invalid schema reference: {:?}", reference),
                        )),
                    })?;

            // Resolve the schema from components
            if let Some(components) = components
                && let Some(resolved_schema) = components.schemas.get(schema_name)
            {
                return schema_to_go_expression(resolved_schema, Some(components));
            }

            // If we can't resolve, use the schema name as a type reference
            Ok(GoExpression::Reference(schema_name.to_pascal_case()))
        }
    }
}

fn schema_to_go_expression_inner(
    schema: &openapi::Schema,
    components: Option<&openapi::Components>,
) -> Result<GoExpression, GeneratorError> {
    match schema {
        openapi::Schema::Object(obj_schema) => {
            // Check for format in object schema
            if let Some(format) = &obj_schema.format {
                match format {
                    SchemaFormat::KnownFormat(KnownFormat::DateTime) => {
                        Ok(GoExpression::Reference("time.Time".to_string()))
                    }
                    SchemaFormat::KnownFormat(KnownFormat::Date) => {
                        Ok(GoExpression::Reference("time.Time".to_string()))
                    }
                    SchemaFormat::KnownFormat(KnownFormat::Byte) => Ok(GoExpression::Slice(
                        Box::new(GoExpression::Primitive(GoPrimitive::Byte)),
                    )),
                    _ => Ok(GoExpression::Primitive(GoPrimitive::String)),
                }
            } else {
                // Default to string for object schemas without properties
                Ok(GoExpression::Primitive(GoPrimitive::String))
            }
        }
        openapi::Schema::Array(arr_schema) => {
            // Array items are stored as ArrayItems
            let item_type = match &arr_schema.items {
                ArrayItems::RefOrSchema(item_ref) => schema_to_go_expression(item_ref, components)?,
                ArrayItems::False => GoExpression::Any,
            };
            Ok(GoExpression::Slice(Box::new(item_type)))
        }
        openapi::Schema::OneOf(one_of) => {
            // For oneOf unions, use interface{} (any type) as Go doesn't have native union types
            // In the future, we could generate a discriminated union interface
            if let Some(first_item) = one_of.items.first() {
                // Use the first type in the union as a fallback, but wrap in interface{} for flexibility
                schema_to_go_expression(first_item, components)
            } else {
                Ok(GoExpression::Any)
            }
        }
        openapi::Schema::AnyOf(any_of) => {
            // Similar to oneOf, use interface{} for anyOf
            if let Some(first_item) = any_of.items.first() {
                schema_to_go_expression(first_item, components)
            } else {
                Ok(GoExpression::Any)
            }
        }
        openapi::Schema::AllOf(all_of) => {
            // For allOf (intersection), use the first type as Go doesn't support intersections
            // In practice, allOf is often used for composition, so we'll use the first schema
            if let Some(first_item) = all_of.items.first() {
                schema_to_go_expression(first_item, components)
            } else {
                Ok(GoExpression::Any)
            }
        }
        _ => Ok(GoExpression::Any),
    }
}

/// Convert an OpenAPI schema to a Go type definition (struct or type alias)
pub fn schema_to_go_type_definition(
    name: &str,
    schema_ref: &openapi::RefOr<openapi::Schema>,
    components: &openapi::Components,
) -> Result<GoTypeDefinition, GeneratorError> {
    let struct_name = name.to_pascal_case();

    match schema_ref {
        openapi::RefOr::T(openapi::Schema::Object(obj_schema)) => {
            // Check if this is an enum
            if let Some(enum_values) = &obj_schema.enum_values
                && !enum_values.is_empty()
            {
                // For enums, create a type alias to string
                let type_expr = GoExpression::Primitive(GoPrimitive::String);
                let doc = obj_schema
                    .description
                    .as_ref()
                    .map(|d| GoDocComment::new(d.clone()));
                return Ok(GoTypeDefinition::TypeAlias(
                    GoTypeAlias::new(struct_name, type_expr)
                        .with_doc(doc.unwrap_or_else(|| GoDocComment::new(String::new()))),
                ));
            }

            // Check if this has properties (struct)
            if !obj_schema.properties.is_empty() {
                let mut fields = Vec::new();
                for (prop_name, prop_schema) in &obj_schema.properties {
                    let field_name = escape_go_keyword(&prop_name.to_pascal_case());
                    let go_type = schema_to_go_expression(prop_schema, Some(components))?;

                    // Check if required
                    let required: bool = obj_schema.required.contains(prop_name);

                    // Check if nullable (schema_type includes Null)
                    let nullable = match prop_schema {
                        openapi::RefOr::T(openapi::Schema::Object(obj)) => {
                            matches!(&obj.schema_type, SchemaType::Array(types) if types.contains(&Type::Null))
                                || matches!(&obj.schema_type, SchemaType::Type(Type::Null))
                        }
                        openapi::RefOr::T(openapi::Schema::Array(arr)) => {
                            matches!(&arr.schema_type, SchemaType::Array(types) if types.contains(&Type::Null))
                                || matches!(&arr.schema_type, SchemaType::Type(Type::Null))
                        }
                        _ => false,
                    };

                    // For optional or nullable fields, use OptionalNullable
                    let final_type = if required && !nullable {
                        go_type
                    } else {
                        GoExpression::OptionalNullable(Box::new(go_type))
                    };

                    // Build JSON tag with omitempty/omitzero for optional fields
                    let json_tag = if required && !nullable {
                        format!("json:\"{}\"", prop_name)
                    } else {
                        format!("json:\"{},omitzero\"", prop_name)
                    };

                    let field = crate::ast::common::GoField::new(field_name, final_type)
                        .with_json_tag(json_tag);

                    fields.push(field);
                }

                // Add AdditionalProperties field if needed
                if let Some(additional_props) = &obj_schema.additional_properties {
                    match additional_props.as_ref() {
                        AdditionalProperties::FreeForm(true) => {
                            // additionalProperties: true -> map[string]any
                            let additional_field = crate::ast::common::GoField::new(
                                "AdditionalProperties".to_string(),
                                GoExpression::Map {
                                    key: Box::new(GoExpression::Primitive(GoPrimitive::String)),
                                    value: Box::new(GoExpression::Any),
                                },
                            )
                            .with_json_tag("additionalProperties:\"true\" json:\"-\"".to_string());
                            fields.push(additional_field);
                        }
                        AdditionalProperties::RefOr(schema_ref) => {
                            // additionalProperties: {schema} -> map[string]T
                            let value_type = schema_to_go_expression(schema_ref, Some(components))?;
                            let additional_field = crate::ast::common::GoField::new(
                                "AdditionalProperties".to_string(),
                                GoExpression::Map {
                                    key: Box::new(GoExpression::Primitive(GoPrimitive::String)),
                                    value: Box::new(value_type),
                                },
                            )
                            .with_json_tag("additionalProperties:\"true\" json:\"-\"".to_string());
                            fields.push(additional_field);
                        }
                        AdditionalProperties::FreeForm(false) => {
                            // No additional properties allowed - skip
                        }
                    }
                }

                let doc = obj_schema
                    .description
                    .as_ref()
                    .map(|d| GoDocComment::new(d.clone()));
                let mut struct_def = GoStruct::new(struct_name).with_fields(fields);
                if let Some(doc) = doc {
                    struct_def = struct_def.with_doc(doc);
                }

                return Ok(GoTypeDefinition::Struct(struct_def));
            }

            // Otherwise, create a type alias
            let type_expr = schema_to_go_expression(schema_ref, Some(components))?;
            let doc = obj_schema
                .description
                .as_ref()
                .map(|d| GoDocComment::new(d.clone()));
            let mut type_alias = GoTypeAlias::new(struct_name, type_expr);
            if let Some(doc) = doc {
                type_alias = type_alias.with_doc(doc);
            }
            Ok(GoTypeDefinition::TypeAlias(type_alias))
        }
        openapi::RefOr::T(openapi::Schema::OneOf(one_of)) => {
            // For oneOf unions, create a type alias to interface{} (any type)
            // Go doesn't have native union types, so we use interface{} for flexibility
            let type_expr = if let Some(first_item) = one_of.items.first() {
                schema_to_go_expression(first_item, Some(components))?
            } else {
                GoExpression::Any
            };
            Ok(GoTypeDefinition::TypeAlias(GoTypeAlias::new(
                struct_name,
                type_expr,
            )))
        }
        openapi::RefOr::T(openapi::Schema::AnyOf(any_of)) => {
            // Similar to oneOf, create a type alias
            let type_expr = if let Some(first_item) = any_of.items.first() {
                schema_to_go_expression(first_item, Some(components))?
            } else {
                GoExpression::Any
            };
            Ok(GoTypeDefinition::TypeAlias(GoTypeAlias::new(
                struct_name,
                type_expr,
            )))
        }
        openapi::RefOr::T(openapi::Schema::AllOf(all_of)) => {
            // For allOf (intersection), use the first type
            let type_expr = if let Some(first_item) = all_of.items.first() {
                schema_to_go_expression(first_item, Some(components))?
            } else {
                GoExpression::Any
            };
            Ok(GoTypeDefinition::TypeAlias(GoTypeAlias::new(
                struct_name,
                type_expr,
            )))
        }
        openapi::RefOr::T(_) => {
            // For other non-object schemas, create a type alias
            let type_expr = schema_to_go_expression(schema_ref, Some(components))?;
            Ok(GoTypeDefinition::TypeAlias(GoTypeAlias::new(
                struct_name,
                type_expr,
            )))
        }
        openapi::RefOr::Ref(reference) => {
            // If it's a reference, resolve it
            if let Some(schema_name) = reference.schema_name()
                && let Some(resolved) = components.schemas.get(schema_name)
            {
                return schema_to_go_type_definition(name, resolved, components);
            }

            // Fallback: create a type alias to the reference name
            let schema_name: String = reference.schema_name().unwrap_or(name).to_pascal_case();
            let type_expr = GoExpression::Reference(schema_name);
            Ok(GoTypeDefinition::TypeAlias(GoTypeAlias::new(
                struct_name,
                type_expr,
            )))
        }
    }
}

/// Generate a Go model from an OpenAPI schema (returns template data)
pub fn generate_model_data(
    name: &str,
    schema_ref: &openapi::RefOr<openapi::Schema>,
    components: &openapi::Components,
    _config: &GoHttpConfig,
    _header_data: &openapi_nexus_core::data::HeaderData,
) -> Result<(GoTypeDefinition, Vec<String>, Vec<String>), GeneratorError> {
    let type_def = schema_to_go_type_definition(name, schema_ref, components)?;

    // Collect imports
    let mut imports = Vec::new();

    // Check if we need optionalnullable import
    let needs_optionalnullable = match &type_def {
        crate::ast::ty::GoTypeDefinition::Struct(s) => s.fields.iter().any(|f| {
            matches!(
                &f.field_type,
                crate::ast::go_expression::GoExpression::OptionalNullable(_)
            )
        }),
        _ => false,
    };
    if needs_optionalnullable {
        // We'll add the module path later in the generator
        imports.push("optionalnullable".to_string());
    }

    // Check if we need utils import (for marshal/unmarshal)
    let needs_utils = matches!(&type_def, crate::ast::ty::GoTypeDefinition::Struct(_));
    if needs_utils {
        // We'll add the module path later in the generator
        imports.push("internal/utils".to_string());
    }

    // Collect required fields for marshal/unmarshal
    let required_fields = match schema_ref {
        openapi::RefOr::T(openapi::Schema::Object(obj_schema)) => obj_schema.required.clone(),
        _ => Vec::new(),
    };

    Ok((type_def, imports, required_fields))
}
