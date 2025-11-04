//! TypeScript type expression definitions

use std::collections::{BTreeMap, BTreeSet};

use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use crate::ast::{TsParameter, TsPrimitive};
use crate::config::MAX_LINE_WIDTH;
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

impl TsExpression {
    /// Convert this expression to a formatted string
    pub fn to_string_formatted(&self) -> String {
        self.to_rcdoc()
            .pretty(MAX_LINE_WIDTH)
            .to_string()
            .trim()
            .to_string()
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
                    let prop_docs: Vec<_> = properties
                        .iter()
                        .map(|(name, type_expr)| {
                            RcDoc::text(name.clone())
                                .append(RcDoc::text(":"))
                                .append(RcDoc::space())
                                .append(type_expr.to_rcdoc())
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
