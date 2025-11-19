//! Go type expressions

use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use crate::ast::ty::GoPrimitive;
use openapi_nexus_core::traits::ToRcDoc;

/// Go type expression
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GoExpression {
    /// Primitive type
    Primitive(GoPrimitive),
    /// Pointer type (*T)
    Pointer(Box<GoExpression>),
    /// Slice type ([]T)
    Slice(Box<GoExpression>),
    /// Map type (map[K]V)
    Map {
        key: Box<GoExpression>,
        value: Box<GoExpression>,
    },
    /// Struct reference
    Reference(String),
    /// Interface reference
    Interface(String),
    /// Function type
    Function {
        params: Vec<GoExpression>,
        returns: Vec<GoExpression>,
    },
    /// Interface{} (any)
    Any,
    /// OptionalNullable[T] type
    OptionalNullable(Box<GoExpression>),
}

impl ToRcDoc for GoExpression {
    fn to_rcdoc(&self) -> RcDoc<'static, ()> {
        match self {
            GoExpression::Primitive(p) => p.to_rcdoc(),
            GoExpression::Pointer(inner) => RcDoc::text("*").append(inner.to_rcdoc()),
            GoExpression::Slice(inner) => RcDoc::text("[]").append(inner.to_rcdoc()),
            GoExpression::Map { key, value } => RcDoc::text("map[")
                .append(key.to_rcdoc())
                .append(RcDoc::text("]"))
                .append(value.to_rcdoc()),
            GoExpression::Reference(name) => RcDoc::text(name.clone()),
            GoExpression::Interface(name) => RcDoc::text(name.clone()),
            GoExpression::Function { params, returns } => {
                let param_docs: Vec<_> = params.iter().map(|p| p.to_rcdoc()).collect();
                let params_doc = if param_docs.is_empty() {
                    RcDoc::text("()")
                } else {
                    RcDoc::text("(")
                        .append(RcDoc::intersperse(param_docs, RcDoc::text(", ")))
                        .append(RcDoc::text(")"))
                };

                let returns_doc = if returns.is_empty() {
                    RcDoc::nil()
                } else if returns.len() == 1 {
                    RcDoc::space()
                        .append(RcDoc::text("("))
                        .append(returns[0].to_rcdoc())
                        .append(RcDoc::text(")"))
                } else {
                    let return_docs: Vec<_> = returns.iter().map(|r| r.to_rcdoc()).collect();
                    RcDoc::space()
                        .append(RcDoc::text("("))
                        .append(RcDoc::intersperse(return_docs, RcDoc::text(", ")))
                        .append(RcDoc::text(")"))
                };

                RcDoc::text("func")
                    .append(RcDoc::space())
                    .append(params_doc)
                    .append(returns_doc)
            }
            GoExpression::Any => RcDoc::text("interface{}"),
            GoExpression::OptionalNullable(inner) => {
                RcDoc::text("optionalnullable.OptionalNullable[")
                    .append(inner.to_rcdoc())
                    .append(RcDoc::text("]"))
            }
        }
    }
}

impl GoExpression {
    /// Create a reference to a type
    pub fn reference(name: String) -> Self {
        GoExpression::Reference(name)
    }

    /// Create a pointer to a type
    pub fn pointer(inner: GoExpression) -> Self {
        GoExpression::Pointer(Box::new(inner))
    }

    /// Create a slice of a type
    pub fn slice(inner: GoExpression) -> Self {
        GoExpression::Slice(Box::new(inner))
    }

    /// Create a map type
    pub fn map(key: GoExpression, value: GoExpression) -> Self {
        GoExpression::Map {
            key: Box::new(key),
            value: Box::new(value),
        }
    }

    /// Create an OptionalNullable type
    pub fn optional_nullable(inner: GoExpression) -> Self {
        GoExpression::OptionalNullable(Box::new(inner))
    }
}
