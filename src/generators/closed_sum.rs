use heck::ToPascalCase;

use crate::ir::types::{
    IrPrimitive, IrSchemaKind, IrSpec, IrTaggedUnion, IrTypeExpr, IrUnion, TaggingStyle,
};

/// Target-independent plan for a closed sum emitted from an OpenAPI union.
///
/// The plan deliberately keeps payloads as IR expressions. Each generator owns
/// the conversion to the target language's semantic `TypeName` representation.
#[derive(Debug, Clone)]
pub(crate) struct ClosedSumPlan {
    pub name: String,
    pub docs: Vec<String>,
    pub cases: Vec<ClosedSumCasePlan>,
    pub encoding: ClosedSumEncoding,
}

#[derive(Debug, Clone)]
pub(crate) struct ClosedSumCasePlan {
    pub name: String,
    pub payload: Option<IrTypeExpr>,
    pub doc: Option<String>,
    pub wire_name: Option<String>,
    pub is_null: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum ClosedSumEncoding {
    Untagged,
    Internal {
        discriminator_field: String,
    },
    Adjacent {
        discriminator_field: String,
        content_field: String,
    },
    External,
}

pub(crate) fn plan_union(name: &str, description: Option<&str>, union: &IrUnion) -> ClosedSumPlan {
    let mut cases = Vec::with_capacity(union.members.len() + usize::from(union.nullable));
    let mut used_names = std::collections::HashSet::new();

    for (index, member) in union.members.iter().enumerate() {
        let desired_name = union_case_name(member, index);
        let case_name = unique_case_name(&desired_name, union_name(name), &mut used_names);
        cases.push(ClosedSumCasePlan {
            name: case_name,
            payload: Some(member.clone()),
            doc: None,
            wire_name: match member {
                IrTypeExpr::StringLiteral(value) => Some(value.clone()),
                _ => None,
            },
            is_null: false,
        });
    }

    if union.nullable {
        cases.push(ClosedSumCasePlan {
            name: unique_case_name("Null", union_name(name), &mut used_names),
            payload: None,
            doc: Some("The union's null value.".to_string()),
            wire_name: None,
            is_null: true,
        });
    }

    ClosedSumPlan {
        name: union_name(name),
        docs: description.into_iter().map(str::to_string).collect(),
        cases,
        encoding: ClosedSumEncoding::Untagged,
    }
}

pub(crate) fn plan_tagged_union(
    name: &str,
    description: Option<&str>,
    union: &IrTaggedUnion,
    ir: &IrSpec,
) -> ClosedSumPlan {
    let hint = match &union.tagging {
        TaggingStyle::Internal => {
            format!("Discriminator: {} (internal).", union.discriminator_field)
        }
        TaggingStyle::Adjacent { content_field } => format!(
            "Discriminator: {} / content: {} (adjacent).",
            union.discriminator_field, content_field
        ),
        TaggingStyle::External => "Discriminator: variant key (external).".to_string(),
    };
    let docs = match description {
        Some(description) => vec![format!("{description}\n\n{hint}")],
        None => vec![hint],
    };

    let mut used_names = std::collections::HashSet::new();
    let cases = union
        .variants
        .iter()
        .enumerate()
        .map(|(index, variant)| {
            let desired_name = if variant.discriminator_value.is_empty() {
                tagged_case_name(&variant.content_type, index)
            } else {
                variant.discriminator_value.to_pascal_case()
            };
            let case_name = unique_case_name(&desired_name, union_name(name), &mut used_names);
            ClosedSumCasePlan {
                name: case_name,
                payload: (!is_tag_only_variant(
                    ir,
                    &union.discriminator_field,
                    &variant.discriminator_value,
                    &variant.content_type,
                ) && !matches!(variant.content_type, IrTypeExpr::StringLiteral(_)))
                .then(|| variant.content_type.clone()),
                doc: variant.description.clone(),
                wire_name: Some(variant.discriminator_value.clone()),
                is_null: false,
            }
        })
        .collect();

    ClosedSumPlan {
        name: union_name(name),
        docs,
        cases,
        encoding: match &union.tagging {
            TaggingStyle::Internal => ClosedSumEncoding::Internal {
                discriminator_field: union.discriminator_field.clone(),
            },
            TaggingStyle::Adjacent { content_field } => ClosedSumEncoding::Adjacent {
                discriminator_field: union.discriminator_field.clone(),
                content_field: content_field.clone(),
            },
            TaggingStyle::External => ClosedSumEncoding::External,
        },
    }
}

fn union_name(name: &str) -> String {
    name.to_pascal_case()
}

fn unique_case_name(
    desired: &str,
    sum_name: String,
    used_names: &mut std::collections::HashSet<String>,
) -> String {
    let desired = if desired.is_empty() {
        "Variant".to_string()
    } else {
        desired.to_pascal_case()
    };
    let desired = format!("Variant{desired}");
    let desired = if desired == sum_name {
        format!("{desired}Variant")
    } else {
        desired
    };
    if used_names.insert(desired.clone()) {
        return desired;
    }
    for index in 2..=u32::MAX {
        let candidate = format!("{desired}{index}");
        if used_names.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!("u32 name space exhausted")
}

fn tagged_case_name(expr: &IrTypeExpr, index: usize) -> String {
    let name = match expr {
        IrTypeExpr::Named(name) => name.to_pascal_case(),
        _ => union_case_name(expr, index),
    };
    if name.is_empty() {
        format!("Variant{}", index + 1)
    } else {
        name
    }
}

fn union_case_name(expr: &IrTypeExpr, index: usize) -> String {
    let name = match expr {
        IrTypeExpr::Named(name) => name.to_pascal_case(),
        IrTypeExpr::Primitive(primitive) => primitive_case_name(primitive),
        IrTypeExpr::Array(_) => "Array".to_string(),
        IrTypeExpr::Map(_) => "Map".to_string(),
        IrTypeExpr::Nullable(_) => "Nullable".to_string(),
        IrTypeExpr::StringLiteral(value) => value.to_pascal_case(),
        IrTypeExpr::StringEnum(_) => "String".to_string(),
        IrTypeExpr::Union(_) => "Union".to_string(),
        IrTypeExpr::Any => "Any".to_string(),
    };
    if name.is_empty() {
        format!("Variant{}", index + 1)
    } else {
        name
    }
}

fn is_tag_only_variant(
    ir: &IrSpec,
    discriminator_field: &str,
    discriminator_value: &str,
    content_type: &IrTypeExpr,
) -> bool {
    let IrTypeExpr::Named(name) = content_type else {
        return false;
    };
    let Some(schema) = ir.schemas.get(name) else {
        return false;
    };
    let IrSchemaKind::Object(object) = &schema.kind else {
        return false;
    };
    if object.additional_properties.is_some() || object.properties.len() != 1 {
        return false;
    }
    let Some(property) = object.properties.get(discriminator_field) else {
        return false;
    };
    matches!(
        &property.type_expr,
        IrTypeExpr::StringLiteral(value) if value == discriminator_value
    )
}

fn primitive_case_name(primitive: &IrPrimitive) -> String {
    match primitive {
        IrPrimitive::String | IrPrimitive::StringWithFormat(_) => "String",
        IrPrimitive::Date | IrPrimitive::DateTime => "String",
        IrPrimitive::Uuid => "String",
        IrPrimitive::Binary => "Binary",
        IrPrimitive::Integer | IrPrimitive::IntegerWithFormat(_) => "Integer",
        IrPrimitive::Number | IrPrimitive::NumberWithFormat(_) => "Number",
        IrPrimitive::Boolean => "Boolean",
    }
    .to_string()
}
