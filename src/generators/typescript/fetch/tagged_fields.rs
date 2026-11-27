//! TypeScript names and field ownership at an authored tagged use site.
//!
//! A common constraint may refer to a key supplied and renamed by the selected
//! native payload codec. Fields only present on the union are carried beside
//! that payload, without changing the reusable model or its nested values.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use heck::ToLowerCamelCase;
use sigil_stitch::prelude::TypeName;

use crate::ir::types::{IrJsonKind, IrSchemaKind, IrSpec, IrTypeExpr, IrWireShape, TaggingStyle};

use super::plan::union_type;

#[derive(Default)]
pub(super) struct TaggedFields {
    model_keys: BTreeSet<String>,
    discriminator: String,
    pub(super) envelope: BTreeMap<String, EnvelopeField>,
}

pub(super) struct EnvelopeField {
    pub(super) required: bool,
    pub(super) rename: bool,
    pub(super) ty: TypeName,
}

impl EnvelopeField {
    pub(super) fn model_key(&self, wire: &str) -> String {
        model_key(wire, self.rename)
    }
}

fn model_key(wire: &str, rename: bool) -> String {
    if rename {
        wire.to_lower_camel_case()
    } else {
        wire.to_owned()
    }
}

impl TaggedFields {
    pub(super) fn new(
        common: Option<&IrWireShape>,
        payload: &IrTypeExpr,
        discriminator: &str,
        ir: &IrSpec,
    ) -> Self {
        let mut model_keys = BTreeSet::from([discriminator.to_owned()]);
        native_keys(payload, ir, &mut HashSet::new(), &mut model_keys);
        let mut common_fields = BTreeMap::new();
        if let Some(common) = common {
            envelope_fields(common, ir, &mut HashSet::new(), &mut common_fields);
        }
        common_fields.retain(|wire, _| !model_keys.contains(wire));
        model_keys.extend(
            common_fields
                .iter()
                .filter(|(_, field)| field.rename)
                .map(|(wire, _)| wire.clone()),
        );
        Self {
            model_keys,
            discriminator: discriminator.to_owned(),
            envelope: common_fields,
        }
    }

    pub(super) fn renames_envelope(&self, key: &str) -> bool {
        self.discriminator == key || self.envelope.get(key).is_some_and(|field| field.rename)
    }

    pub(super) fn renames(&self, key: &str) -> bool {
        self.model_keys.contains(key)
    }
}

fn native_keys<'ir>(
    payload: &'ir IrTypeExpr,
    ir: &'ir IrSpec,
    seen: &mut HashSet<&'ir str>,
    keys: &mut BTreeSet<String>,
) {
    match payload.representation() {
        IrTypeExpr::Named(name) if seen.insert(name) => {
            match ir.schemas.get(name).map(|schema| &schema.kind) {
                Some(IrSchemaKind::Object(object)) => {
                    keys.extend(object.properties.keys().cloned())
                }
                Some(IrSchemaKind::Alias(inner)) => native_keys(inner, ir, seen, keys),
                Some(IrSchemaKind::Intersection(all)) => {
                    for part in &all.members {
                        native_keys(part, ir, seen, keys);
                    }
                }
                Some(IrSchemaKind::Union(union)) => {
                    for part in &union.members {
                        native_keys(part, ir, seen, keys);
                    }
                }
                Some(IrSchemaKind::TaggedUnion(union)) => match &union.tagging {
                    TaggingStyle::Internal => {
                        keys.insert(union.discriminator_field.clone());
                        for case in &union.variants {
                            native_keys(&case.content_type, ir, seen, keys);
                        }
                    }
                    TaggingStyle::Adjacent { content_field } => {
                        keys.insert(union.discriminator_field.clone());
                        keys.insert(content_field.clone());
                    }
                    TaggingStyle::External => {}
                },
                _ => {}
            }
            seen.remove(name.as_str());
        }
        IrTypeExpr::Nullable(inner) => native_keys(inner, ir, seen, keys),
        IrTypeExpr::Union(parts) => {
            for part in parts {
                native_keys(part, ir, seen, keys);
            }
        }
        _ => {}
    }
}

fn envelope_fields<'ir>(
    shape: &'ir IrWireShape,
    ir: &'ir IrSpec,
    seen: &mut HashSet<&'ir str>,
    fields: &mut BTreeMap<String, EnvelopeField>,
) {
    match shape {
        IrWireShape::Object(object) => {
            for (key, shape) in &object.properties {
                let ty = scalar_type(shape);
                fields
                    .entry(key.clone())
                    .and_modify(|field| {
                        field.rename = true;
                        if field.ty == TypeName::primitive("unknown") {
                            field.ty = ty.clone();
                        }
                    })
                    .or_insert(EnvelopeField {
                        required: false,
                        rename: true,
                        ty,
                    });
            }
            for key in &object.required {
                fields
                    .entry(key.clone())
                    .and_modify(|field| field.required = true)
                    .or_insert_with(|| EnvelopeField {
                        required: true,
                        rename: false,
                        ty: scalar_type(&object.additional_properties),
                    });
            }
        }
        IrWireShape::AllOf(parts) => {
            for part in parts {
                envelope_fields(part, ir, seen, fields);
            }
        }
        IrWireShape::OneOf(parts) | IrWireShape::AnyOf(parts) => {
            let shapes: Vec<_> = parts
                .iter()
                .map(|part| {
                    let mut branch = BTreeMap::new();
                    envelope_fields(part, ir, seen, &mut branch);
                    branch
                })
                .collect();
            let keys: BTreeSet<_> = shapes.iter().flat_map(BTreeMap::keys).collect();
            for key in keys {
                let in_all = shapes.iter().all(|branch| branch.contains_key(key));
                let projected: Vec<_> =
                    shapes.iter().filter_map(|branch| branch.get(key)).collect();
                let field = EnvelopeField {
                    required: in_all && projected.iter().all(|field| field.required),
                    rename: projected.iter().any(|field| field.rename),
                    // An alternative without the declaration may accept values
                    // beyond the other alternatives' scalar types.
                    ty: if in_all {
                        union_type(projected.iter().map(|field| field.ty.clone()).collect())
                    } else {
                        TypeName::primitive("unknown")
                    },
                };
                fields
                    .entry(key.clone())
                    .and_modify(|prior| {
                        prior.required |= field.required;
                        prior.rename |= field.rename;
                    })
                    .or_insert(field);
            }
        }
        IrWireShape::Reference(name) if seen.insert(name) => {
            if let Some(target) = ir.wire_schemas.get(name) {
                envelope_fields(target, ir, seen, fields);
            }
            seen.remove(name.as_str());
        }
        _ => {}
    }
}

// A use-site field has no independent lowered model. Declare scalar kinds, but
// leave complex subtrees opaque and wire-shaped instead of claiming a codec we
// cannot invoke. Matchers still validate against the complete authored shape.
fn scalar_type(shape: &IrWireShape) -> TypeName {
    match shape {
        IrWireShape::Kind(IrJsonKind::Boolean) => TypeName::primitive("boolean"),
        IrWireShape::Kind(IrJsonKind::Integer | IrJsonKind::Number) => {
            TypeName::primitive("number")
        }
        IrWireShape::Kind(IrJsonKind::String) => TypeName::primitive("string"),
        IrWireShape::Kind(IrJsonKind::Null) => TypeName::primitive("null"),
        IrWireShape::AnyOf(parts) => union_type(parts.iter().map(scalar_type).collect()),
        _ => TypeName::primitive("unknown"),
    }
}
