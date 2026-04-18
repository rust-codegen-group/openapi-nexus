//! `IrTypeExpr` → `TsExpression` lowering used by API operation generation.
//!
//! Model rendering now goes through `sigil_emit`. API operation generation
//! still walks `IrTypeExpr` to build request / response / parameter type
//! expressions for minijinja contexts, so this small helper survives.

use std::collections::{BTreeMap, BTreeSet};

use heck::ToPascalCase as _;

use openapi_nexus_ir::types::{IrPrimitive, IrTypeExpr};

use crate::ast::{ObjectProperty, TsExpression, TsPrimitive};

/// Lower an `IrTypeExpr` to the TS AST expression used by API templates.
pub fn type_expr_to_ts(type_expr: &IrTypeExpr) -> TsExpression {
    match type_expr {
        IrTypeExpr::Named(name) => TsExpression::Reference(name.to_pascal_case()),
        IrTypeExpr::Primitive(p) => primitive_to_ts(p),
        IrTypeExpr::StringLiteral(val) => TsExpression::Literal(format!("\"{}\"", val)),
        IrTypeExpr::StringEnum(values) => {
            let mut union = BTreeSet::new();
            for v in values {
                union.insert(TsExpression::Literal(format!("\"{}\"", v)));
            }
            TsExpression::Union(union)
        }
        IrTypeExpr::Array(inner) => TsExpression::Array(Box::new(type_expr_to_ts(inner))),
        IrTypeExpr::Map(inner) => {
            let value_type = type_expr_to_ts(inner);
            let index_key = "[key: string]".to_string();
            let prop = ObjectProperty {
                ts_name: index_key.clone(),
                original_name: index_key.clone(),
                type_expr: value_type,
            };
            TsExpression::Object(BTreeMap::from([(index_key, prop)]))
        }
        IrTypeExpr::Nullable(inner) => {
            let inner_ts = type_expr_to_ts(inner);
            let mut union = BTreeSet::new();
            union.insert(inner_ts);
            union.insert(TsExpression::Primitive(TsPrimitive::Null));
            TsExpression::Union(union)
        }
        IrTypeExpr::Union(members) => {
            let ts_members: BTreeSet<TsExpression> = members.iter().map(type_expr_to_ts).collect();
            TsExpression::Union(ts_members)
        }
        IrTypeExpr::Any => TsExpression::Primitive(TsPrimitive::Any),
    }
}

fn primitive_to_ts(p: &IrPrimitive) -> TsExpression {
    match p {
        IrPrimitive::String
        | IrPrimitive::Date
        | IrPrimitive::DateTime
        | IrPrimitive::Uuid
        | IrPrimitive::Binary
        | IrPrimitive::StringWithFormat(_) => TsExpression::Primitive(TsPrimitive::String),
        IrPrimitive::Integer
        | IrPrimitive::Number
        | IrPrimitive::IntegerWithFormat(_)
        | IrPrimitive::NumberWithFormat(_) => TsExpression::Primitive(TsPrimitive::Number),
        IrPrimitive::Boolean => TsExpression::Primitive(TsPrimitive::Boolean),
    }
}
