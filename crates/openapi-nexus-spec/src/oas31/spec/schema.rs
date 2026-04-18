//! Schema specification for OpenAPI 3.1

use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Deserializer, Serialize};
use snafu::Snafu;

use super::{
    ErrorRef, FromRef, ObjectOrReference, OpenApiV31Spec, Ref, RefType,
    discriminator::Discriminator, spec_extensions,
};

/// Schema errors.
#[derive(Debug, Clone, PartialEq, Snafu)]
#[snafu(visibility(pub))]
pub enum ErrorSchema {
    #[snafu(display("Missing type field"))]
    NoType,

    #[snafu(display("Unknown type: {}", type_name))]
    UnknownType { type_name: String },

    #[snafu(display("Required property list specified for a non-object schema"))]
    RequiredSpecifiedOnNonObject,
}

/// Single schema type.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Type {
    Boolean,
    Integer,
    Number,
    String,
    Array,
    Object,
    Null,
}

/// Set of schema types.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum TypeSet {
    Single(Type),
    Multiple(Vec<Type>),
}

impl TypeSet {
    /// Returns `true` if this type-set contains the given type.
    pub fn contains(&self, type_: Type) -> bool {
        match self {
            TypeSet::Single(single_type) => *single_type == type_,
            TypeSet::Multiple(type_set) => type_set.contains(&type_),
        }
    }

    /// Returns `true` if this type-set is `object` or `[object, 'null']`.
    pub fn is_object_or_nullable_object(&self) -> bool {
        match self {
            TypeSet::Single(Type::Object) => true,
            TypeSet::Multiple(set) if set == &[Type::Object] => true,
            TypeSet::Multiple(set) if set == &[Type::Object, Type::Null] => true,
            TypeSet::Multiple(set) if set == &[Type::Null, Type::Object] => true,
            _ => false,
        }
    }

    /// Returns `true` if this type-set is `array` or `[array, 'null']`.
    pub fn is_array_or_nullable_array(&self) -> bool {
        match self {
            TypeSet::Single(Type::Array) => true,
            TypeSet::Multiple(set) if set == &[Type::Array] => true,
            TypeSet::Multiple(set) if set == &[Type::Array, Type::Null] => true,
            TypeSet::Multiple(set) if set == &[Type::Null, Type::Array] => true,
            _ => false,
        }
    }
}

/// A schema object allows the definition of input and output data types.
#[derive(Debug, Clone, PartialEq, Default, Deserialize, Serialize)]
pub struct ObjectSchema {
    #[serde(rename = "allOf", default, skip_serializing_if = "Vec::is_empty")]
    pub all_of: Vec<ObjectOrReference<ObjectSchema>>,

    #[serde(rename = "anyOf", default, skip_serializing_if = "Vec::is_empty")]
    pub any_of: Vec<ObjectOrReference<ObjectSchema>>,

    #[serde(rename = "oneOf", default, skip_serializing_if = "Vec::is_empty")]
    pub one_of: Vec<ObjectOrReference<ObjectSchema>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<Schema>>,

    #[serde(rename = "prefixItems", default, skip_serializing_if = "Vec::is_empty")]
    pub prefix_items: Vec<ObjectOrReference<ObjectSchema>>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, ObjectOrReference<ObjectSchema>>,

    #[serde(
        rename = "additionalProperties",
        skip_serializing_if = "Option::is_none"
    )]
    pub additional_properties: Option<Schema>,

    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub schema_type: Option<TypeSet>,

    #[serde(rename = "enum", default, skip_serializing_if = "Vec::is_empty")]
    pub enum_values: Vec<serde_json::Value>,

    #[serde(rename = "const", skip_serializing_if = "Option::is_none")]
    pub const_value: Option<serde_json::Value>,

    #[serde(rename = "multipleOf", skip_serializing_if = "Option::is_none")]
    pub multiple_of: Option<serde_json::Number>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum: Option<serde_json::Number>,

    #[serde(rename = "exclusiveMaximum", skip_serializing_if = "Option::is_none")]
    pub exclusive_maximum: Option<serde_json::Number>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<serde_json::Number>,

    #[serde(rename = "exclusiveMinimum", skip_serializing_if = "Option::is_none")]
    pub exclusive_minimum: Option<serde_json::Number>,

    #[serde(rename = "maxLength", skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u64>,

    #[serde(rename = "minLength", skip_serializing_if = "Option::is_none")]
    pub min_length: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,

    #[serde(rename = "maxItems", skip_serializing_if = "Option::is_none")]
    pub max_items: Option<u64>,

    #[serde(rename = "minItems", skip_serializing_if = "Option::is_none")]
    pub min_items: Option<u64>,

    #[serde(rename = "uniqueItems", skip_serializing_if = "Option::is_none")]
    pub unique_items: Option<bool>,

    #[serde(rename = "maxProperties", skip_serializing_if = "Option::is_none")]
    pub max_properties: Option<u64>,

    #[serde(rename = "minProperties", skip_serializing_if = "Option::is_none")]
    pub min_properties: Option<u64>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<bool>,

    #[serde(rename = "readOnly", skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,

    #[serde(rename = "writeOnly", skip_serializing_if = "Option::is_none")]
    pub write_only: Option<bool>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<serde_json::Value>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discriminator: Option<Discriminator>,

    #[serde(
        default,
        deserialize_with = "distinguish_missing_and_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub example: Option<serde_json::Value>,

    #[serde(flatten, with = "spec_extensions")]
    pub extensions: BTreeMap<String, serde_json::Value>,
}

impl ObjectSchema {
    /// Returns true if Null appears in set of schema types, or None if unspecified.
    pub fn is_nullable(&self) -> Option<bool> {
        Some(match self.schema_type.as_ref()? {
            TypeSet::Single(type_) => *type_ == Type::Null,
            TypeSet::Multiple(set) => set.contains(&Type::Null),
        })
    }
}

impl FromRef for ObjectSchema {
    fn from_ref(spec: &OpenApiV31Spec, path: &str) -> Result<Self, ErrorRef> {
        let refpath = path.parse::<Ref>()?;

        match refpath.kind {
            RefType::Schema => spec
                .components
                .as_ref()
                .and_then(|cs| cs.schemas.get(&refpath.name))
                .ok_or_else(|| ErrorRef::Unresolvable {
                    path: path.to_owned(),
                })
                .and_then(|oor| oor.resolve(spec)),

            typ => Err(ErrorRef::MismatchedType {
                expected: typ,
                actual: RefType::Schema,
            }),
        }
    }
}

/// A boolean JSON schema.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(transparent)]
pub struct BooleanSchema(pub bool);

/// A JSON schema document.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Schema {
    Boolean(BooleanSchema),
    Object(Box<ObjectOrReference<ObjectSchema>>),
}

/// Considers any value that is present as `Some`, including `null`.
fn distinguish_missing_and_null<'de, T, D>(de: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de> + fmt::Debug,
    D: Deserializer<'de>,
{
    T::deserialize(de).map(Some)
}

#[cfg(test)]
mod tests {
    use super::{ObjectSchema, Type};

    #[test]
    fn type_set_contains() {
        let spec = r#"{"type": "integer"}"#;
        let schema = serde_json::from_str::<ObjectSchema>(spec).unwrap();
        let schema_type = schema.schema_type.unwrap();
        assert!(schema_type.contains(Type::Integer));

        let spec = r#"{"type": ["integer", "null"]}"#;
        let schema = serde_json::from_str::<ObjectSchema>(spec).unwrap();
        let schema_type = schema.schema_type.unwrap();
        assert!(schema_type.contains(Type::Integer));

        let spec = r#"{"type": ["object", "null"]}"#;
        let schema = serde_json::from_str::<ObjectSchema>(spec).unwrap();
        let schema_type = schema.schema_type.unwrap();
        assert!(schema_type.contains(Type::Object));
        assert!(schema_type.is_object_or_nullable_object());
    }

    #[test]
    fn example_can_be_explicit_null() {
        let spec = r#"{"type": ["string", "null"]}"#;
        let schema = serde_json::from_str::<ObjectSchema>(spec).unwrap();
        assert_eq!(schema.example, None);

        let spec = r#"{"type": ["string", "null"], "example": null}"#;
        let schema = serde_json::from_str::<ObjectSchema>(spec).unwrap();
        assert_eq!(schema.example, Some(serde_json::Value::Null));
    }
}
