//! Object views for compatible intersections with overlapping nested fields.
//!
//! The shared IR keeps authored intersections. Only this generator's view is
//! projected, so ordinary object converters can convert each nested field once
//! instead of shallow-spreading partially converted copies over one another.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use heck::ToPascalCase;
use indexmap::IndexMap;

use crate::ir::types::{IrObject, IrPrimitive, IrSchema, IrSchemaKind, IrSpec, IrTypeExpr};

#[derive(Clone)]
struct ObjectView {
    object: IrObject,
    nested_overlap: bool,
}

#[cfg(test)]
mod tests;

struct Projection {
    schemas: IndexMap<String, IrSchema>,
    used_names: HashSet<String>,
    objects: HashMap<String, Option<ObjectView>>,
    resolving: HashSet<String>,
    pairs: Vec<(IrTypeExpr, IrTypeExpr, String)>,
    values: Vec<(IrTypeExpr, IrTypeExpr, usize)>,
    created: Vec<String>,
}

pub(super) fn project(ir: &IrSpec) -> Cow<'_, IrSpec> {
    let mut projection = Projection {
        schemas: ir.schemas.clone(),
        used_names: ir
            .schemas
            .keys()
            .map(|name| name.to_pascal_case())
            .collect(),
        objects: HashMap::new(),
        resolving: HashSet::new(),
        pairs: Vec::new(),
        values: Vec::new(),
        created: Vec::new(),
    };
    let mut changed = false;
    for (name, schema) in &ir.schemas {
        if !matches!(schema.kind, IrSchemaKind::Intersection(_)) {
            continue;
        }
        projection.objects.clear();
        projection.resolving.clear();
        let checkpoint = (projection.created.len(), projection.pairs.len());
        let view = projection.object(&IrTypeExpr::Named(name.clone()), name);
        if let Some(view) = view.filter(|view| view.nested_overlap) {
            projection
                .schemas
                .get_mut(name)
                .expect("authored intersection")
                .kind = IrSchemaKind::Object(view.object);
            changed = true;
        } else {
            for name in projection.created.drain(checkpoint.0..) {
                projection.schemas.shift_remove(&name);
                projection.used_names.remove(&name);
            }
            projection.pairs.truncate(checkpoint.1);
        }
    }
    if !changed {
        return Cow::Borrowed(ir);
    }
    let mut projected = ir.clone();
    projected.schemas = projection.schemas;
    Cow::Owned(projected)
}

impl Projection {
    fn object(&mut self, expr: &IrTypeExpr, hint: &str) -> Option<ObjectView> {
        match expr {
            IrTypeExpr::Named(name) => {
                if let Some(cached) = self.objects.get(name) {
                    return cached.clone();
                }
                if !self.resolving.insert(name.clone()) {
                    return None;
                }
                let kind = self.schemas.get(name)?.kind.clone();
                let view = match kind {
                    IrSchemaKind::Object(object) => Some(ObjectView {
                        object,
                        nested_overlap: false,
                    }),
                    IrSchemaKind::Alias(inner) => self.object(&inner, hint),
                    IrSchemaKind::Intersection(intersection) => {
                        let mut view = ObjectView {
                            object: IrObject {
                                properties: IndexMap::new(),
                                additional_properties: None,
                            },
                            nested_overlap: false,
                        };
                        let mut compatible = true;
                        for member in &intersection.members {
                            if let Some(next) = self.object(member, hint)
                                && let Some(merged) = self.merge_objects(view.clone(), next, hint)
                            {
                                view = merged;
                            } else {
                                compatible = false;
                                break;
                            }
                        }
                        compatible.then_some(view)
                    }
                    _ => None,
                };
                self.resolving.remove(name);
                self.objects.insert(name.clone(), view.clone());
                view
            }
            IrTypeExpr::Map(value) => Some(ObjectView {
                object: IrObject {
                    properties: IndexMap::new(),
                    additional_properties: Some(value.as_ref().clone()),
                },
                nested_overlap: false,
            }),
            _ => None,
        }
    }

    fn merge_objects(
        &mut self,
        mut left: ObjectView,
        right: ObjectView,
        hint: &str,
    ) -> Option<ObjectView> {
        left.nested_overlap |= right.nested_overlap;
        for (name, property) in right.object.properties {
            if let Some(existing) = left.object.properties.get_mut(&name) {
                let (value, nested) = self.value(
                    &existing.type_expr,
                    &property.type_expr,
                    &format!("{hint}{}", name.to_pascal_case()),
                )?;
                existing.type_expr = value;
                existing.required |= property.required;
                existing.nullable &= property.nullable;
                if existing.description.is_none() {
                    existing.description = property.description;
                }
                left.nested_overlap |= nested;
            } else {
                left.object.properties.insert(name, property);
            }
        }
        if let Some(right) = right.object.additional_properties {
            if let Some(existing) = &left.object.additional_properties {
                let (value, nested) = self.value(existing, &right, &format!("{hint}Value"))?;
                left.object.additional_properties = Some(value);
                left.nested_overlap |= nested;
            } else {
                left.object.additional_properties = Some(right);
            }
        }
        Some(left)
    }

    fn value(
        &mut self,
        left: &IrTypeExpr,
        right: &IrTypeExpr,
        hint: &str,
    ) -> Option<(IrTypeExpr, bool)> {
        if left == right {
            return Some((left.clone(), false));
        }
        if let Some((_, _, name)) = self
            .pairs
            .iter()
            .find(|(a, b, _)| (a == left && b == right) || (a == right && b == left))
        {
            return Some((IrTypeExpr::Named(name.clone()), true));
        }
        // Object pairs reserve productive recursive edges. Alias/container-only
        // cycles cannot supply an object view and must retain the old fallback.
        if let Some((_, _, object_depth)) = self
            .values
            .iter()
            .rev()
            .find(|(a, b, _)| (a == left && b == right) || (a == right && b == left))
            && *object_depth == self.pairs.len()
        {
            return None;
        }
        self.values
            .push((left.clone(), right.clone(), self.pairs.len()));
        let result = self.value_inner(left, right, hint);
        self.values.pop();
        result
    }

    fn value_inner(
        &mut self,
        left: &IrTypeExpr,
        right: &IrTypeExpr,
        hint: &str,
    ) -> Option<(IrTypeExpr, bool)> {
        // Follow aliases before inspecting wrappers, without erasing enum facts
        // or treating a union rendered as unknown as an unconstrained Any.
        for (candidate, other) in [(left, right), (right, left)] {
            if let IrTypeExpr::Named(name) = candidate
                && let Some(IrSchemaKind::Alias(target)) =
                    self.schemas.get(name).map(|schema| &schema.kind)
            {
                return self.value(&target.clone(), other, hint);
            }
        }
        match (left, right) {
            (IrTypeExpr::Any, value) | (value, IrTypeExpr::Any) => {
                let nested = matches!(
                    value,
                    IrTypeExpr::Named(_)
                        | IrTypeExpr::Array(_)
                        | IrTypeExpr::Map(_)
                        | IrTypeExpr::Union(_)
                );
                return Some((value.clone(), nested));
            }
            (IrTypeExpr::Primitive(a), IrTypeExpr::Primitive(b)) => {
                fn integer(value: &IrPrimitive) -> bool {
                    matches!(
                        value,
                        IrPrimitive::Integer | IrPrimitive::IntegerWithFormat(_)
                    )
                }
                fn number(value: &IrPrimitive) -> bool {
                    integer(value)
                        || matches!(
                            value,
                            IrPrimitive::Number | IrPrimitive::NumberWithFormat(_)
                        )
                }
                if number(a) && number(b) {
                    return Some((
                        IrTypeExpr::Primitive(if integer(b) { b.clone() } else { a.clone() }),
                        false,
                    ));
                }
            }
            (
                IrTypeExpr::Primitive(IrPrimitive::String),
                value @ (IrTypeExpr::StringLiteral(_) | IrTypeExpr::StringEnum(_)),
            )
            | (
                value @ (IrTypeExpr::StringLiteral(_) | IrTypeExpr::StringEnum(_)),
                IrTypeExpr::Primitive(IrPrimitive::String),
            ) => {
                return Some((value.clone(), false));
            }
            (IrTypeExpr::Nullable(a), IrTypeExpr::Nullable(b)) => {
                let (value, nested) = self.value(a, b, hint)?;
                return Some((IrTypeExpr::Nullable(Box::new(value)), nested));
            }
            (IrTypeExpr::Nullable(inner), value) | (value, IrTypeExpr::Nullable(inner)) => {
                return self.value(inner, value, hint);
            }
            (IrTypeExpr::Array(a), IrTypeExpr::Array(b)) => {
                let (value, nested) = self.value(a, b, &format!("{hint}Item"))?;
                return Some((IrTypeExpr::Array(Box::new(value)), nested));
            }
            (IrTypeExpr::Map(a), IrTypeExpr::Map(b)) => {
                let (value, nested) = self.value(a, b, &format!("{hint}Value"))?;
                return Some((IrTypeExpr::Map(Box::new(value)), nested));
            }
            _ => {}
        }
        let a = self.object(left, hint)?;
        let b = self.object(right, hint)?;
        let mut name = hint.to_pascal_case();
        let mut suffix = 2;
        while !self.used_names.insert(name.clone()) {
            name = format!("{}{suffix}", hint.to_pascal_case());
            suffix += 1;
        }
        // Reserve before descending into fields: productive recursive pairs refer
        // to this named view rather than expanding an infinite object tree.
        self.created.push(name.clone());
        self.pairs.push((left.clone(), right.clone(), name.clone()));
        self.schemas.insert(
            name.clone(),
            IrSchema {
                name: name.clone(),
                description: None,
                deprecated: false,
                kind: IrSchemaKind::Alias(IrTypeExpr::Any),
                is_component: true,
            },
        );
        let merged = self.merge_objects(a, b, &name)?;
        self.schemas
            .get_mut(&name)
            .expect("reserved object view")
            .kind = IrSchemaKind::Object(merged.object);
        Some((IrTypeExpr::Named(name), true))
    }
}
