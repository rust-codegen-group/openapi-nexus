//! Target-independent structural member selection for untagged conversion.
//!
//! This is deliberately not a JSON Schema validator: formats, numeric bounds,
//! and other validation keywords do not select conversion functions. Property
//! names stay in their wire form; each renderer owns its naming convention.

use std::collections::HashSet;

use crate::ir::types::{IrPrimitive, IrSchemaKind, IrSpec, IrTypeExpr, IrUnion, TaggingStyle};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum JsonShape {
    Any,
    Never,
    Null,
    Boolean,
    String,
    Number { integer: bool },
    Literal(serde_json::Value),
    Array(Box<Self>),
    Map(Box<Self>),
    Object(Vec<PropertyMatch>),
    AnyOf(Vec<Self>),
    AllOf(Vec<Self>),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PropertyMatch {
    pub wire_name: String,
    pub rename: bool,
    pub required: bool,
    pub shape: JsonShape,
}

#[derive(Debug, Clone)]
pub(crate) struct UnionMemberMatch {
    pub type_expr: IrTypeExpr,
    pub shape: JsonShape,
    pub fallback: bool,
}

pub(crate) fn plan_union_members(union: &IrUnion, ir: &IrSpec) -> Vec<UnionMemberMatch> {
    fn flatten<'a>(members: &'a [IrTypeExpr], output: &mut Vec<&'a IrTypeExpr>) {
        for member in members {
            if let IrTypeExpr::Union(nested) = member {
                flatten(nested, output);
            } else {
                output.push(member);
            }
        }
    }
    let mut flattened = Vec::new();
    flatten(&union.members, &mut flattened);
    let mut members: Vec<_> = flattened
        .into_iter()
        .map(|type_expr| {
            let shape = plan_shape(type_expr, ir, &mut HashSet::new());
            let fallback = matches!(&shape, JsonShape::Any)
                || matches!(&shape, JsonShape::Map(inner) if **inner == JsonShape::Any)
                || matches!(&shape, JsonShape::Object(properties) if properties.is_empty());
            UnionMemberMatch {
                type_expr: type_expr.clone(),
                shape,
                fallback,
            }
        })
        .collect();
    // Stable sorting retains schema order within both groups. An unrestricted
    // JSON alternative must not hide a concrete convertible object after it.
    members.sort_by_key(|member| member.fallback);
    members
}

fn any_of(shapes: Vec<JsonShape>) -> JsonShape {
    if shapes.contains(&JsonShape::Any) {
        return JsonShape::Any;
    }
    match shapes.len() {
        0 => JsonShape::Never,
        1 => shapes.into_iter().next().expect("one union shape"),
        _ => JsonShape::AnyOf(shapes),
    }
}

fn plan_shape(expr: &IrTypeExpr, ir: &IrSpec, visiting: &mut HashSet<String>) -> JsonShape {
    match expr {
        IrTypeExpr::Any => JsonShape::Any,
        IrTypeExpr::Primitive(primitive) => match primitive {
            IrPrimitive::Boolean => JsonShape::Boolean,
            IrPrimitive::Integer | IrPrimitive::IntegerWithFormat(_) => {
                JsonShape::Number { integer: true }
            }
            IrPrimitive::Number | IrPrimitive::NumberWithFormat(_) => {
                JsonShape::Number { integer: false }
            }
            _ => JsonShape::String,
        },
        IrTypeExpr::StringLiteral(value) => {
            JsonShape::Literal(serde_json::Value::String(value.clone()))
        }
        IrTypeExpr::StringEnum(values) => any_of(
            values
                .iter()
                .map(|value| JsonShape::Literal(serde_json::Value::String(value.clone())))
                .collect(),
        ),
        IrTypeExpr::Array(inner) => JsonShape::Array(Box::new(plan_shape(inner, ir, visiting))),
        IrTypeExpr::Map(inner) => JsonShape::Map(Box::new(plan_shape(inner, ir, visiting))),
        IrTypeExpr::Nullable(inner) => {
            any_of(vec![JsonShape::Null, plan_shape(inner, ir, visiting)])
        }
        IrTypeExpr::Union(members) => any_of(
            members
                .iter()
                .map(|member| plan_shape(member, ir, visiting))
                .collect(),
        ),
        IrTypeExpr::Named(name) => {
            // Recursive payloads are still converted recursively by the member
            // converter. Stop expanding the selection guard at a cycle.
            if !visiting.insert(name.clone()) {
                return JsonShape::Any;
            }
            let shape = match ir.schemas.get(name).map(|schema| &schema.kind) {
                Some(IrSchemaKind::Object(object)) => {
                    if object.properties.is_empty() {
                        object.additional_properties.as_ref().map_or_else(
                            || JsonShape::Object(Vec::new()),
                            |value| JsonShape::Map(Box::new(plan_shape(value, ir, visiting))),
                        )
                    } else {
                        JsonShape::Object(
                            object
                                .properties
                                .iter()
                                .map(|(wire_name, property)| {
                                    let shape = plan_shape(&property.type_expr, ir, visiting);
                                    PropertyMatch {
                                        wire_name: wire_name.clone(),
                                        rename: true,
                                        required: property.required,
                                        shape: if property.nullable {
                                            any_of(vec![JsonShape::Null, shape])
                                        } else {
                                            shape
                                        },
                                    }
                                })
                                .collect(),
                        )
                    }
                }
                Some(IrSchemaKind::Enum(enumeration)) => any_of(
                    enumeration
                        .values
                        .iter()
                        .map(|value| JsonShape::Literal(value.value.clone()))
                        .collect(),
                ),
                Some(IrSchemaKind::Alias(inner)) => plan_shape(inner, ir, visiting),
                Some(IrSchemaKind::Union(union)) => {
                    let mut shapes: Vec<_> = union
                        .members
                        .iter()
                        .map(|member| plan_shape(member, ir, visiting))
                        .collect();
                    if union.nullable {
                        shapes.push(JsonShape::Null);
                    }
                    any_of(shapes)
                }
                Some(IrSchemaKind::Intersection(intersection)) => JsonShape::AllOf(
                    intersection
                        .members
                        .iter()
                        .map(|member| plan_shape(member, ir, visiting))
                        .collect(),
                ),
                Some(IrSchemaKind::TaggedUnion(union)) => any_of(
                    union
                        .variants
                        .iter()
                        .map(|variant| {
                            let tag = PropertyMatch {
                                wire_name: union.discriminator_field.clone(),
                                rename: true,
                                required: true,
                                shape: JsonShape::Literal(serde_json::Value::String(
                                    variant.discriminator_value.clone(),
                                )),
                            };
                            let payload = plan_shape(&variant.content_type, ir, visiting);
                            match &union.tagging {
                                TaggingStyle::Internal
                                    if matches!(
                                        variant.content_type,
                                        IrTypeExpr::StringLiteral(_)
                                    ) =>
                                {
                                    JsonShape::Object(vec![tag])
                                }
                                TaggingStyle::Internal => {
                                    JsonShape::AllOf(vec![JsonShape::Object(vec![tag]), payload])
                                }
                                TaggingStyle::Adjacent { content_field } => {
                                    JsonShape::Object(vec![
                                        tag,
                                        PropertyMatch {
                                            wire_name: content_field.clone(),
                                            rename: true,
                                            required: true,
                                            shape: payload,
                                        },
                                    ])
                                }
                                TaggingStyle::External => JsonShape::Object(vec![PropertyMatch {
                                    wire_name: variant.discriminator_value.clone(),
                                    rename: false,
                                    required: true,
                                    shape: payload,
                                }]),
                            }
                        })
                        .collect(),
                ),
                None => JsonShape::Any,
            };
            visiting.remove(name);
            shape
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> IrSpec {
        let parsed = crate::parser::parse_content_yaml(include_str!(
            "../../tests/fixtures/valid/typescript-untagged-union.yaml"
        ))
        .unwrap();
        crate::ir::lower::lower(parsed).unwrap()
    }

    #[test]
    fn mixed_members_retain_required_wire_fields_and_literal_constraints() {
        let ir = fixture();
        let IrSchemaKind::Union(union) = &ir.schemas["Tool"].kind else {
            panic!("mixed tool remains untagged");
        };
        let members = plan_union_members(union, &ir);
        let JsonShape::Object(properties) = &members[0].shape else {
            panic!("client member is an object");
        };
        assert!(
            properties
                .iter()
                .any(|property| property.wire_name == "input_schema" && property.required)
        );
        let JsonShape::Object(properties) = &members[1].shape else {
            panic!("search member is an object");
        };
        assert!(
            properties
                .iter()
                .any(|property| property.wire_name == "type"
                    && property.shape == JsonShape::Literal("web_search_20250305".into()))
        );
    }

    #[test]
    fn raw_json_fallback_does_not_shadow_a_concrete_member() {
        let ir = fixture();
        let union = IrUnion {
            members: vec![IrTypeExpr::Any, IrTypeExpr::Named("ClientTool".into())],
            nullable: false,
        };
        let members = plan_union_members(&union, &ir);
        assert!(!members[0].fallback);
        assert!(matches!(&members[0].type_expr, IrTypeExpr::Named(name) if name == "ClientTool"));
        assert!(members[1].fallback);
    }

    #[test]
    fn recursive_alias_guard_expansion_terminates() {
        let mut ir = fixture();
        ir.schemas.get_mut("ClientTool").unwrap().kind =
            IrSchemaKind::Alias(IrTypeExpr::Named("ClientTool".into()));
        assert_eq!(
            plan_shape(
                &IrTypeExpr::Named("ClientTool".into()),
                &ir,
                &mut HashSet::new()
            ),
            JsonShape::Any
        );
    }

    #[test]
    fn inline_nested_union_members_are_selected_individually() {
        let ir = fixture();
        let union = IrUnion {
            members: vec![IrTypeExpr::Union(vec![
                IrTypeExpr::Named("ClientTool".into()),
                IrTypeExpr::Named("SearchTool".into()),
            ])],
            nullable: false,
        };
        let members = plan_union_members(&union, &ir);
        assert_eq!(members.len(), 2);
        assert!(members.iter().all(|member| !member.fallback));
    }

    #[test]
    fn external_variant_keys_are_not_ergonomic_property_names() {
        use crate::ir::types::{IrTaggedUnion, IrTaggedVariant};
        let mut ir = fixture();
        ir.schemas.get_mut("ClientTool").unwrap().kind = IrSchemaKind::TaggedUnion(IrTaggedUnion {
            discriminator_field: String::new(),
            tagging: TaggingStyle::External,
            variants: vec![IrTaggedVariant {
                discriminator_value: "snake_tag".into(),
                content_type: IrTypeExpr::Named("SearchTool".into()),
                description: None,
                additional_properties: None,
            }],
        });
        let shape = plan_shape(
            &IrTypeExpr::Named("ClientTool".into()),
            &ir,
            &mut HashSet::new(),
        );
        let JsonShape::Object(properties) = shape else {
            panic!("external wrapper")
        };
        assert_eq!(properties[0].wire_name, "snake_tag");
        assert!(!properties[0].rename);
    }
}
