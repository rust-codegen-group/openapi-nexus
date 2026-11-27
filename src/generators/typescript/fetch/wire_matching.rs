//! Project a shared authored guard into TypeScript's static matching helpers.
//! Only property access changes with the naming mode; wire facts do not.

use std::collections::{HashMap, HashSet};

use heck::ToLowerCamelCase;
use serde_json::Value;

use crate::generators::union_match::{JsonShape, MatchScope, PropertyMatch};
use crate::ir::types::{
    IrJsonKind, IrSchemaKind, IrSpec, IrTypeExpr, IrUnion, IrWireShape, TaggingStyle,
};

use super::plan::Direction;
use super::tagged_fields::TaggedFields;

/// Distinguish these functions from older model-derived recursive predicates.
const WIRE_PREFIX: &str = "$Wire";

pub(super) fn reference(name: &str, wire_keys: bool) -> JsonShape {
    let direction = if wire_keys { "$Preserve" } else { "$Model" };
    JsonShape::Reference {
        schema: format!("{WIRE_PREFIX}{direction}${name}"),
        scope: MatchScope::Any,
        unrestricted: false,
        accepts_null: false,
    }
}

pub(super) fn reference_target(name: &str) -> Option<(&str, bool)> {
    let (projection, target) = name
        .strip_prefix(WIRE_PREFIX)?
        .strip_prefix('$')?
        .split_once('$')?;
    Some((target, projection == "Preserve"))
}

type SelectLiteralMember<'a> = dyn Fn(&str, &IrUnion, &Value) -> Option<usize> + 'a;

pub(super) struct Projection<'a, 'p> {
    ir: &'a IrSpec,
    visiting: HashMap<&'a str, usize>,
    wire_keys: bool,
    project_literals: bool,
    select_literal_member: &'p SelectLiteralMember<'p>,
}

impl<'a, 'p> Projection<'a, 'p> {
    pub(super) fn new(
        ir: &'a IrSpec,
        wire_keys: bool,
        direction: Direction,
        select_literal_member: &'p SelectLiteralMember<'p>,
    ) -> Self {
        Self {
            ir,
            visiting: HashMap::new(),
            wire_keys,
            project_literals: !wire_keys && direction == Direction::ToWire,
            select_literal_member,
        }
    }

    pub(super) fn root(
        &mut self,
        shape: &'a IrWireShape,
        fields: &TaggedFields,
        payload: &'a IrTypeExpr,
    ) -> JsonShape {
        self.at(
            shape,
            0,
            self.wire_keys,
            Some(fields),
            self.project_literals.then_some(Codec::Expr(payload)),
        )
    }

    pub(super) fn named(&mut self, name: &str) -> JsonShape {
        let Some((name, shape)) = self.ir.wire_schemas.get_key_value(name) else {
            return JsonShape::Never;
        };
        let codec = self.project_literals.then_some(Codec::Named(name));
        self.visiting.insert(name, 0);
        let shape = self.at(shape, 0, self.wire_keys, None, codec);
        self.visiting.remove(name.as_str());
        shape
    }

    fn at(
        &mut self,
        shape: &'a IrWireShape,
        depth: usize,
        wire_keys: bool,
        root: Option<&TaggedFields>,
        codec: Option<Codec<'a>>,
    ) -> JsonShape {
        match shape {
            IrWireShape::Any => JsonShape::Any,
            IrWireShape::Never => JsonShape::Never,
            IrWireShape::Kind(kind) => match kind {
                IrJsonKind::Null => JsonShape::Null,
                IrJsonKind::Boolean => JsonShape::Boolean,
                IrJsonKind::Integer => JsonShape::Number { integer: true },
                IrJsonKind::Number => JsonShape::Number { integer: false },
                IrJsonKind::String => JsonShape::String,
                IrJsonKind::Array => JsonShape::Array(Box::new(JsonShape::Any)),
                IrJsonKind::Object => JsonShape::Object(Vec::new()),
            },
            IrWireShape::Finite(values) => or(values
                .iter()
                .map(|value| {
                    let value = if self.project_literals && !wire_keys {
                        model_literal(value, codec, root, self.ir, self.select_literal_member)
                    } else {
                        value.clone()
                    };
                    JsonShape::Literal(value)
                })
                .collect()),
            IrWireShape::Object(object) => {
                let properties = object
                    .properties
                    .keys()
                    .chain(
                        object
                            .required
                            .iter()
                            .filter(|name| !object.properties.contains_key(*name)),
                    )
                    .map(|name| PropertyMatch {
                        required: object.required.contains(name),
                        rename: !wire_keys
                            && (object.properties.contains_key(name)
                                || root.is_some_and(|fields| fields.renames(name))),
                        wire_name: name.clone(),
                        // A required-but-undeclared key remains an extension in
                        // this conjunct, so it must satisfy the local value shape.
                        shape: self.at(
                            object
                                .properties
                                .get(name)
                                .unwrap_or(&object.additional_properties),
                            depth + 1,
                            wire_keys
                                || root.is_some_and(|fields| fields.envelope.contains_key(name)),
                            None,
                            codec
                                .and_then(|codec| codec.property(name, self.ir))
                                .and_then(|(_, codec)| codec),
                        ),
                    })
                    .collect();
                if matches!(*object.additional_properties, IrWireShape::Any) {
                    JsonShape::Object(properties)
                } else {
                    let values = Box::new(
                        self.at(
                            &object.additional_properties,
                            depth + 1,
                            wire_keys,
                            None,
                            codec
                                .and_then(|codec| codec.descend(CodecEdge::Entries, self.ir))
                                .and_then(|(_, codec)| codec),
                        ),
                    );
                    JsonShape::ObjectWithAdditional { properties, values }
                }
            }
            IrWireShape::Array(item) => JsonShape::Array(Box::new(
                self.at(
                    item,
                    depth + 1,
                    wire_keys,
                    None,
                    codec
                        .and_then(|codec| codec.descend(CodecEdge::Items, self.ir))
                        .and_then(|(_, codec)| codec),
                ),
            )),
            IrWireShape::AllOf(parts) => and(parts
                .iter()
                .map(|part| self.at(part, depth, wire_keys, root, codec))
                .collect()),
            IrWireShape::OneOf(parts) | IrWireShape::AnyOf(parts) => or(parts
                .iter()
                .map(|part| self.at(part, depth, wire_keys, root, codec))
                .collect()),
            IrWireShape::Reference(name) => {
                if let Some(ancestor) = self.visiting.get(name.as_str()) {
                    return if depth > *ancestor {
                        reference(name, wire_keys)
                    } else {
                        JsonShape::Never
                    };
                }
                let Some(target) = self.ir.wire_schemas.get(name.as_str()) else {
                    return JsonShape::Never;
                };
                self.visiting.insert(name, depth);
                let shape = self.at(target, depth, wire_keys, root, codec);
                self.visiting.remove(name.as_str());
                shape
            }
        }
    }
}

/// A literal is already a wire value. Only rename fields the native payload
/// codec actually projects. Opaque JSON and dictionary keys are not schema
/// property names, even if they look like camel-case candidates.
fn model_literal<'a>(
    value: &Value,
    codec: Option<Codec<'a>>,
    root: Option<&TaggedFields>,
    ir: &'a IrSpec,
    select: &SelectLiteralMember<'_>,
) -> Value {
    let codec = codec.and_then(|codec| codec.for_literal(value, ir, select, &mut HashSet::new()));
    match value {
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(wire, value)| {
                    let (rename, child) =
                        if root.is_some_and(|fields| fields.envelope.contains_key(wire)) {
                            (false, None)
                        } else {
                            codec
                                .and_then(|codec| codec.property(wire, ir))
                                .unwrap_or((false, None))
                        };
                    let model =
                        if rename || root.is_some_and(|fields| fields.renames_envelope(wire)) {
                            wire.to_lower_camel_case()
                        } else {
                            wire.clone()
                        };
                    (model, model_literal(value, child, None, ir, select))
                })
                .collect(),
        ),
        Value::Array(items) => {
            let child = codec
                .and_then(|codec| codec.descend(CodecEdge::Items, ir))
                .and_then(|(_, codec)| codec);
            Value::Array(
                items
                    .iter()
                    .map(|item| model_literal(item, child, None, ir, select))
                    .collect(),
            )
        }
        _ => value.clone(),
    }
}

#[derive(Clone, Copy)]
enum Codec<'a> {
    Named(&'a str),
    Expr(&'a IrTypeExpr),
    Schema(&'a IrSchemaKind),
}

#[derive(Clone, Copy)]
enum CodecEdge<'a> {
    Property(&'a str),
    Entries,
    Items,
}

// The boolean belongs to this edge only: a dictionary does not rename a key,
// but its element can still have a native codec that renames declared fields.
type CodecMember<'a> = (bool, Option<Codec<'a>>);

impl<'a> Codec<'a> {
    fn for_literal(
        self,
        value: &Value,
        ir: &'a IrSpec,
        select: &SelectLiteralMember<'_>,
        seen: &mut HashSet<&'a str>,
    ) -> Option<Self> {
        match self {
            Self::Expr(expr) => match expr.representation() {
                IrTypeExpr::Named(name) => Self::Named(name).for_literal(value, ir, select, seen),
                IrTypeExpr::Nullable(inner) => {
                    Self::Expr(inner).for_literal(value, ir, select, seen)
                }
                _ => Some(self),
            },
            Self::Named(name) if seen.insert(name) => {
                let result = ir.schemas.get(name).and_then(|schema| match &schema.kind {
                    IrSchemaKind::Alias(expr) => {
                        Self::Expr(expr).for_literal(value, ir, select, seen)
                    }
                    IrSchemaKind::Union(union) => {
                        let selected = select(name, union, value)?;
                        Self::Expr(union.members.get(selected)?)
                            .for_literal(value, ir, select, seen)
                    }
                    kind => Some(Self::Schema(kind)),
                });
                seen.remove(name);
                result
            }
            Self::Named(_) => None,
            Self::Schema(_) => Some(self),
        }
    }

    fn property(self, name: &str, ir: &'a IrSpec) -> Option<CodecMember<'a>> {
        self.descend(CodecEdge::Property(name), ir)
    }

    fn descend(self, edge: CodecEdge<'_>, ir: &'a IrSpec) -> Option<CodecMember<'a>> {
        self.descend_at(edge, ir, &mut HashSet::new())
    }

    fn descend_at(
        self,
        edge: CodecEdge<'_>,
        ir: &'a IrSpec,
        seen: &mut HashSet<&'a str>,
    ) -> Option<CodecMember<'a>> {
        let declared = |ty: &'a IrTypeExpr| (true, Some(Self::Expr(ty)));
        let element = |ty: &'a IrTypeExpr| (false, Some(Self::Expr(ty)));
        match self {
            Self::Named(name) if seen.insert(name) => {
                let codec = ir
                    .schemas
                    .get(name)
                    .and_then(|schema| Self::Schema(&schema.kind).descend_at(edge, ir, seen));
                seen.remove(name);
                codec
            }
            Self::Named(_) => None,
            Self::Expr(expr) => match (expr.representation(), edge) {
                (IrTypeExpr::Named(name), _) => Self::Named(name).descend_at(edge, ir, seen),
                (IrTypeExpr::Nullable(inner), _) => Self::Expr(inner).descend_at(edge, ir, seen),
                (IrTypeExpr::Array(inner), CodecEdge::Items) => Some(element(inner)),
                (IrTypeExpr::Map(inner), CodecEdge::Property(_) | CodecEdge::Entries) => {
                    Some(element(inner))
                }
                (IrTypeExpr::Union(members), _) => first_declared(
                    members
                        .iter()
                        .filter_map(|member| Self::Expr(member).descend_at(edge, ir, seen)),
                ),
                _ => None,
            },
            Self::Schema(kind) => match kind {
                IrSchemaKind::Object(object) => match edge {
                    CodecEdge::Property(name) => object
                        .properties
                        .get(name)
                        .map(|prop| declared(&prop.type_expr))
                        .or_else(|| object.additional_properties.as_ref().map(element)),
                    CodecEdge::Entries => object.additional_properties.as_ref().map(element),
                    CodecEdge::Items => None,
                },
                IrSchemaKind::Alias(expr) => Self::Expr(expr).descend_at(edge, ir, seen),
                IrSchemaKind::Intersection(all) => first_declared(
                    all.members
                        .iter()
                        .filter_map(|member| Self::Expr(member).descend_at(edge, ir, seen)),
                ),
                // Knowing a field inside a named union does not prove which
                // member the native codec selected. Use `for_literal` when an
                // exact enclosing constant is available.
                IrSchemaKind::Union(_) => None,
                IrSchemaKind::TaggedUnion(union) => match (&union.tagging, edge) {
                    (
                        TaggingStyle::Internal | TaggingStyle::Adjacent { .. },
                        CodecEdge::Property(key),
                    ) if key == union.discriminator_field => Some((true, None)),
                    (TaggingStyle::Internal, _) => {
                        first_declared(union.variants.iter().filter_map(|variant| {
                            Self::Expr(&variant.content_type).descend_at(edge, ir, seen)
                        }))
                    }
                    (TaggingStyle::Adjacent { content_field }, CodecEdge::Property(key))
                        if key == content_field =>
                    {
                        union
                            .variants
                            .first()
                            .map(|variant| declared(&variant.content_type))
                    }
                    (TaggingStyle::External, CodecEdge::Property(key)) => union
                        .variants
                        .iter()
                        .find(|variant| variant.discriminator_value == key)
                        .map(|variant| element(&variant.content_type)),
                    _ => None,
                },
                _ => None,
            },
        }
    }
}

fn first_declared<'a>(members: impl Iterator<Item = CodecMember<'a>>) -> Option<CodecMember<'a>> {
    let mut first = None;
    for member in members {
        if member.0 {
            return Some(member);
        }
        first.get_or_insert(member);
    }
    first
}

pub(super) fn and(mut parts: Vec<JsonShape>) -> JsonShape {
    if parts.contains(&JsonShape::Never) {
        return JsonShape::Never;
    }
    parts.retain(|shape| *shape != JsonShape::Any);
    match parts.len() {
        0 => JsonShape::Any,
        1 => parts.pop().unwrap(),
        _ => JsonShape::AllOf(parts),
    }
}

pub(super) fn or(mut parts: Vec<JsonShape>) -> JsonShape {
    parts.retain(|shape| *shape != JsonShape::Never);
    match parts.len() {
        0 => JsonShape::Never,
        1 => parts.pop().unwrap(),
        _ => JsonShape::AnyOf(parts),
    }
}

pub(super) fn matcher_name(name: &str, direction: super::plan::Direction) -> String {
    use heck::ToPascalCase;
    if let Some((target, wire_keys)) = reference_target(name) {
        let model = target.to_pascal_case();
        let projection = if wire_keys { "Preserve" } else { "Model" };
        return format!("match$Wire{projection}${model}{}", direction.suffix());
    }
    format!("match{}{}", name.to_pascal_case(), direction.suffix())
}
