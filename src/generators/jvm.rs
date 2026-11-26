//! JVM representation and structural predicates for authored internal tags.

use std::collections::{BTreeSet, HashSet};

use serde::{Deserialize, Serialize};
use sigil_stitch::prelude::TypeName;

use super::closed_sum::{ClosedSumCasePlan, ClosedSumPlan};
use super::union_match::{
    JsonShape, MatchScope, PropertyMatch, ShapePolicy, TagProjection, selection_shapes,
};
use super::union_wire::{Strategy, TagOwner, TagRead, UnionWirePlan, UnionWirePlans, WireBinding};
use crate::ir::types::{
    IrIntersection, IrJsonKind, IrSchemaKind, IrSpec, IrTypeExpr, IrWireShape, TaggingStyle,
};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TaggedUnionStrategy {
    #[default]
    FirstMatch,
    Strict,
    Raw,
}

impl From<TaggedUnionStrategy> for Strategy {
    fn from(value: TaggedUnionStrategy) -> Self {
        match value {
            TaggedUnionStrategy::FirstMatch => Self::FirstMatch,
            TaggedUnionStrategy::Strict => Self::Strict,
            TaggedUnionStrategy::Raw => Self::Raw,
        }
    }
}

pub(super) struct TaggedWirePlans<'ir> {
    plans: UnionWirePlans<'ir>,
    ir: &'ir IrSpec,
    pub strategy: Strategy,
}

impl<'ir> TaggedWirePlans<'ir> {
    pub fn build(ir: &'ir IrSpec, strategy: TaggedUnionStrategy) -> Self {
        let strategy = strategy.into();
        Self {
            plans: UnionWirePlans::build(ir, strategy),
            ir,
            strategy,
        }
    }

    fn get(&self, name: &str) -> Option<&UnionWirePlan<'ir>> {
        let IrSchemaKind::TaggedUnion(union) = &self.ir.schemas.get(name)?.kind else {
            return None;
        };
        let plan = self.plans.get(name)?;
        (matches!(union.tagging, TaggingStyle::Internal) && !plan.has_distinct_fixed_tags())
            .then_some(plan)
    }

    pub fn uses_authored_codec(&self, expr: &IrTypeExpr) -> bool {
        match expr.representation() {
            IrTypeExpr::Named(name) => self.get(name).is_some(),
            IrTypeExpr::Array(inner) | IrTypeExpr::Map(inner) | IrTypeExpr::Nullable(inner) => {
                self.uses_authored_codec(inner)
            }
            _ => false,
        }
    }

    pub fn named_override(&self, name: &str) -> Option<JsonShape> {
        self.get(name)?;
        Some(if self.strategy == Strategy::Raw {
            JsonShape::Any
        } else {
            authored_reference(name, true)
        })
    }

    pub fn legacy_policy<'a>(
        &self,
        override_named: &'a impl Fn(&str) -> Option<JsonShape>,
    ) -> ShapePolicy<'a> {
        ShapePolicy {
            tag_projection: TagProjection::FixedRequired,
            override_named: Some(override_named),
        }
    }

    /// Names, payload types and authored branch indices still refer to the
    /// native closed sum. Only case-local projection and ownership may differ.
    pub fn tagged(
        &self,
        name: &str,
        sum: &mut ClosedSumPlan,
        is_composed: impl Fn(&IrIntersection) -> bool,
        kotlin_aliases: bool,
    ) -> Option<TaggedWire<'ir>> {
        let plan = self.get(name)?;
        if self.strategy == Strategy::Raw {
            sum.cases = vec![ClosedSumCasePlan {
                name: if sum.name == "VariantRaw" {
                    "VariantRawVariant"
                } else {
                    "VariantRaw"
                }
                .into(),
                payload: Some(IrTypeExpr::Any),
                doc: None,
                wire_name: None,
                is_null: false,
            }];
            return Some(TaggedWire::Raw);
        }
        let common = plan
            .common_shape()
            .map(project)
            .filter(|shape| *shape != JsonShape::Any);
        let common_needs_projection = common
            .as_ref()
            .is_some_and(|shape| *shape != JsonShape::Object(Vec::new()));
        let mut cases = Vec::new();
        for (index, (case, branch)) in sum.cases.iter_mut().zip(plan.branches()).enumerate() {
            let WireBinding::Internal { field, owner, read } =
                plan.binding(index).expect("inferred internal binding")
            else {
                unreachable!("inferred internal binding")
            };
            let domain = plan
                .tag_domain(index)
                .expect("inferred discriminator domain");
            let non_null = domain
                .values
                .as_ref()
                .is_some_and(|values| values.iter().all(|v| !v.is_null()));
            let mut required_nullable = BTreeSet::new();
            let mut nested_owner = false;
            let context = PayloadContext {
                plans: self,
                field,
                is_composed: &is_composed,
                kotlin_aliases,
            };
            let native_flat = case.payload.as_ref().is_some_and(|expr| {
                context.flat(
                    expr,
                    &mut HashSet::new(),
                    &mut required_nullable,
                    &mut nested_owner,
                )
            });
            let project_payload = common_needs_projection
                || owner == TagOwner::CaseWithPayloadProjection
                || !native_flat
                || case.payload.as_ref().is_some_and(|expr| {
                    context.erases_nested_required_null(expr, false, &mut HashSet::new())
                });
            let projected_reader = if project_payload && native_flat {
                // Preserve the wire subtree as storage, but do not bypass the
                // existing payload codec or the policies of its children.
                case.payload.clone()
            } else {
                None
            };
            if project_payload {
                case.payload = Some(IrTypeExpr::Any);
                required_nullable.clear();
            }
            required_nullable.remove(field);
            let payload_owns_tag = !project_payload
                && owner == TagOwner::Payload
                && read == TagRead::Keep
                && (non_null || nested_owner);
            cases.push(TaggedCase {
                shape: project(branch),
                case_owns_tag: !payload_owns_tag,
                projected: project_payload,
                projected_reader,
                read,
                projects_object: project_payload
                    && [plan.common_shape(), Some(branch)]
                        .into_iter()
                        .flatten()
                        .any(|shape| objects_only(shape, self.ir, &mut HashSet::new())),
                // A projected tree replaces the original reader. A typed codec
                // that consumes this tag always receives the full object.
                strip: project_payload || read == TagRead::Strip,
                required_nullable,
            });
        }
        Some(TaggedWire::Typed(TypedWire {
            field: plan.tag_domain(0).expect("tagged union has branches").field,
            common,
            cases,
            strict: self.strategy == Strategy::Strict,
        }))
    }
}

struct PayloadContext<'a, 'ir, F> {
    plans: &'a TaggedWirePlans<'ir>,
    field: &'ir str,
    is_composed: &'a F,
    kotlin_aliases: bool,
}

impl<F: Fn(&IrIntersection) -> bool> PayloadContext<'_, '_, F> {
    /// Gson's ordinary reflective writer omits nulls. Root required fields
    /// can be restored unambiguously; inside a native value graph, retain
    /// the authored case's tree rather than changing reusable member codecs.
    fn erases_nested_required_null<'a>(
        &'a self,
        expr: &'a IrTypeExpr,
        nested: bool,
        visiting: &mut HashSet<(&'a str, bool)>,
    ) -> bool {
        match expr.representation() {
            IrTypeExpr::Named(name) if visiting.insert((name, nested)) => {
                let erases = match self.plans.ir.schemas.get(name).map(|schema| &schema.kind) {
                    Some(IrSchemaKind::Object(object)) => {
                        object.properties.values().any(|property| {
                            (nested && property.required && property.nullable)
                                || self.erases_nested_required_null(
                                    &property.type_expr,
                                    true,
                                    visiting,
                                )
                        }) || object.additional_properties.as_ref().is_some_and(|expr| {
                            self.erases_nested_required_null(expr, true, visiting)
                        })
                    }
                    Some(IrSchemaKind::Intersection(intersection))
                        if (self.is_composed)(intersection) =>
                    {
                        intersection.members.iter().any(|member| {
                            self.erases_nested_required_null(member, nested, visiting)
                        })
                    }
                    Some(IrSchemaKind::Alias(expr)) => {
                        self.erases_nested_required_null(expr, nested, visiting)
                    }
                    Some(IrSchemaKind::Union(union)) => union
                        .members
                        .iter()
                        .any(|member| self.erases_nested_required_null(member, nested, visiting)),
                    _ => false,
                };
                visiting.remove(&(name, nested));
                erases
            }
            IrTypeExpr::Array(inner) | IrTypeExpr::Map(inner) => {
                self.erases_nested_required_null(inner, true, visiting)
            }
            IrTypeExpr::Nullable(inner) => {
                self.erases_nested_required_null(inner, nested, visiting)
            }
            IrTypeExpr::Union(parts) => parts
                .iter()
                .any(|part| self.erases_nested_required_null(part, nested, visiting)),
            _ => false,
        }
    }

    fn flat(
        &self,
        expr: &IrTypeExpr,
        visiting: &mut HashSet<String>,
        required_nullable: &mut BTreeSet<String>,
        nested_owner: &mut bool,
    ) -> bool {
        match expr.representation() {
            IrTypeExpr::Named(name) if visiting.insert(name.clone()) => {
                let result = match self.plans.ir.schemas.get(name).map(|schema| &schema.kind) {
                    Some(IrSchemaKind::Object(object)) => {
                        required_nullable.extend(
                            object
                                .properties
                                .iter()
                                .filter(|(_, property)| property.required && property.nullable)
                                .map(|(key, _)| key.clone()),
                        );
                        true
                    }
                    Some(IrSchemaKind::Intersection(intersection))
                        if (self.is_composed)(intersection) =>
                    {
                        intersection
                            .members
                            .iter()
                            .all(|part| self.flat(part, visiting, required_nullable, nested_owner))
                    }
                    Some(IrSchemaKind::Alias(expr)) if self.kotlin_aliases => {
                        self.flat(expr, visiting, required_nullable, nested_owner)
                    }
                    Some(IrSchemaKind::TaggedUnion(union))
                        if union.discriminator_field == self.field
                            && self.plans.get(name).is_some() =>
                    {
                        *nested_owner = true;
                        true
                    }
                    _ => false,
                };
                visiting.remove(name);
                result
            }
            IrTypeExpr::Nullable(inner) => {
                self.flat(inner, visiting, required_nullable, nested_owner)
            }
            IrTypeExpr::Map(_) => true,
            _ => false,
        }
    }
}

pub(super) enum TaggedWire<'ir> {
    Typed(TypedWire<'ir>),
    Raw,
}

pub(super) struct TypedWire<'ir> {
    pub field: &'ir str,
    pub common: Option<JsonShape>,
    pub cases: Vec<TaggedCase>,
    pub strict: bool,
}

impl TypedWire<'_> {
    pub fn selection(&self) -> Vec<(usize, JsonShape, bool)> {
        let mut checks = Vec::new();
        for (index, case) in self.cases.iter().enumerate() {
            let (concrete, fallback) = selection_shapes(case.shape.clone());
            for (shape, opaque) in [(concrete, false), (fallback, true)] {
                if shape != JsonShape::Never {
                    checks.push((index, shape, opaque));
                }
            }
        }
        checks.sort_by_key(|(_, _, opaque)| *opaque);
        checks
    }
}

pub(super) struct TaggedCase {
    pub shape: JsonShape,
    pub case_owns_tag: bool,
    pub projected: bool,
    pub projected_reader: Option<IrTypeExpr>,
    pub read: TagRead,
    pub projects_object: bool,
    pub strip: bool,
    pub required_nullable: BTreeSet<String>,
}

pub(super) fn discriminator_type() -> TypeName {
    TypeName::importable("com.google.gson", "JsonElement")
}

impl TaggedCase {
    pub fn projected_type(&self) -> Option<TypeName> {
        self.projected.then(|| {
            if self.projects_object {
                TypeName::importable("com.google.gson", "JsonObject")
            } else {
                discriminator_type()
            }
        })
    }
}

fn objects_only<'a>(
    shape: &'a IrWireShape,
    ir: &'a IrSpec,
    visiting: &mut HashSet<&'a str>,
) -> bool {
    match shape {
        IrWireShape::Kind(IrJsonKind::Object) | IrWireShape::Object(_) | IrWireShape::Never => true,
        IrWireShape::Finite(values) => values.iter().all(serde_json::Value::is_object),
        IrWireShape::AllOf(parts) => parts.iter().any(|part| objects_only(part, ir, visiting)),
        IrWireShape::OneOf(parts) | IrWireShape::AnyOf(parts) => {
            parts.iter().all(|part| objects_only(part, ir, visiting))
        }
        IrWireShape::Reference(name) if visiting.insert(name) => {
            let result = ir
                .wire_schemas
                .get(name)
                .is_some_and(|shape| objects_only(shape, ir, visiting));
            visiting.remove(name.as_str());
            result
        }
        _ => false,
    }
}

// Internal name space only: no authored reference keys are emitted as symbols.
fn authored_reference(name: &str, adopted: bool) -> JsonShape {
    JsonShape::Reference {
        schema: if adopted {
            format!("\0\0{name}")
        } else {
            format!("\0{name}")
        },
        scope: MatchScope::Any,
        unrestricted: false,
        accepts_null: false,
    }
}

/// For fallback scopes, the first shape is the excluded concrete predicate.
/// Preserve the split when an adopted named codec has common constraints: a
/// common gate permits a branch, but never supplies its selection evidence.
pub(super) fn named_predicates(
    name: &str,
    ir: &IrSpec,
    scope: MatchScope,
    legacy: impl FnOnce() -> JsonShape,
) -> (JsonShape, Option<JsonShape>, bool) {
    let wire_name = name.strip_prefix('\0');
    let authored = wire_name.is_some();
    let common = wire_name
        .and_then(|wire| wire.strip_prefix('\0'))
        .and_then(|key| match &ir.schemas.get(key)?.kind {
            IrSchemaKind::TaggedUnion(union) => union.inferred_wire.as_ref(),
            _ => None,
        });
    let shape = if let Some(union) = common {
        JsonShape::AnyOf(union.branches.iter().map(project).collect())
    } else if let Some(key) = wire_name {
        ir.wire_schemas.get(key).map_or(JsonShape::Never, project)
    } else {
        legacy()
    };
    let gate = |shape| match common {
        Some(union) if shape != JsonShape::Never && union.common != IrWireShape::Any => {
            JsonShape::AllOf(vec![project(&union.common), shape])
        }
        _ => shape,
    };
    if scope == MatchScope::Any {
        (gate(shape), None, authored)
    } else {
        let (concrete, fallback) = selection_shapes(shape);
        (
            gate(concrete),
            (scope == MatchScope::Fallback).then(|| gate(fallback)),
            authored,
        )
    }
}

fn project(shape: &IrWireShape) -> JsonShape {
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
        IrWireShape::Finite(values) => {
            JsonShape::AnyOf(values.iter().cloned().map(JsonShape::Literal).collect())
        }
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
                    wire_name: name.clone(),
                    required: object.required.contains(name),
                    rename: false,
                    // An undeclared required key is subject to the local additional shape.
                    shape: project(
                        object
                            .properties
                            .get(name)
                            .unwrap_or(&object.additional_properties),
                    ),
                })
                .collect();
            if matches!(*object.additional_properties, IrWireShape::Any) {
                JsonShape::Object(properties)
            } else {
                JsonShape::ObjectWithAdditional {
                    properties,
                    values: Box::new(project(&object.additional_properties)),
                }
            }
        }
        IrWireShape::Array(items) => JsonShape::Array(Box::new(project(items))),
        IrWireShape::AllOf(parts) => JsonShape::AllOf(parts.iter().map(project).collect()),
        IrWireShape::AnyOf(parts) | IrWireShape::OneOf(parts) => {
            JsonShape::AnyOf(parts.iter().map(project).collect())
        }
        IrWireShape::Reference(name) => authored_reference(name, false),
    }
}
