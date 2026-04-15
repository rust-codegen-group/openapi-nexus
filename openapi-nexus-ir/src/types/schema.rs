//! Schema types — the core of the IR's type system.

use indexmap::IndexMap;
use serde::Serialize;

use super::type_expr::{IrTypeExpr, IrValidation};

/// A named schema in the IR. Every schema has a name (its key in `IrSpec.schemas`),
/// optional metadata, and a classified kind.
#[derive(Debug, Clone, Serialize)]
pub struct IrSchema {
    pub name: String,
    pub description: Option<String>,
    pub deprecated: bool,
    pub kind: IrSchemaKind,
    /// Whether this schema originated from `components/schemas` (true) or was
    /// promoted from an inline definition in operations/responses (false).
    pub is_component: bool,
}

/// Pre-classified schema kind. The lowering pass determines this once;
/// generators match on it directly instead of re-interpreting raw spec fields.
#[derive(Debug, Clone, Serialize)]
pub enum IrSchemaKind {
    /// An object with named properties and optional additional properties.
    Object(IrObject),
    /// A typed enumeration.
    Enum(IrEnum),
    /// A discriminated union (oneOf with a discriminator field).
    TaggedUnion(IrTaggedUnion),
    /// An untagged union (oneOf/anyOf without a discriminator).
    Union(IrUnion),
    /// An intersection type (allOf).
    Intersection(IrIntersection),
    /// A type alias — wraps a single type expression.
    Alias(IrTypeExpr),
}

// ---------------------------------------------------------------------------
// Object
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct IrObject {
    pub properties: IndexMap<String, IrProperty>,
    pub additional_properties: Option<IrTypeExpr>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IrProperty {
    pub name: String,
    pub type_expr: IrTypeExpr,
    pub required: bool,
    pub nullable: bool,
    pub description: Option<String>,
    pub default_value: Option<serde_json::Value>,
    pub format: Option<String>,
    pub validation: Option<IrValidation>,
}

// ---------------------------------------------------------------------------
// Enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct IrEnum {
    pub value_type: IrEnumValueType,
    pub values: Vec<IrEnumValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum IrEnumValueType {
    String,
    Integer,
    Number,
    /// Mixed types (e.g. string + integer).
    Mixed,
}

#[derive(Debug, Clone, Serialize)]
pub struct IrEnumValue {
    pub value: serde_json::Value,
    pub description: Option<String>,
}

// ---------------------------------------------------------------------------
// Tagged Union (discriminated)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct IrTaggedUnion {
    pub discriminator_field: String,
    pub tagging: TaggingStyle,
    pub variants: Vec<IrTaggedVariant>,
}

#[derive(Debug, Clone, Serialize)]
pub enum TaggingStyle {
    /// Tag is a property inside the object itself (`allOf[$ref, { tag: enum }]`).
    Internal,
    /// Tag and content are separate fields.
    Adjacent { content_field: String },
    /// The key in a wrapping object determines the variant.
    External,
}

#[derive(Debug, Clone, Serialize)]
pub struct IrTaggedVariant {
    pub discriminator_value: String,
    pub content_type: IrTypeExpr,
    pub description: Option<String>,
}

// ---------------------------------------------------------------------------
// Union (untagged)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct IrUnion {
    pub members: Vec<IrTypeExpr>,
    /// Whether the union also includes null (e.g., `anyOf: [string, number, null]`).
    pub nullable: bool,
}

// ---------------------------------------------------------------------------
// Intersection (allOf)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct IrIntersection {
    pub members: Vec<IrTypeExpr>,
}
