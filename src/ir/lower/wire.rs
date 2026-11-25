//! Lower the authored shape used by wire dispatch without model promotion.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::ir::types::{IrJsonKind, IrUnionWire, IrWireObject, IrWireShape};

use super::composition::CompositionSchema;

pub(super) trait WireSchema: CompositionSchema {
    type Schema;

    fn kinds(&self) -> Vec<IrJsonKind>;
    fn properties(&self) -> &BTreeMap<String, Self::Member>;
    fn one_of(&self) -> &[Self::Member];
    fn any_of(&self) -> &[Self::Member];
    fn additional(&self) -> IrWireShape;
    fn items(&self) -> Option<&Self::Schema>;
    fn constant(&self) -> Option<&Value>;
    fn schema(value: &Self::Schema) -> IrWireShape;
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Alternatives {
    OneOf,
    AnyOf,
}

pub(super) fn union<S: WireSchema>(node: &S) -> IrUnionWire {
    let (kind, alternatives) = if node.one_of().is_empty() {
        (Alternatives::AnyOf, node.any_of())
    } else {
        (Alternatives::OneOf, node.one_of())
    };
    IrUnionWire {
        common: shape_excluding(node, Some(kind)),
        branches: alternatives.iter().map(member::<S>).collect(),
    }
}

pub(super) fn member<S: WireSchema>(node: &S::Member) -> IrWireShape {
    if let Some(reference) = S::reference(node) {
        let name = reference.strip_prefix("#/components/schemas/").map_or_else(
            || reference.to_owned(),
            |name| name.replace("~1", "/").replace("~0", "~"),
        );
        IrWireShape::Reference(name)
    } else {
        shape(S::object(node).expect("schema member is an object or reference"))
    }
}

fn shape<S: WireSchema>(node: &S) -> IrWireShape {
    shape_excluding(node, None)
}

fn shape_excluding<S: WireSchema>(node: &S, alternatives: Option<Alternatives>) -> IrWireShape {
    let mut constraints = Vec::new();
    if !node.enum_values().is_empty() {
        constraints.push(IrWireShape::Finite(node.enum_values().to_vec()));
    }
    if let Some(value) = node.constant() {
        constraints.push(IrWireShape::Finite(vec![value.clone()]));
    }

    let additional = node.additional();
    let has_object_fields = !node.properties().is_empty()
        || !node.required().is_empty()
        || additional != IrWireShape::Any;
    let kinds = node.kinds();
    let object = || {
        IrWireShape::Object(IrWireObject {
            properties: node
                .properties()
                .iter()
                .map(|(key, value)| (key.clone(), member::<S>(value)))
                .collect(),
            required: node.required().iter().cloned().collect(),
            additional_properties: Box::new(additional.clone()),
        })
    };
    let array = || IrWireShape::Array(Box::new(node.items().map_or(IrWireShape::Any, S::schema)));
    if kinds.is_empty() {
        if has_object_fields {
            constraints.push(object());
        } else if node.items().is_some() {
            constraints.push(array());
        }
    } else {
        let shapes: Vec<_> = kinds
            .into_iter()
            .map(|kind| match kind {
                IrJsonKind::Object if has_object_fields => object(),
                IrJsonKind::Array if node.items().is_some() => array(),
                _ => IrWireShape::Kind(kind),
            })
            .collect();
        constraints.push(group(shapes, IrWireShape::AnyOf));
    }

    constraints.extend(node.all_of().iter().map(member::<S>));
    if alternatives != Some(Alternatives::OneOf) && !node.one_of().is_empty() {
        constraints.push(IrWireShape::OneOf(
            node.one_of().iter().map(member::<S>).collect(),
        ));
    }
    if alternatives != Some(Alternatives::AnyOf) && !node.any_of().is_empty() {
        constraints.push(IrWireShape::AnyOf(
            node.any_of().iter().map(member::<S>).collect(),
        ));
    }
    group(constraints, IrWireShape::AllOf)
}

fn group(mut shapes: Vec<IrWireShape>, many: fn(Vec<IrWireShape>) -> IrWireShape) -> IrWireShape {
    match shapes.len() {
        0 => IrWireShape::Any,
        1 => shapes.pop().expect("one wire shape"),
        _ => many(shapes),
    }
}

fn boolean(value: bool) -> IrWireShape {
    if value {
        IrWireShape::Any
    } else {
        IrWireShape::Never
    }
}

macro_rules! common {
    ($version:ident) => {
        type Schema = crate::spec::$version::spec::Schema;

        fn properties(&self) -> &BTreeMap<String, Self::Member> {
            &self.properties
        }
        fn one_of(&self) -> &[Self::Member] {
            &self.one_of
        }
        fn any_of(&self) -> &[Self::Member] {
            &self.any_of
        }
        fn items(&self) -> Option<&Self::Schema> {
            self.items.as_deref()
        }
        fn schema(value: &Self::Schema) -> IrWireShape {
            match value {
                Self::Schema::Boolean(b) => boolean(b.0),
                Self::Schema::Object(object) => member::<Self>(object),
            }
        }
    };
}

impl WireSchema for crate::spec::oas30::spec::ObjectSchema {
    common!(oas30);

    fn kinds(&self) -> Vec<IrJsonKind> {
        let kind = match self.schema_type.as_deref() {
            Some("boolean") => IrJsonKind::Boolean,
            Some("integer") => IrJsonKind::Integer,
            Some("number") => IrJsonKind::Number,
            Some("string") => IrJsonKind::String,
            Some("array") => IrJsonKind::Array,
            Some("object") => IrJsonKind::Object,
            _ => return Vec::new(),
        };
        if self.nullable == Some(true) {
            vec![kind, IrJsonKind::Null]
        } else {
            vec![kind]
        }
    }

    fn additional(&self) -> IrWireShape {
        use crate::spec::oas30::spec::AdditionalProperties;
        match self.additional_properties.as_ref() {
            None => IrWireShape::Any,
            Some(AdditionalProperties::Boolean(b)) => boolean(*b),
            Some(AdditionalProperties::Schema(schema)) => Self::schema(schema),
        }
    }

    fn constant(&self) -> Option<&Value> {
        None
    }
}

macro_rules! adapt_type_sets {
    ($version:ident) => {
        impl WireSchema for crate::spec::$version::spec::ObjectSchema {
            common!($version);

            fn kinds(&self) -> Vec<IrJsonKind> {
                use crate::spec::$version::spec::{SchemaType as Type, SchemaTypeSet as TypeSet};
                match self.schema_type.as_ref() {
                    None => &[],
                    Some(TypeSet::Single(kind)) => std::slice::from_ref(kind),
                    Some(TypeSet::Multiple(kinds)) => kinds.as_slice(),
                }
                .iter()
                .map(|kind| match kind {
                    Type::Null => IrJsonKind::Null,
                    Type::Boolean => IrJsonKind::Boolean,
                    Type::Integer => IrJsonKind::Integer,
                    Type::Number => IrJsonKind::Number,
                    Type::String => IrJsonKind::String,
                    Type::Array => IrJsonKind::Array,
                    Type::Object => IrJsonKind::Object,
                })
                .collect()
            }

            fn additional(&self) -> IrWireShape {
                self.additional_properties
                    .as_ref()
                    .map_or(IrWireShape::Any, Self::schema)
            }

            fn constant(&self) -> Option<&Value> {
                self.const_value.as_ref()
            }
        }
    };
}

adapt_type_sets!(oas31);
adapt_type_sets!(oas32);
