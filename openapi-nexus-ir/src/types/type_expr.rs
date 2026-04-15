//! Type expressions — the building blocks for representing types in the IR.

use serde::Serialize;

/// A type expression that replaces `ObjectOrReference<ObjectSchema>` everywhere.
/// Fully resolved — no `$ref` wrappers, no spec-version-specific types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum IrTypeExpr {
    /// Reference to a named schema (by its key in `IrSpec.schemas`).
    Named(String),
    /// A primitive type (string, integer, number, boolean, etc.).
    Primitive(IrPrimitive),
    /// A literal string value (e.g., `"SimpleA"`). Used for single-value string enums
    /// when they appear as union members.
    StringLiteral(String),
    /// Inline string enum (e.g., `"active" | "inactive"`). Used for multi-value string
    /// enums in property types that don't warrant a separate named schema.
    StringEnum(Vec<String>),
    /// Array of items.
    Array(Box<IrTypeExpr>),
    /// Map with string keys and typed values (the `additionalProperties` pattern).
    Map(Box<IrTypeExpr>),
    /// Inline union of types (e.g. OAS 3.1 `type: [string, integer]`).
    /// Distinct from `IrSchemaKind::Union` which is a named schema-level union.
    Union(Vec<IrTypeExpr>),
    /// Nullable wrapper — the inner type can also be null.
    Nullable(Box<IrTypeExpr>),
    /// Truly untyped / any value.
    Any,
}

/// Primitive types with optional format hints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum IrPrimitive {
    String,
    Integer,
    Number,
    Boolean,
    /// Binary data (format: "binary").
    Binary,
    /// Date string (format: "date").
    Date,
    /// Date-time string (format: "date-time").
    DateTime,
    /// UUID string (format: "uuid").
    Uuid,
    /// Generic string with a format the IR doesn't special-case.
    StringWithFormat(String),
    /// Generic integer with a format (e.g. "int32", "int64").
    IntegerWithFormat(String),
    /// Generic number with a format (e.g. "float", "double").
    NumberWithFormat(String),
}

/// Validation constraints carried through from the spec.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct IrValidation {
    pub max_length: Option<u64>,
    pub min_length: Option<u64>,
    pub pattern: Option<String>,
    pub maximum: Option<f64>,
    pub exclusive_maximum: Option<bool>,
    pub minimum: Option<f64>,
    pub exclusive_minimum: Option<bool>,
    pub multiple_of: Option<f64>,
    pub max_items: Option<u64>,
    pub min_items: Option<u64>,
    pub unique_items: Option<bool>,
}
