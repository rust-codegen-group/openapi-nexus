//! TypeScript type expression definitions

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use crate::ast::{TsParameter, TsPrimitive};
use crate::emission::error::EmitError;
use openapi_nexus_core::traits::ToRcDoc;

/// TypeScript type expression
#[derive(Debug, Clone, Ord, PartialOrd, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum TsExpression {
    Primitive(TsPrimitive),
    Union(BTreeSet<TsExpression>),
    Intersection(BTreeSet<TsExpression>),
    Array(Box<TsExpression>),
    Object(BTreeMap<String, TsExpression>),
    Reference(String),
    Generic(String),
    Function {
        parameters: Vec<TsParameter>,
        return_type: Option<Box<TsExpression>>,
    },
    Literal(String),
    IndexSignature(String, Box<TsExpression>),
    Tuple(Vec<TsExpression>),
}

impl fmt::Display for TsExpression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TsExpression::Primitive(primitive) => match primitive {
                TsPrimitive::String => write!(f, "string"),
                TsPrimitive::Number => write!(f, "number"),
                TsPrimitive::Boolean => write!(f, "boolean"),
                TsPrimitive::Any => write!(f, "any"),
                TsPrimitive::Unknown => write!(f, "unknown"),
                TsPrimitive::Void => write!(f, "void"),
                TsPrimitive::Never => write!(f, "never"),
                TsPrimitive::Null => write!(f, "null"),
                TsPrimitive::Undefined => write!(f, "undefined"),
            },
            TsExpression::Reference(name) => write!(f, "{}", name),
            TsExpression::Array(item_type) => write!(f, "Array<{}>", item_type),
            TsExpression::Union(types) => {
                let type_strings: Vec<String> = types.iter().map(|t| t.to_string()).collect();
                write!(f, "{}", type_strings.join(" | "))
            }
            TsExpression::Intersection(types) => {
                let type_strings: Vec<String> = types.iter().map(|t| t.to_string()).collect();
                write!(f, "{}", type_strings.join(" & "))
            }
            TsExpression::Function {
                parameters,
                return_type,
            } => {
                let params: Vec<String> = parameters
                    .iter()
                    .map(|p| match &p.type_expr {
                        Some(t) => format!("{}: {}", p.name, t),
                        None => p.name.clone(),
                    })
                    .collect();
                let return_type_str = if let Some(ret_type) = return_type {
                    ret_type.to_string()
                } else {
                    "void".to_string()
                };
                write!(f, "({}) => {}", params.join(", "), return_type_str)
            }
            TsExpression::Object(properties) => {
                let prop_strings: Vec<String> = properties
                    .iter()
                    .map(|(name, type_expr)| format!("{}: {}", name, type_expr))
                    .collect();
                write!(f, "{{ {} }}", prop_strings.join("; "))
            }
            TsExpression::Tuple(types) => {
                let type_strings: Vec<String> = types.iter().map(|t| t.to_string()).collect();
                write!(f, "[{}]", type_strings.join(", "))
            }
            TsExpression::Literal(value) => write!(f, "{}", value),
            TsExpression::Generic(name) => write!(f, "{}", name),
            TsExpression::IndexSignature(key, value_type) => {
                write!(f, "[{}: {}]", key, value_type)
            }
        }
    }
}

impl ToRcDoc for TsExpression {
    type Error = EmitError;

    fn to_rcdoc(&self) -> Result<RcDoc<'static, ()>, EmitError> {
        let doc = match self {
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
                .append(item_type.to_rcdoc()?)
                .append(RcDoc::text(">")),
            TsExpression::Union(types) => {
                let docs: Result<Vec<_>, _> = types.iter().map(|t| t.to_rcdoc()).collect();
                RcDoc::intersperse(
                    docs?,
                    RcDoc::space()
                        .append(RcDoc::text("|"))
                        .append(RcDoc::space()),
                )
            }
            TsExpression::Intersection(types) => {
                let docs: Result<Vec<_>, _> = types.iter().map(|t| t.to_rcdoc()).collect();
                RcDoc::intersperse(
                    docs?,
                    RcDoc::space()
                        .append(RcDoc::text("&"))
                        .append(RcDoc::space()),
                )
            }
            TsExpression::Function {
                parameters,
                return_type,
            } => {
                let param_docs: Result<Vec<_>, _> =
                    parameters.iter().map(|p| p.to_rcdoc()).collect();
                let params = RcDoc::text("(")
                    .append(RcDoc::intersperse(
                        param_docs?,
                        RcDoc::text(",").append(RcDoc::space()),
                    ))
                    .append(RcDoc::text(")"));
                let ret = if let Some(ret_type) = return_type {
                    ret_type.to_rcdoc()?
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
                    let prop_docs: Result<Vec<_>, _> = properties
                        .iter()
                        .map(|(name, type_expr)| {
                            Ok(RcDoc::text(name.clone())
                                .append(RcDoc::text(":"))
                                .append(RcDoc::space())
                                .append(type_expr.to_rcdoc()?))
                        })
                        .collect();
                    RcDoc::text("{")
                        .append(RcDoc::space())
                        .append(RcDoc::intersperse(
                            prop_docs?,
                            RcDoc::text(";").append(RcDoc::space()),
                        ))
                        .append(RcDoc::space())
                        .append(RcDoc::text("}"))
                }
            }
            TsExpression::Tuple(types) => {
                let docs: Result<Vec<_>, _> = types.iter().map(|t| t.to_rcdoc()).collect();
                RcDoc::text("[")
                    .append(RcDoc::intersperse(
                        docs?,
                        RcDoc::text(",").append(RcDoc::space()),
                    ))
                    .append(RcDoc::text("]"))
            }
            TsExpression::Literal(value) => RcDoc::text(value.clone()),
            TsExpression::IndexSignature(key, value_type) => RcDoc::text("[")
                .append(RcDoc::text(key.clone()))
                .append(RcDoc::text(":"))
                .append(RcDoc::space())
                .append(value_type.to_rcdoc()?)
                .append(RcDoc::text("]")),
        };
        Ok(doc)
    }
}
