//! Type mapping from OpenAPI schemas to Go types

use heck::ToPascalCase as _;

use crate::ast::common::GoDocComment;
use crate::ast::go_expression::GoExpression;
use crate::ast::ty::{GoPrimitive, GoStruct, GoTypeAlias, GoTypeDefinition};
use crate::config::GoHttpConfig;
use crate::consts::escape_go_keyword;
use crate::errors::GeneratorError;
use openapi_nexus_core::traits::OpenApiRefExt as _;
use openapi_nexus_spec::oas31::spec::{
    BooleanSchema, Components, ObjectOrReference, ObjectSchema, Schema, SchemaType, SchemaTypeSet,
};

/// Convert an OpenAPI schema to a Go type expression
pub fn schema_to_go_expression(
    schema_ref: &ObjectOrReference<ObjectSchema>,
    components: Option<&Components>,
) -> Result<GoExpression, GeneratorError> {
    match schema_ref {
        ObjectOrReference::Object(schema) => schema_to_go_expression_inner(schema, components),
        ObjectOrReference::Ref { .. } => {
            let schema_name =
                schema_ref
                    .schema_name()
                    .ok_or_else(|| GeneratorError::TypeMapping {
                        source: Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("Invalid schema reference: {:?}", schema_ref),
                        )),
                    })?;
            // Use the component name as the type; do not resolve and recurse, or we'd
            // treat object schemas as interface{} when used as map values etc.
            Ok(GoExpression::Reference(schema_name.to_pascal_case()))
        }
    }
}

fn schema_to_go_expression_inner(
    obj_schema: &ObjectSchema,
    components: Option<&Components>,
) -> Result<GoExpression, GeneratorError> {
    // Format overrides
    if let Some(format) = &obj_schema.format {
        match format.as_str() {
            "date-time" | "date" => return Ok(GoExpression::Reference("time.Time".to_string())),
            "byte" => {
                return Ok(GoExpression::Slice(Box::new(GoExpression::Primitive(
                    GoPrimitive::Byte,
                ))));
            }
            _ => return Ok(GoExpression::Primitive(GoPrimitive::String)),
        }
    }

    // Enum
    if !obj_schema.enum_values.is_empty() {
        return Ok(GoExpression::Primitive(GoPrimitive::String));
    }

    // Object with only additionalProperties (map-like): emit map[string]ValueType
    if obj_schema.properties.is_empty()
        && obj_schema.items.is_none()
        && obj_schema.one_of.is_empty()
        && obj_schema.any_of.is_empty()
        && obj_schema.all_of.is_empty()
        && let Some(additional_props) = obj_schema.additional_properties.as_ref()
    {
        match additional_props {
            Schema::Object(schema_ref) => {
                let value_type = schema_to_go_expression(schema_ref.as_ref(), components)?;
                return Ok(GoExpression::Map {
                    key: Box::new(GoExpression::Primitive(GoPrimitive::String)),
                    value: Box::new(value_type),
                });
            }
            Schema::Boolean(BooleanSchema(true)) => {
                return Ok(GoExpression::Map {
                    key: Box::new(GoExpression::Primitive(GoPrimitive::String)),
                    value: Box::new(GoExpression::Any),
                });
            }
            Schema::Boolean(BooleanSchema(false)) => {}
        }
    }

    // Object with no properties -> string fallback
    if obj_schema.properties.is_empty()
        && obj_schema.items.is_none()
        && obj_schema.one_of.is_empty()
        && obj_schema.any_of.is_empty()
        && obj_schema.all_of.is_empty()
    {
        return Ok(GoExpression::Primitive(GoPrimitive::String));
    }

    // Array
    if obj_schema
        .schema_type
        .as_ref()
        .is_some_and(|t| t.is_array_or_nullable_array())
        && obj_schema.items.is_some()
    {
        let item_type = match obj_schema.items.as_ref().unwrap().as_ref() {
            Schema::Object(schema_ref) => schema_to_go_expression(schema_ref.as_ref(), components)?,
            Schema::Boolean(_) => GoExpression::Any,
        };
        return Ok(GoExpression::Slice(Box::new(item_type)));
    }

    // one_of / any_of / all_of: use first member
    if !obj_schema.one_of.is_empty() {
        return schema_to_go_expression(&obj_schema.one_of[0], components);
    }
    if !obj_schema.any_of.is_empty() {
        return schema_to_go_expression(&obj_schema.any_of[0], components);
    }
    if !obj_schema.all_of.is_empty() {
        return schema_to_go_expression(&obj_schema.all_of[0], components);
    }

    // Primitive from schema_type
    let base = map_schema_type_to_primitive(&obj_schema.schema_type);
    Ok(base)
}

fn map_schema_type_to_primitive(schema_type: &Option<SchemaTypeSet>) -> GoExpression {
    let type_set = match schema_type {
        Some(ts) => ts,
        None => return GoExpression::Any,
    };
    match type_set {
        SchemaTypeSet::Single(t) => map_single_type_to_primitive(*t),
        SchemaTypeSet::Multiple(types) => {
            let non_null: Vec<_> = types.iter().filter(|t| **t != SchemaType::Null).collect();
            if non_null.is_empty() {
                GoExpression::Any
            } else if non_null.len() == 1 {
                map_single_type_to_primitive(*non_null[0])
            } else {
                // Multiple non-null types -> use first
                map_single_type_to_primitive(*non_null[0])
            }
        }
    }
}

fn map_single_type_to_primitive(t: SchemaType) -> GoExpression {
    match t {
        SchemaType::String => GoExpression::Primitive(GoPrimitive::String),
        SchemaType::Integer => GoExpression::Primitive(GoPrimitive::Int),
        SchemaType::Number => GoExpression::Primitive(GoPrimitive::Float64),
        SchemaType::Boolean => GoExpression::Primitive(GoPrimitive::Bool),
        SchemaType::Array => GoExpression::Slice(Box::new(GoExpression::Any)),
        SchemaType::Object => GoExpression::Any,
        SchemaType::Null => GoExpression::Any,
    }
}

fn is_nullable(schema_ref: &ObjectOrReference<ObjectSchema>) -> bool {
    match schema_ref {
        ObjectOrReference::Object(obj) => obj
            .schema_type
            .as_ref()
            .is_some_and(|t| t.contains(SchemaType::Null)),
        ObjectOrReference::Ref { .. } => false,
    }
}

/// Convert an OpenAPI schema to a Go type definition (struct or type alias)
pub fn schema_to_go_type_definition(
    name: &str,
    schema_ref: &ObjectOrReference<ObjectSchema>,
    components: &Components,
) -> Result<GoTypeDefinition, GeneratorError> {
    let struct_name = name.to_pascal_case();

    match schema_ref {
        ObjectOrReference::Object(obj_schema) => {
            // Enum
            if !obj_schema.enum_values.is_empty() {
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

            // Struct with properties
            if !obj_schema.properties.is_empty() {
                let mut fields = Vec::new();
                for (prop_name, prop_schema) in &obj_schema.properties {
                    let field_name = escape_go_keyword(&prop_name.to_pascal_case());
                    let go_type = schema_to_go_expression(prop_schema, Some(components))?;
                    let required = obj_schema.required.contains(prop_name);
                    let nullable = is_nullable(prop_schema);
                    let final_type = if required && !nullable {
                        go_type
                    } else {
                        GoExpression::OptionalNullable(Box::new(go_type))
                    };
                    let json_tag = if required && !nullable {
                        format!("json:\"{}\"", prop_name)
                    } else {
                        format!("json:\"{},omitzero\"", prop_name)
                    };
                    let field = crate::ast::common::GoField::new(field_name, final_type)
                        .with_json_tag(json_tag);
                    fields.push(field);
                }

                // additionalProperties
                if let Some(additional_props) = &obj_schema.additional_properties {
                    match additional_props {
                        Schema::Object(schema_ref) => {
                            let value_type =
                                schema_to_go_expression(schema_ref.as_ref(), Some(components))?;
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
                        Schema::Boolean(BooleanSchema(true)) => {
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
                        Schema::Boolean(BooleanSchema(false)) => {}
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

            // one_of / any_of / all_of
            if !obj_schema.one_of.is_empty() {
                let type_expr = schema_to_go_expression(&obj_schema.one_of[0], Some(components))?;
                return Ok(GoTypeDefinition::TypeAlias(GoTypeAlias::new(
                    struct_name,
                    type_expr,
                )));
            }
            if !obj_schema.any_of.is_empty() {
                let type_expr = schema_to_go_expression(&obj_schema.any_of[0], Some(components))?;
                return Ok(GoTypeDefinition::TypeAlias(GoTypeAlias::new(
                    struct_name,
                    type_expr,
                )));
            }
            if !obj_schema.all_of.is_empty() {
                let type_expr = schema_to_go_expression(&obj_schema.all_of[0], Some(components))?;
                return Ok(GoTypeDefinition::TypeAlias(GoTypeAlias::new(
                    struct_name,
                    type_expr,
                )));
            }

            // Type alias for rest
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
        ObjectOrReference::Ref { .. } => {
            if let Some(schema_name) = schema_ref.schema_name()
                && let Some(resolved) = components.schemas.get(schema_name)
            {
                return schema_to_go_type_definition(name, resolved, components);
            }
            let schema_name: String = schema_ref.schema_name().unwrap_or(name).to_pascal_case();
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
    schema_ref: &ObjectOrReference<ObjectSchema>,
    components: &Components,
    _config: &GoHttpConfig,
    _header_data: &openapi_nexus_core::data::HeaderData,
) -> Result<(GoTypeDefinition, Vec<String>, Vec<String>), GeneratorError> {
    let type_def = schema_to_go_type_definition(name, schema_ref, components)?;

    let mut imports = Vec::new();

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
        imports.push("optionalnullable".to_string());
    }

    let needs_utils = matches!(&type_def, crate::ast::ty::GoTypeDefinition::Struct(_));
    if needs_utils {
        imports.push("internal/utils".to_string());
    }

    let required_fields = match schema_ref {
        ObjectOrReference::Object(obj_schema) => obj_schema.required.clone(),
        _ => Vec::new(),
    };

    Ok((type_def, imports, required_fields))
}
