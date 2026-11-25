//! Target-independent structural member selection for untagged conversion.
//!
//! This is deliberately not a JSON Schema validator: formats, numeric bounds,
//! and other validation keywords do not select conversion functions. Property
//! names stay in their wire form; each renderer owns its naming convention.

use std::collections::HashMap;

use crate::generators::closed_sum::is_tagged_unit_variant;
use crate::ir::types::{
    IrPrimitive, IrSchemaKind, IrSpec, IrTaggedUnion, IrTaggedVariant, IrTypeExpr, IrUnion,
    TaggingStyle,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MatchScope {
    Any,
    Concrete,
    Fallback,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum JsonShape {
    Any,
    Never,
    Null,
    Boolean,
    String,
    Binary,
    Number {
        integer: bool,
    },
    Literal(serde_json::Value),
    /// A nullable wrapper grants permission without inventing an authored member.
    Nullable(Box<Self>),
    Array(Box<Self>),
    Map(Box<Self>),
    MapWithConcreteValue {
        values: Box<Self>,
        concrete: Box<Self>,
    },
    Object(Vec<PropertyMatch>),
    ObjectWithAdditional {
        properties: Vec<PropertyMatch>,
        values: Box<Self>,
    },
    /// Productive recursion retains a graph edge rather than weakening to Any.
    Reference {
        schema: String,
        scope: MatchScope,
        unrestricted: bool,
        accepts_null: bool,
    },
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
    /// Index of the authored member, before concrete/fallback partitioning.
    pub source_index: usize,
    pub type_expr: IrTypeExpr,
    pub shape: JsonShape,
    pub fallback: bool,
}

type NamedShapeOverride<'a> = &'a dyn Fn(&str) -> Option<JsonShape>;

/// Transitional policy for existing representation-dependent predicates.
/// An adapter can bind a named wire predicate without flattening its schema.
#[derive(Clone, Copy)]
pub(crate) struct ShapePolicy<'a> {
    pub(crate) tag_projection: TagProjection,
    pub(crate) override_named: Option<NamedShapeOverride<'a>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TagProjection {
    FixedRequired,
    AuthoredValues,
}

impl ShapePolicy<'_> {
    const FIXED_REQUIRED: Self = Self {
        tag_projection: TagProjection::FixedRequired,
        override_named: None,
    };
    const AUTHORED_VALUES: Self = Self {
        tag_projection: TagProjection::AuthoredValues,
        override_named: None,
    };
}

pub(crate) fn plan_union_members(
    name: &str,
    union: &IrUnion,
    ir: &IrSpec,
) -> Vec<UnionMemberMatch> {
    plan_members(&union.members, ir, Some(name), ShapePolicy::FIXED_REQUIRED)
}

pub(crate) fn plan_typescript_union_members(
    name: &str,
    union: &IrUnion,
    ir: &IrSpec,
) -> Vec<UnionMemberMatch> {
    plan_members(&union.members, ir, Some(name), ShapePolicy::AUTHORED_VALUES)
}

/// Inline unions inside containers select a new value, not the enclosing schema.
pub(crate) fn plan_inline_union_members(
    members: &[IrTypeExpr],
    ir: &IrSpec,
) -> Vec<UnionMemberMatch> {
    plan_members(members, ir, None, ShapePolicy::FIXED_REQUIRED)
}

pub(crate) fn plan_inline_typescript_union_members(
    members: &[IrTypeExpr],
    ir: &IrSpec,
) -> Vec<UnionMemberMatch> {
    plan_members(members, ir, None, ShapePolicy::AUTHORED_VALUES)
}

pub(crate) fn plan_members(
    types: &[IrTypeExpr],
    ir: &IrSpec,
    current_schema: Option<&str>,
    policy: ShapePolicy<'_>,
) -> Vec<UnionMemberMatch> {
    let mut members: Vec<_> = types
        .iter()
        .enumerate()
        .flat_map(|(source_index, type_expr)| {
            let mut visiting = current_schema
                .map(|name| HashMap::from([(name.to_string(), 0)]))
                .unwrap_or_default();
            let shape = plan_shape_with_policy(type_expr, ir, &mut visiting, 0, policy);
            let (concrete, fallback) = selection_shapes(shape);
            [(concrete, false), (fallback, true)]
                .into_iter()
                .filter_map(move |(shape, fallback)| {
                    (shape != JsonShape::Never).then(|| UnionMemberMatch {
                        source_index,
                        type_expr: type_expr.clone(),
                        shape,
                        fallback,
                    })
                })
        })
        .collect();
    // Stable sorting retains schema order within both groups. An unrestricted
    // JSON alternative must not hide a concrete convertible object after it.
    members.sort_by_key(|member| member.fallback);
    members
}

fn any_of(shapes: Vec<JsonShape>) -> JsonShape {
    let mut flattened = Vec::new();
    for shape in shapes {
        match shape {
            JsonShape::Never => {}
            JsonShape::AnyOf(shapes) => {
                for shape in shapes {
                    if !flattened.contains(&shape) {
                        flattened.push(shape);
                    }
                }
            }
            shape => {
                if !flattened.contains(&shape) {
                    flattened.push(shape);
                }
            }
        }
    }
    // Unlike a boolean validator, a conversion plan must retain the concrete
    // alternatives next to Any so their converters can still be selected.
    match flattened.len() {
        0 => JsonShape::Never,
        1 => flattened.into_iter().next().expect("one union shape"),
        _ => JsonShape::AnyOf(flattened),
    }
}

fn all_of(shapes: Vec<JsonShape>) -> JsonShape {
    let mut flattened = Vec::new();
    for shape in shapes {
        match shape {
            JsonShape::Never => return JsonShape::Never,
            JsonShape::Any => {}
            JsonShape::AllOf(shapes) => flattened.extend(shapes),
            shape => flattened.push(shape),
        }
    }
    match flattened.len() {
        0 => JsonShape::Any,
        1 => flattened
            .into_iter()
            .next()
            .expect("one intersection shape"),
        _ => JsonShape::AllOf(flattened),
    }
}

fn nullable(shape: JsonShape) -> JsonShape {
    match shape {
        JsonShape::Any | JsonShape::Null | JsonShape::Nullable(_) => shape,
        JsonShape::Never => JsonShape::Null,
        shape => JsonShape::Nullable(Box::new(shape)),
    }
}

fn shape_accepts_null(shape: &JsonShape) -> bool {
    match shape {
        JsonShape::Any | JsonShape::Null | JsonShape::Nullable(_) => true,
        JsonShape::Literal(value) => value.is_null(),
        JsonShape::Reference {
            scope: MatchScope::Any,
            accepts_null,
            ..
        } => *accepts_null,
        JsonShape::AnyOf(shapes) => shapes.iter().any(shape_accepts_null),
        JsonShape::AllOf(shapes) => shapes.iter().all(shape_accepts_null),
        _ => false,
    }
}

pub(crate) fn accepts_any(shape: &JsonShape) -> bool {
    match shape {
        JsonShape::Any => true,
        JsonShape::Reference {
            scope: MatchScope::Any,
            unrestricted,
            ..
        } => *unrestricted,
        JsonShape::Nullable(inner) => accepts_any(inner),
        JsonShape::AnyOf(shapes) => shapes.iter().any(accepts_any),
        JsonShape::AllOf(shapes) => shapes.iter().all(accepts_any),
        _ => false,
    }
}

/// Keep dispatch priority separate from the original member's conversion
/// boundary. A named open union has both a concrete guard and a fallback,
/// but both branches must call that named converter (including its policy).
pub(crate) fn selection_shapes(shape: JsonShape) -> (JsonShape, JsonShape) {
    match shape {
        JsonShape::Never => (JsonShape::Never, JsonShape::Never),
        JsonShape::Any => (JsonShape::Never, JsonShape::Any),
        JsonShape::Reference {
            schema,
            unrestricted,
            accepts_null,
            ..
        } => (
            JsonShape::Reference {
                schema: schema.clone(),
                scope: MatchScope::Concrete,
                unrestricted,
                accepts_null,
            },
            JsonShape::Reference {
                schema,
                scope: MatchScope::Fallback,
                unrestricted,
                accepts_null,
            },
        ),
        JsonShape::Nullable(inner) => {
            let already_nullable = shape_accepts_null(&inner);
            let (concrete, fallback) = selection_shapes(*inner);
            if already_nullable {
                // Preserve authored concrete null paths beside an opaque fallback.
                // A repeated wrapper adds permission, not a new candidate.
                (concrete, fallback)
            } else if concrete == JsonShape::Never && fallback != JsonShape::Never {
                (concrete, any_of(vec![JsonShape::Null, fallback]))
            } else {
                (any_of(vec![JsonShape::Null, concrete]), fallback)
            }
        }
        JsonShape::Map(inner) if accepts_any(&inner) => dictionary_selection(inner),
        // Authored objects use the wire contract: an open fieldless object
        // has evidence only from matching entries. A closed empty object, by
        // contrast, has an explicit structural restriction even with no entries.
        JsonShape::ObjectWithAdditional { properties, values }
            if properties.is_empty() && *values != JsonShape::Never =>
        {
            dictionary_selection(values)
        }
        JsonShape::Object(ref properties) if properties.is_empty() => (JsonShape::Never, shape),
        JsonShape::AnyOf(shapes) => {
            let (concrete, fallback): (Vec<_>, Vec<_>) =
                shapes.into_iter().map(selection_shapes).unzip();
            (any_of(concrete), any_of(fallback))
        }
        JsonShape::AllOf(shapes) => {
            let shape = all_of(shapes);
            let JsonShape::AllOf(shapes) = &shape else {
                return selection_shapes(shape);
            };
            let (concrete, fallback): (Vec<_>, Vec<_>) =
                shapes.iter().cloned().map(selection_shapes).unzip();
            // Every conjunct must accept a candidate; a concrete path must also
            // match at least one concrete conjunct. No distributive expansion.
            (all_of(vec![shape, any_of(concrete)]), all_of(fallback))
        }
        shape => (shape, JsonShape::Never),
    }
}

fn dictionary_selection(values: Box<JsonShape>) -> (JsonShape, JsonShape) {
    let (concrete, fallback) = selection_shapes((*values).clone());
    let concrete = if concrete == JsonShape::Never {
        JsonShape::Never
    } else {
        // All entries must match the value shape; at least one must supply
        // concrete evidence. The empty dictionary stays in the fallback tier.
        JsonShape::MapWithConcreteValue {
            values,
            concrete: Box::new(concrete),
        }
    };
    (concrete, JsonShape::Map(Box::new(fallback)))
}

fn plan_tagged_variant_shape_at(
    union: &IrTaggedUnion,
    variant: &IrTaggedVariant,
    ir: &IrSpec,
    visiting: &mut HashMap<String, usize>,
    depth: usize,
    policy: ShapePolicy<'_>,
) -> JsonShape {
    let authored_values = policy.tag_projection == TagProjection::AuthoredValues;
    let tag_values = if authored_values && !variant.discriminator_values.is_empty() {
        variant.discriminator_values.clone()
    } else {
        vec![variant.discriminator_value.clone()]
    };
    let tag = PropertyMatch {
        wire_name: union.discriminator_field.clone(),
        rename: true,
        required: !authored_values || !variant.discriminator_optional,
        shape: any_of(
            tag_values
                .into_iter()
                .map(|value| JsonShape::Literal(serde_json::Value::String(value)))
                .collect(),
        ),
    };
    let tag_only = is_tagged_unit_variant(ir, union, variant);
    match &union.tagging {
        TaggingStyle::Internal | TaggingStyle::Adjacent { .. } if tag_only => {
            JsonShape::Object(vec![tag])
        }
        TaggingStyle::Internal => all_of(vec![
            JsonShape::Object(vec![tag]),
            plan_shape_with_policy(&variant.content_type, ir, visiting, depth, policy),
        ]),
        TaggingStyle::Adjacent { content_field } => JsonShape::Object(vec![
            tag,
            PropertyMatch {
                wire_name: content_field.clone(),
                rename: true,
                required: true,
                shape: plan_shape_with_policy(
                    &variant.content_type,
                    ir,
                    visiting,
                    depth + 1,
                    policy,
                ),
            },
        ]),
        TaggingStyle::External => {
            let properties = vec![PropertyMatch {
                wire_name: variant.discriminator_value.clone(),
                rename: false,
                required: true,
                shape: plan_shape_with_policy(
                    &variant.content_type,
                    ir,
                    visiting,
                    depth + 1,
                    policy,
                ),
            }];
            match &variant.additional_properties {
                Some(expr) => JsonShape::ObjectWithAdditional {
                    properties,
                    values: Box::new(plan_shape_with_policy(
                        expr,
                        ir,
                        visiting,
                        depth + 1,
                        policy,
                    )),
                },
                None => JsonShape::Object(properties),
            }
        }
    }
}

fn plan_shape_with_policy(
    expr: &IrTypeExpr,
    ir: &IrSpec,
    visiting: &mut HashMap<String, usize>,
    depth: usize,
    policy: ShapePolicy<'_>,
) -> JsonShape {
    match expr {
        IrTypeExpr::ScalarEnum {
            representation,
            representation_is_constraint,
            representation_nullable,
            values,
        } => {
            let shape = if *representation_is_constraint {
                plan_shape_with_policy(representation, ir, visiting, depth, policy)
            } else {
                JsonShape::Any
            };
            all_of(vec![
                if *representation_nullable {
                    nullable(shape)
                } else {
                    shape
                },
                any_of(values.iter().cloned().map(JsonShape::Literal).collect()),
            ])
        }
        IrTypeExpr::Any => JsonShape::Any,
        IrTypeExpr::Primitive(primitive) => match primitive {
            IrPrimitive::Boolean => JsonShape::Boolean,
            IrPrimitive::Binary => JsonShape::Binary,
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
        IrTypeExpr::Array(inner) => JsonShape::Array(Box::new(plan_shape_with_policy(
            inner,
            ir,
            visiting,
            depth + 1,
            policy,
        ))),
        IrTypeExpr::Map(inner) => JsonShape::Map(Box::new(plan_shape_with_policy(
            inner,
            ir,
            visiting,
            depth + 1,
            policy,
        ))),
        IrTypeExpr::Nullable(inner) => {
            nullable(plan_shape_with_policy(inner, ir, visiting, depth, policy))
        }
        IrTypeExpr::Union(members) => any_of(
            members
                .iter()
                .map(|member| plan_shape_with_policy(member, ir, visiting, depth, policy))
                .collect(),
        ),
        IrTypeExpr::Named(name) => {
            if let Some(shape) = policy.override_named.and_then(|resolve| resolve(name)) {
                return shape;
            }
            // Recursive payloads are still converted recursively by the member
            // converter. Stop expanding the selection guard at a cycle.
            if let Some(previous_depth) = visiting.get(name) {
                // A cycle through an array/map/property has consumed a value;
                // only its guard expansion stops. An alias cycle at the same
                // value cannot make progress and must not invoke itself.
                return if depth > *previous_depth {
                    JsonShape::Reference {
                        schema: name.clone(),
                        scope: MatchScope::Any,
                        unrestricted: unrestricted_type(expr, ir, &mut Default::default()),
                        accepts_null: accepts_null(expr, ir),
                    }
                } else {
                    JsonShape::Never
                };
            }
            visiting.insert(name.clone(), depth);
            let shape = match ir.schemas.get(name).map(|schema| &schema.kind) {
                Some(IrSchemaKind::Object(object)) => {
                    if object.properties.is_empty() {
                        object.additional_properties.as_ref().map_or_else(
                            || JsonShape::Object(Vec::new()),
                            |value| {
                                JsonShape::Map(Box::new(plan_shape_with_policy(
                                    value,
                                    ir,
                                    visiting,
                                    depth + 1,
                                    policy,
                                )))
                            },
                        )
                    } else {
                        let properties = object
                            .properties
                            .iter()
                            .map(|(wire_name, property)| {
                                let shape = plan_shape_with_policy(
                                    &property.type_expr,
                                    ir,
                                    visiting,
                                    depth + 1,
                                    policy,
                                );
                                PropertyMatch {
                                    wire_name: wire_name.clone(),
                                    rename: true,
                                    required: property.required,
                                    // Nullability describes the type's value
                                    // kind, but a scalar enum at this same node
                                    // can still exclude null. Its shape already
                                    // includes null exactly when listed.
                                    shape: if property.nullable
                                        && !matches!(
                                            property.type_expr,
                                            IrTypeExpr::ScalarEnum { .. }
                                                | IrTypeExpr::StringLiteral(_)
                                                | IrTypeExpr::StringEnum(_)
                                        ) {
                                        nullable(shape)
                                    } else {
                                        shape
                                    },
                                }
                            })
                            .collect();
                        match &object.additional_properties {
                            Some(value) => JsonShape::ObjectWithAdditional {
                                properties,
                                values: Box::new(plan_shape_with_policy(
                                    value,
                                    ir,
                                    visiting,
                                    depth + 1,
                                    policy,
                                )),
                            },
                            None => JsonShape::Object(properties),
                        }
                    }
                }
                Some(IrSchemaKind::Enum(enumeration)) => any_of(
                    enumeration
                        .values
                        .iter()
                        .map(|value| JsonShape::Literal(value.value.clone()))
                        .collect(),
                ),
                Some(IrSchemaKind::Alias(inner)) => {
                    plan_shape_with_policy(inner, ir, visiting, depth, policy)
                }
                Some(IrSchemaKind::Union(union)) => {
                    let shape = any_of(
                        union
                            .members
                            .iter()
                            .map(|member| {
                                plan_shape_with_policy(member, ir, visiting, depth, policy)
                            })
                            .collect(),
                    );
                    if union.nullable {
                        nullable(shape)
                    } else {
                        shape
                    }
                }
                Some(IrSchemaKind::Intersection(intersection)) => all_of(
                    intersection
                        .members
                        .iter()
                        .map(|member| plan_shape_with_policy(member, ir, visiting, depth, policy))
                        .collect(),
                ),
                Some(IrSchemaKind::TaggedUnion(union)) => any_of(
                    union
                        .variants
                        .iter()
                        .map(|variant| {
                            plan_tagged_variant_shape_at(
                                union, variant, ir, visiting, depth, policy,
                            )
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
fn plan_shape(
    expr: &IrTypeExpr,
    ir: &IrSpec,
    visiting: &mut HashMap<String, usize>,
    depth: usize,
) -> JsonShape {
    plan_shape_with_policy(expr, ir, visiting, depth, ShapePolicy::FIXED_REQUIRED)
}

pub(crate) fn plan_tagged_variant_shape(
    union: &IrTaggedUnion,
    variant: &IrTaggedVariant,
    ir: &IrSpec,
) -> JsonShape {
    plan_tagged_variant_shape_at(
        union,
        variant,
        ir,
        &mut HashMap::new(),
        0,
        ShapePolicy::AUTHORED_VALUES,
    )
}

/// Shapes are shared semantic data; target renderers decide property spelling
/// and runtime representations, including binary values.
pub(crate) fn plan_named_shape(name: &str, ir: &IrSpec) -> JsonShape {
    plan_named_shape_with_policy(name, ir, ShapePolicy::FIXED_REQUIRED)
}

pub(crate) fn plan_named_shape_with_policy(
    name: &str,
    ir: &IrSpec,
    policy: ShapePolicy<'_>,
) -> JsonShape {
    plan_shape_with_policy(
        &IrTypeExpr::Named(name.to_string()),
        ir,
        &mut HashMap::new(),
        0,
        policy,
    )
}

/// Fixed-value null acceptance never follows container payloads. This fact also
/// travels on productive reference edges, where guard expansion has stopped.
pub(crate) fn accepts_null(expr: &IrTypeExpr, ir: &IrSpec) -> bool {
    fn visit(expr: &IrTypeExpr, ir: &IrSpec, seen: &mut std::collections::HashSet<String>) -> bool {
        match expr {
            IrTypeExpr::Any | IrTypeExpr::Nullable(_) => true,
            IrTypeExpr::ScalarEnum {
                representation,
                representation_is_constraint,
                representation_nullable,
                values,
            } => {
                values.iter().any(serde_json::Value::is_null)
                    && (!representation_is_constraint
                        || *representation_nullable
                        || visit(representation, ir, seen))
            }
            IrTypeExpr::Union(members) => members.iter().any(|member| visit(member, ir, seen)),
            IrTypeExpr::Named(name) => {
                if !seen.insert(name.clone()) {
                    return false;
                }
                let accepts = match ir.schemas.get(name).map(|schema| &schema.kind) {
                    Some(IrSchemaKind::Alias(inner)) => visit(inner, ir, seen),
                    Some(IrSchemaKind::Union(union)) => {
                        union.nullable || union.members.iter().any(|member| visit(member, ir, seen))
                    }
                    Some(IrSchemaKind::Intersection(all)) => {
                        all.members.iter().all(|member| visit(member, ir, seen))
                    }
                    Some(IrSchemaKind::Enum(enumeration)) => {
                        enumeration.values.iter().any(|value| value.value.is_null())
                    }
                    None => true,
                    _ => false,
                };
                seen.remove(name);
                accepts
            }
            _ => false,
        }
    }
    visit(expr, ir, &mut Default::default())
}

/// Whether the schema accepts arbitrary values, independently of conversion
/// evidence. This fixed-value traversal never follows container payloads.
fn unrestricted_type(
    expr: &IrTypeExpr,
    ir: &IrSpec,
    seen: &mut std::collections::HashSet<String>,
) -> bool {
    match expr {
        IrTypeExpr::Any => true,
        IrTypeExpr::Nullable(inner) => unrestricted_type(inner, ir, seen),
        IrTypeExpr::Union(members) => members
            .iter()
            .any(|member| unrestricted_type(member, ir, seen)),
        IrTypeExpr::Named(name) => {
            if !seen.insert(name.clone()) {
                return false;
            }
            let unrestricted = match ir.schemas.get(name).map(|schema| &schema.kind) {
                Some(IrSchemaKind::Alias(inner)) => unrestricted_type(inner, ir, seen),
                Some(IrSchemaKind::Union(union)) => union
                    .members
                    .iter()
                    .any(|member| unrestricted_type(member, ir, seen)),
                Some(IrSchemaKind::Intersection(all)) => all
                    .members
                    .iter()
                    .all(|member| unrestricted_type(member, ir, seen)),
                None => true,
                _ => false,
            };
            seen.remove(name);
            unrestricted
        }
        _ => false,
    }
}

pub(crate) fn referenced_shapes(shape: &JsonShape, names: &mut std::collections::BTreeSet<String>) {
    match shape {
        JsonShape::Reference { schema, .. } => {
            names.insert(schema.clone());
        }
        JsonShape::Array(value) | JsonShape::Map(value) | JsonShape::Nullable(value) => {
            referenced_shapes(value, names)
        }
        JsonShape::MapWithConcreteValue { values, concrete } => {
            referenced_shapes(values, names);
            referenced_shapes(concrete, names);
        }
        JsonShape::Object(properties) | JsonShape::ObjectWithAdditional { properties, .. } => {
            for property in properties {
                referenced_shapes(&property.shape, names);
            }
            if let JsonShape::ObjectWithAdditional { values, .. } = shape {
                referenced_shapes(values, names);
            }
        }
        JsonShape::AnyOf(shapes) | JsonShape::AllOf(shapes) => {
            for shape in shapes {
                referenced_shapes(shape, names);
            }
        }
        _ => {}
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
    fn nullable_wrappers_preserve_authored_null_beside_opaque_fallbacks() {
        let ir = fixture();
        let inner = plan_named_shape("NullableOpenInner", &ir);
        let (concrete, fallback) = selection_shapes(inner.clone());
        assert!(shape_accepts_null(&concrete));
        assert!(accepts_any(&fallback));
        assert_eq!(
            selection_shapes(nullable(nullable(inner))),
            (concrete, fallback)
        );
        for name in ["NullableOpaque", "TwiceNullableOpaque"] {
            let (concrete, fallback) = selection_shapes(plan_named_shape(name, &ir));
            assert_eq!(concrete, JsonShape::Never);
            assert!(accepts_any(&fallback));
        }
    }

    #[test]
    fn nullable_reference_does_not_invent_concrete_null_evidence() {
        let ir = fixture();
        let reference = JsonShape::Reference {
            schema: "NullableMapFallback".into(),
            scope: MatchScope::Any,
            unrestricted: false,
            accepts_null: accepts_null(&IrTypeExpr::Named("NullableMapFallback".into()), &ir),
        };
        assert!(shape_accepts_null(&reference));
        assert_eq!(
            selection_shapes(nullable(reference.clone())),
            selection_shapes(reference)
        );
    }

    #[test]
    fn null_acceptance_intersects_every_scalar_enum_constraint() {
        let ir = fixture();
        let inner = IrTypeExpr::ScalarEnum {
            representation: Box::new(IrTypeExpr::Nullable(Box::new(IrTypeExpr::Primitive(
                IrPrimitive::Integer,
            )))),
            representation_is_constraint: true,
            representation_nullable: false,
            values: vec![1.into()],
        };
        let outer = IrTypeExpr::ScalarEnum {
            representation: Box::new(inner),
            representation_is_constraint: true,
            representation_nullable: false,
            values: vec![1.into(), serde_json::Value::Null],
        };
        assert!(!accepts_null(&outer, &ir));
        let (extracted, nullable) = outer.clone().split_nullable();
        assert!(nullable);
        assert!(!accepts_null(&extracted, &ir));
        assert!(accepts_null(&IrTypeExpr::Nullable(Box::new(outer)), &ir));
    }

    #[test]
    fn mixed_members_retain_required_wire_fields_and_literal_constraints() {
        let ir = fixture();
        let IrSchemaKind::Union(union) = &ir.schemas["Choice"].kind else {
            panic!("mixed choice remains untagged");
        };
        let members = plan_union_members("Choice", union, &ir);
        let JsonShape::Object(properties) = &members[0].shape else {
            panic!("payload A member is an object");
        };
        assert!(
            properties
                .iter()
                .any(|property| property.wire_name == "payload_schema" && property.required)
        );
        let JsonShape::Object(properties) = &members[1].shape else {
            panic!("payload B member is an object");
        };
        assert!(
            properties
                .iter()
                .any(|property| property.wire_name == "kind"
                    && property.shape == JsonShape::Literal("variant_b".into()))
        );
    }

    #[test]
    fn raw_json_fallback_does_not_shadow_a_concrete_member() {
        let ir = fixture();
        let union = IrUnion {
            members: vec![IrTypeExpr::Any, IrTypeExpr::Named("PayloadA".into())],
            nullable: false,
            authored_wire: None,
        };
        let members = plan_union_members("FallbackUnion", &union, &ir);
        assert!(!members[0].fallback);
        assert_eq!(members[0].source_index, 1);
        assert!(matches!(&members[0].type_expr, IrTypeExpr::Named(name) if name == "PayloadA"));
        assert!(members[1].fallback);
        assert_eq!(members[1].source_index, 0);
    }

    #[test]
    fn unrestricted_wrappers_do_not_precede_outer_concrete_members() {
        let ir = fixture();
        let IrSchemaKind::Union(union) = &ir.schemas["AliasOuter"].kind else {
            panic!("outer alias union");
        };
        let members = plan_union_members("AliasOuter", union, &ir);
        assert_eq!(members.len(), 3);
        assert!(matches!(&members[0].type_expr, IrTypeExpr::Named(name) if name == "OpenAlias"));
        assert!(!members[0].fallback);
        assert!(matches!(&members[1].type_expr, IrTypeExpr::Named(name) if name == "PayloadB"));
        assert!(!members[1].fallback);
        assert!(matches!(&members[2].type_expr, IrTypeExpr::Named(name) if name == "OpenAlias"));
        assert!(members[2].fallback);
        assert_eq!(members[2].shape, JsonShape::Any);
        let IrSchemaKind::Union(union) = &ir.schemas["NullableMapFallback"].kind else {
            panic!("nullable map union");
        };
        let members = plan_union_members("NullableMapFallback", union, &ir);
        assert!(!members[0].fallback);
        assert!(matches!(&members[0].type_expr, IrTypeExpr::Named(name) if name == "Detail"));
        assert!(members[1].fallback);
        assert_eq!(
            members[1].shape,
            JsonShape::AnyOf(vec![
                JsonShape::Null,
                JsonShape::Map(Box::new(JsonShape::Any))
            ])
        );
    }

    #[test]
    fn named_nested_unions_keep_both_delegation_paths() {
        let ir = fixture();
        let IrSchemaKind::Union(union) = &ir.schemas["Outer"].kind else {
            panic!("outer union")
        };
        let members = plan_union_members("Outer", union, &ir);
        assert_eq!(members.len(), 3);
        assert!(matches!(&members[0].type_expr, IrTypeExpr::Named(name) if name == "OpenInner"));
        assert!(
            matches!(&members[0].shape, JsonShape::Object(properties) if properties[0].wire_name == "wire_name")
        );
        assert!(!members[0].fallback);
        assert_eq!(members[1].shape, JsonShape::String);
        assert!(matches!(&members[2].type_expr, IrTypeExpr::Named(name) if name == "OpenInner"));
        assert!(members[2].fallback);
        assert_eq!(members[2].shape, JsonShape::Any);
    }

    #[test]
    fn open_map_values_retain_a_concrete_delegation_path() {
        let ir = fixture();
        let IrSchemaKind::Union(union) = &ir.schemas["MapOuter"].kind else {
            panic!("outer map union")
        };
        let members = plan_union_members("MapOuter", union, &ir);
        assert_eq!(members.len(), 3);
        assert!(matches!(&members[0].type_expr, IrTypeExpr::Named(name) if name == "OpenMap"));
        assert!(!members[0].fallback);
        assert!(
            matches!(&members[0].shape, JsonShape::MapWithConcreteValue { values, concrete }
            if accepts_any(values) && matches!(**concrete, JsonShape::Object(_)))
        );
        assert_eq!(members[1].shape, JsonShape::Any);
        assert!(members[1].fallback);
        assert_eq!(members[2].shape, JsonShape::Map(Box::new(JsonShape::Any)));
        assert!(members[2].fallback);
    }

    #[test]
    fn authored_dictionary_requires_an_entry_for_concrete_evidence() {
        let value = Box::new(JsonShape::String);
        let (concrete, fallback) = selection_shapes(JsonShape::ObjectWithAdditional {
            properties: vec![],
            values: value.clone(),
        });
        assert_eq!(
            concrete,
            JsonShape::MapWithConcreteValue {
                values: value.clone(),
                concrete: value,
            },
        );
        assert_eq!(fallback, JsonShape::Map(Box::new(JsonShape::Never)));

        let closed = JsonShape::ObjectWithAdditional {
            properties: vec![],
            values: Box::new(JsonShape::Never),
        };
        assert_eq!(selection_shapes(closed.clone()), (closed, JsonShape::Never));
    }

    #[test]
    fn inline_union_selection_can_delegate_to_its_enclosing_schema() {
        let ir = fixture();
        let members = plan_inline_union_members(&[IrTypeExpr::Named("Outer".into())], &ir);
        assert_eq!(members.len(), 2);
        assert!(
            members.iter().all(
                |member| matches!(&member.type_expr, IrTypeExpr::Named(name) if name == "Outer")
            )
        );
        assert!(!members[0].fallback);
        assert!(members[1].fallback);
    }

    #[test]
    fn binary_and_adjacent_unit_shapes_keep_their_semantics() {
        let ir = fixture();
        let binary = plan_shape(
            &IrTypeExpr::Named("BinaryModel".into()),
            &ir,
            &mut HashMap::new(),
            0,
        );
        let JsonShape::Object(properties) = binary else {
            panic!("binary model")
        };
        assert!(
            properties
                .iter()
                .any(|property| property.wire_name == "binary_data"
                    && property.shape == JsonShape::Binary)
        );
        let adjacent = plan_shape(
            &IrTypeExpr::Named("Adjacent".into()),
            &ir,
            &mut HashMap::new(),
            0,
        );
        let JsonShape::AnyOf(variants) = adjacent else {
            panic!("adjacent variants")
        };
        let JsonShape::Object(unit) = &variants[0] else {
            panic!("unit variant")
        };
        assert_eq!(unit.len(), 1);
        assert_eq!(unit[0].wire_name, "event_type");
        let JsonShape::Object(payload) = &variants[1] else {
            panic!("payload variant")
        };
        assert_eq!(payload.len(), 2);
        assert_eq!(payload[1].wire_name, "event_data");
        assert!(matches!(payload[1].shape, JsonShape::Object(_)));
    }

    #[test]
    fn intersection_partition_retains_constraints_on_fallback_paths() {
        let constrained = JsonShape::Object(vec![PropertyMatch {
            wire_name: "kind".into(),
            rename: true,
            required: true,
            shape: JsonShape::Literal("item".into()),
        }]);
        let (concrete, fallback) =
            selection_shapes(JsonShape::AllOf(vec![JsonShape::Any, constrained.clone()]));
        assert_eq!(concrete, constrained);
        assert_eq!(fallback, JsonShape::Never);
        let (_, fallback) = selection_shapes(JsonShape::AllOf(vec![
            JsonShape::Any,
            JsonShape::Map(Box::new(JsonShape::Any)),
        ]));
        assert_eq!(fallback, JsonShape::Map(Box::new(JsonShape::Any)));
    }

    #[test]
    fn recursive_alias_guard_expansion_terminates() {
        let mut ir = fixture();
        ir.schemas.get_mut("PayloadA").unwrap().kind =
            IrSchemaKind::Alias(IrTypeExpr::Named("PayloadA".into()));
        assert_eq!(
            plan_shape(
                &IrTypeExpr::Named("PayloadA".into()),
                &ir,
                &mut HashMap::new(),
                0
            ),
            JsonShape::Never
        );
    }

    #[test]
    fn same_value_alias_cycles_do_not_shadow_productive_members() {
        let mut ir = fixture();
        ir.schemas.get_mut("OpenAlias").unwrap().kind =
            IrSchemaKind::Alias(IrTypeExpr::Named("Outer".into()));
        let union = IrUnion {
            members: vec![
                IrTypeExpr::Named("OpenAlias".into()),
                IrTypeExpr::Named("Detail".into()),
            ],
            nullable: false,
            authored_wire: None,
        };
        let members = plan_union_members("Outer", &union, &ir);
        assert_eq!(members.len(), 1);
        assert!(matches!(&members[0].type_expr, IrTypeExpr::Named(name) if name == "Detail"));
    }

    #[test]
    fn inline_nested_union_retains_its_own_selection_and_conversion_identity() {
        let ir = fixture();
        let union = IrUnion {
            members: vec![IrTypeExpr::Union(vec![
                IrTypeExpr::Named("PayloadA".into()),
                IrTypeExpr::Named("PayloadB".into()),
            ])],
            nullable: false,
            authored_wire: None,
        };
        let members = plan_union_members("InlineUnion", &union, &ir);
        assert_eq!(members.len(), 1);
        assert!(matches!(&members[0].type_expr, IrTypeExpr::Union(_)));
        assert!(members.iter().all(|member| !member.fallback));
    }

    #[test]
    fn delegate_shapes_keep_identity_inside_named_containers() {
        let ir = fixture();
        let override_named = |name: &str| {
            (name == "PayloadA").then(|| JsonShape::Reference {
                schema: name.to_owned(),
                scope: MatchScope::Any,
                unrestricted: false,
                accepts_null: false,
            })
        };
        let members = [IrTypeExpr::Array(Box::new(IrTypeExpr::Named(
            "PayloadA".into(),
        )))];
        let matches = plan_members(
            &members,
            &ir,
            None,
            ShapePolicy {
                tag_projection: TagProjection::FixedRequired,
                override_named: Some(&override_named),
            },
        );
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].shape,
            JsonShape::Array(Box::new(override_named("PayloadA").unwrap()))
        );
    }

    #[test]
    fn external_variant_keys_are_not_ergonomic_property_names() {
        use crate::ir::types::{IrTaggedUnion, IrTaggedVariant};
        let mut ir = fixture();
        ir.schemas.get_mut("PayloadA").unwrap().kind = IrSchemaKind::TaggedUnion(IrTaggedUnion {
            discriminator_field: String::new(),
            tagging: TaggingStyle::External,
            inferred_wire: None,
            variants: vec![IrTaggedVariant {
                discriminator_value: "snake_tag".into(),
                discriminator_values: Vec::new(),
                discriminator_optional: false,
                content_type: IrTypeExpr::Named("PayloadB".into()),
                description: None,
                additional_properties: None,
            }],
        });
        let shape = plan_shape(
            &IrTypeExpr::Named("PayloadA".into()),
            &ir,
            &mut HashMap::new(),
            0,
        );
        let JsonShape::Object(properties) = shape else {
            panic!("external wrapper")
        };
        assert_eq!(properties[0].wire_name, "snake_tag");
        assert!(!properties[0].rename);
    }
}
