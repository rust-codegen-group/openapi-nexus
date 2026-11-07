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
    /// Finite scalar values with the pre-existing target representation retained.
    /// Generators may use the values for selection without changing public types.
    ScalarEnum {
        representation: Box<Self>,
        /// Whether the retained representation is an authored value constraint,
        /// rather than the legacy string fallback for an enum without a type.
        representation_is_constraint: bool,
        /// Null permission extracted from this representation by `split_nullable`.
        /// Keep its original constraint boundary without changing legacy types.
        representation_nullable: bool,
        values: Vec<serde_json::Value>,
    },
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

impl IrTypeExpr {
    /// View the pre-existing representation without scalar selection metadata.
    /// Generators that do not consume finite-value facts can keep their existing
    /// type, codec and transport policies through this transparent view.
    pub fn representation(&self) -> &Self {
        match self {
            Self::ScalarEnum { representation, .. } => representation.representation(),
            other => other,
        }
    }

    /// Separate a property's structural nullability without moving finite-value
    /// constraints across that boundary. The enum still constrains the whole
    /// original node, including whether null is one of its permitted values.
    pub(crate) fn split_nullable(self) -> (Self, bool) {
        match self {
            Self::Nullable(inner) => (*inner, true),
            Self::ScalarEnum {
                representation,
                representation_is_constraint,
                representation_nullable,
                values,
            } => {
                let removed_here = matches!(*representation, Self::Nullable(_));
                let (representation, nullable) = representation.split_nullable();
                (
                    Self::ScalarEnum {
                        representation: Box::new(representation),
                        representation_is_constraint,
                        representation_nullable: representation_nullable || removed_here,
                        values,
                    },
                    nullable || representation_nullable,
                )
            }
            other => (other, false),
        }
    }

    /// Retain scalar enum facts without widening the supported type grammar or
    /// changing the representation consumed by existing language generators.
    pub(crate) fn with_scalar_enum(
        self,
        values: &[serde_json::Value],
        representation_is_constraint: bool,
    ) -> Self {
        if values.is_empty()
            || values
                .iter()
                .any(|value| value.is_array() || value.is_object())
            || matches!(&self, Self::StringLiteral(_) | Self::StringEnum(_))
        {
            self
        } else {
            Self::ScalarEnum {
                representation: Box::new(self),
                representation_is_constraint,
                representation_nullable: false,
                values: values.to_vec(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finite(representation: IrTypeExpr, values: Vec<serde_json::Value>) -> IrTypeExpr {
        IrTypeExpr::ScalarEnum {
            representation: Box::new(representation),
            representation_is_constraint: true,
            representation_nullable: false,
            values,
        }
    }

    #[test]
    fn extracted_null_permission_remains_at_its_original_enum_boundary() {
        let integer = IrTypeExpr::Primitive(IrPrimitive::Integer);
        let inner_values = vec![1.into()];
        let outer_values = vec![1.into(), serde_json::Value::Null];
        let outside = finite(
            IrTypeExpr::Nullable(Box::new(finite(integer.clone(), inner_values.clone()))),
            outer_values.clone(),
        );
        let inside = finite(
            finite(
                IrTypeExpr::Nullable(Box::new(integer.clone())),
                inner_values,
            ),
            outer_values,
        );
        let (outside, outside_nullable) = outside.split_nullable();
        let (inside, inside_nullable) = inside.split_nullable();
        assert!(outside_nullable && inside_nullable);
        assert_eq!(outside.representation(), &integer);
        assert_eq!(inside.representation(), &integer);
        assert_ne!(
            outside, inside,
            "the same legacy type must not erase constraint scope"
        );
        let IrTypeExpr::ScalarEnum {
            representation,
            representation_nullable,
            ..
        } = outside
        else {
            panic!("outer finite constraint");
        };
        assert!(representation_nullable);
        assert!(matches!(
            *representation,
            IrTypeExpr::ScalarEnum {
                representation_nullable: false,
                ..
            }
        ));
        let IrTypeExpr::ScalarEnum {
            representation,
            representation_nullable,
            ..
        } = inside
        else {
            panic!("outer finite constraint");
        };
        assert!(!representation_nullable);
        assert!(matches!(
            *representation,
            IrTypeExpr::ScalarEnum {
                representation_nullable: true,
                ..
            }
        ));
    }
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
