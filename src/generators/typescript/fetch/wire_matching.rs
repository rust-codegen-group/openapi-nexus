//! Project a shared authored guard into TypeScript's static matching helpers.
//! Only property access changes with the naming mode; wire facts do not.

use std::collections::{BTreeMap, HashMap};

use crate::generators::union_match::{JsonShape, MatchScope, PropertyMatch};
use crate::ir::types::{IrJsonKind, IrWireShape};

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

pub(super) struct Projection<'a> {
    schemas: &'a BTreeMap<String, IrWireShape>,
    visiting: HashMap<&'a str, usize>,
    wire_keys: bool,
}

impl<'a> Projection<'a> {
    pub(super) fn new(schemas: &'a BTreeMap<String, IrWireShape>, wire_keys: bool) -> Self {
        Self {
            schemas,
            visiting: HashMap::new(),
            wire_keys,
        }
    }

    pub(super) fn shape(&mut self, shape: &'a IrWireShape) -> JsonShape {
        self.at(shape, 0)
    }

    pub(super) fn named(&mut self, name: &str) -> JsonShape {
        let Some((name, shape)) = self.schemas.get_key_value(name) else {
            return JsonShape::Never;
        };
        self.visiting.insert(name, 0);
        let shape = self.shape(shape);
        self.visiting.remove(name.as_str());
        shape
    }

    fn at(&mut self, shape: &'a IrWireShape, depth: usize) -> JsonShape {
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
                or(values.iter().cloned().map(JsonShape::Literal).collect())
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
                        required: object.required.contains(name),
                        rename: !self.wire_keys && object.properties.contains_key(name),
                        wire_name: name.clone(),
                        // A required-but-undeclared key remains an extension in
                        // this conjunct, so it must satisfy the local value shape.
                        shape: self.at(
                            object
                                .properties
                                .get(name)
                                .unwrap_or(&object.additional_properties),
                            depth + 1,
                        ),
                    })
                    .collect();
                if matches!(*object.additional_properties, IrWireShape::Any) {
                    JsonShape::Object(properties)
                } else {
                    let values = Box::new(self.at(&object.additional_properties, depth + 1));
                    if object.properties.is_empty() && object.required.is_empty() {
                        JsonShape::Map(values)
                    } else {
                        JsonShape::ObjectWithAdditional { properties, values }
                    }
                }
            }
            IrWireShape::Array(item) => JsonShape::Array(Box::new(self.at(item, depth + 1))),
            IrWireShape::AllOf(parts) => {
                and(parts.iter().map(|part| self.at(part, depth)).collect())
            }
            IrWireShape::OneOf(parts) | IrWireShape::AnyOf(parts) => {
                or(parts.iter().map(|part| self.at(part, depth)).collect())
            }
            IrWireShape::Reference(name) => {
                if let Some(ancestor) = self.visiting.get(name.as_str()) {
                    return if depth > *ancestor {
                        reference(name, self.wire_keys)
                    } else {
                        JsonShape::Never
                    };
                }
                let Some(target) = self.schemas.get(name.as_str()) else {
                    return JsonShape::Never;
                };
                self.visiting.insert(name, depth);
                let shape = self.at(target, depth);
                self.visiting.remove(name.as_str());
                shape
            }
        }
    }
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
