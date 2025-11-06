//! TypeScript type expression definitions

use std::collections::{BTreeMap, BTreeSet};

use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use crate::ast::{TsParameter, TsPrimitive};
use crate::config::MAX_LINE_WIDTH;
use openapi_nexus_core::traits::ToRcDoc;

/// Object property with type expression and original name
#[derive(Debug, Clone, Ord, PartialOrd, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectProperty {
    /// The camelCase property name used in the TypeScript interface
    pub prop_name: String,
    /// The TypeScript type expression for this property
    pub type_expr: TsExpression,
    /// The original property name from the OpenAPI spec (used in JSON)
    pub original_name: String,
}

/// TypeScript type expression
#[derive(Debug, Clone, Ord, PartialOrd, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum TsExpression {
    /// Primitive type (e.g., string, number, boolean, etc).
    Primitive(TsPrimitive),
    /// Union type, represented as a set of possible type expressions.
    Union(BTreeSet<TsExpression>),
    /// Intersection type, represented as a set of combined type expressions.
    Intersection(BTreeSet<TsExpression>),
    /// Array type, containing a single item type expression.
    Array(Box<TsExpression>),
    /// Object type with named properties.
    /// The key is the camelCase name, the value is the full property definition including the original name.
    Object(BTreeMap<String, ObjectProperty>),
    /// Reference to another type (by its name).
    Reference(String),
    /// Generic type applied to one or more type arguments (e.g. `Array<T>`).
    Generic(String),
    /// Function type, capturing its parameters and (optional) return type.
    Function {
        /// Parameters accepted by the function (with names/types).
        parameters: Vec<TsParameter>,
        /// The return type of the function (absent if unknown).
        return_type: Option<Box<TsExpression>>,
    },
    /// Literal value (e.g. `'foo'`, `123`), represented as a string.
    Literal(String),
    /// Index signature type, e.g. `[key: string]: SomeType`.
    /// The first argument is the key name, the second is the type of the value.
    IndexSignature(String, Box<TsExpression>),
    /// Tuple type, a fixed-length array of types.
    Tuple(Vec<TsExpression>),
}

impl TsExpression {
    /// Convert this expression to a formatted string
    pub fn to_string_formatted(&self) -> String {
        self.to_rcdoc()
            .pretty(MAX_LINE_WIDTH)
            .to_string()
            .trim()
            .to_string()
    }

    /// Check if this expression is an array type
    pub fn is_array(&self) -> bool {
        matches!(self, TsExpression::Array(_))
    }

    /// Check if this expression is an array of objects (inline or reference)
    pub fn is_array_of_objects(&self) -> bool {
        if let TsExpression::Array(item_type) = self {
            matches!(
                item_type.as_ref(),
                TsExpression::Object(_) | TsExpression::Reference(_)
            )
        } else {
            false
        }
    }

    /// Check if this expression is an object reference
    pub fn is_object_reference(&self) -> bool {
        matches!(self, TsExpression::Reference(_))
    }

    /// Check if this expression is an inline object
    pub fn is_inline_object(&self) -> bool {
        matches!(self, TsExpression::Object(_))
    }

    /// Extract reference name from this expression, recursing into arrays if needed
    pub fn reference_name(&self) -> Option<String> {
        match self {
            TsExpression::Reference(name) => Some(name.clone()),
            TsExpression::Array(item_type) => item_type.reference_name(),
            _ => None,
        }
    }

    /// Extract array item type from this expression
    pub fn array_item_type(&self) -> Option<TsExpression> {
        if let TsExpression::Array(item_type) = self {
            Some(*item_type.clone())
        } else {
            None
        }
    }

    /// Extract object properties from this expression, recursing into arrays if needed
    pub fn object_properties(&self) -> Vec<ObjectProperty> {
        match self {
            TsExpression::Object(properties) => properties.values().cloned().collect(),
            TsExpression::Array(item_type) => {
                // If it's an array, extract from the item type
                item_type.object_properties()
            }
            _ => Vec::new(),
        }
    }

    /// Collect all referenced type names from this expression
    ///
    /// Recursively traverses the expression to find all type references
    /// that need to be imported. Returns a set of type names.
    pub fn referenced_types(&self) -> BTreeSet<String> {
        let mut references = BTreeSet::new();
        self.collect_references(&mut references);
        references
    }

    /// Recursively collect type references into the provided set
    fn collect_references(&self, references: &mut BTreeSet<String>) {
        match self {
            TsExpression::Reference(name) => {
                references.insert(name.clone());
            }
            TsExpression::Array(item_type) => {
                item_type.collect_references(references);
            }
            TsExpression::Union(types) => {
                for type_expr in types {
                    type_expr.collect_references(references);
                }
            }
            TsExpression::Intersection(types) => {
                for type_expr in types {
                    type_expr.collect_references(references);
                }
            }
            TsExpression::Object(properties) => {
                for property in properties.values() {
                    property.type_expr.collect_references(references);
                }
            }
            TsExpression::Tuple(types) => {
                for type_expr in types {
                    type_expr.collect_references(references);
                }
            }
            TsExpression::Function {
                parameters: _,
                return_type,
            } => {
                if let Some(ret_type) = return_type {
                    ret_type.collect_references(references);
                }
            }
            TsExpression::IndexSignature(_key, value_type) => {
                value_type.collect_references(references);
            }
            TsExpression::Primitive(_)
            | TsExpression::Generic(_)
            | TsExpression::Literal(_) => {
                // These don't contain type references
            }
        }
    }
}

impl ToRcDoc for TsExpression {
    fn to_rcdoc(&self) -> RcDoc<'static, ()> {
        match self {
            TsExpression::Primitive(primitive) => {
                let s = match primitive {
                    TsPrimitive::String => "string",
                    TsPrimitive::Number => "number",
                    TsPrimitive::Boolean => "boolean",
                    TsPrimitive::Any => "any",
                    TsPrimitive::Unknown => "unknown",
                    TsPrimitive::Void => "void",
                    TsPrimitive::Never => "never",
                    TsPrimitive::Null => "null",
                    TsPrimitive::Undefined => "undefined",
                };
                RcDoc::text(s)
            }
            TsExpression::Reference(name) | TsExpression::Generic(name) => {
                RcDoc::text(name.clone())
            }
            TsExpression::Array(item_type) => RcDoc::text("Array")
                .append(RcDoc::text("<"))
                .append(item_type.to_rcdoc())
                .append(RcDoc::text(">")),
            TsExpression::Union(types) => {
                let docs: Vec<_> = types.iter().map(|t| t.to_rcdoc()).collect();
                RcDoc::intersperse(
                    docs,
                    RcDoc::space()
                        .append(RcDoc::text("|"))
                        .append(RcDoc::space()),
                )
            }
            TsExpression::Intersection(types) => {
                let docs: Vec<_> = types.iter().map(|t| t.to_rcdoc()).collect();
                RcDoc::intersperse(
                    docs,
                    RcDoc::space()
                        .append(RcDoc::text("&"))
                        .append(RcDoc::space()),
                )
            }
            TsExpression::Function {
                parameters,
                return_type,
            } => {
                let param_docs: Vec<_> = parameters.iter().map(|p| p.to_rcdoc()).collect();
                let params = RcDoc::text("(")
                    .append(RcDoc::intersperse(
                        param_docs,
                        RcDoc::text(",").append(RcDoc::space()),
                    ))
                    .append(RcDoc::text(")"));
                let ret = if let Some(ret_type) = return_type {
                    ret_type.to_rcdoc()
                } else {
                    RcDoc::text("void")
                };
                params
                    .append(RcDoc::space())
                    .append(RcDoc::text("=>"))
                    .append(RcDoc::space())
                    .append(ret)
            }
            TsExpression::Object(properties) => {
                if properties.is_empty() {
                    RcDoc::text("{}")
                } else {
                    let prop_docs: Vec<_> = properties.values().map(|prop| {
                            RcDoc::text(prop.prop_name.clone())
                                .append(RcDoc::text(":"))
                                .append(RcDoc::space())
                                .append(prop.type_expr.to_rcdoc())
                        })
                        .collect();
                    RcDoc::text("{")
                        .append(RcDoc::space())
                        .append(RcDoc::intersperse(
                            prop_docs,
                            RcDoc::text(";").append(RcDoc::space()),
                        ))
                        .append(RcDoc::space())
                        .append(RcDoc::text("}"))
                }
            }
            TsExpression::Tuple(types) => {
                let docs: Vec<_> = types.iter().map(|t| t.to_rcdoc()).collect();
                RcDoc::text("[")
                    .append(RcDoc::intersperse(
                        docs,
                        RcDoc::text(",").append(RcDoc::space()),
                    ))
                    .append(RcDoc::text("]"))
            }
            TsExpression::Literal(value) => RcDoc::text(value.clone()),
            TsExpression::IndexSignature(key, value_type) => RcDoc::text("[")
                .append(RcDoc::text(key.clone()))
                .append(RcDoc::text(":"))
                .append(RcDoc::space())
                .append(value_type.to_rcdoc())
                .append(RcDoc::text("]")),
        }
    }
}
