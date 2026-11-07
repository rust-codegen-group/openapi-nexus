//! Shared planning for OpenAPI-declared response-header accessors.
//!
//! Generated clients always retain the backend's complete native header
//! collection. These plans only add optional typed convenience accessors for
//! headers declared in the OpenAPI document.

use std::collections::{BTreeMap, HashSet};

use crate::ir::types::{
    IrEnumValueType, IrPrimitive, IrResponse, IrSchemaKind, IrSpec, IrTypeExpr,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseHeaderValueKind {
    String,
    Integer,
    Number,
    Boolean,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseHeaderPlan {
    pub wire_name: String,
    pub value_kind: ResponseHeaderValueKind,
}

/// Build collision-safe accessor names from a generator-specific preferred name.
///
/// Distinct legal HTTP header names can normalize to the same source-language
/// identifier (for example, `X-Foo` and `X_Foo`). The first keeps the preferred
/// name and later collisions receive a numeric suffix.
pub fn unique_response_header_accessor_names(
    headers: &[ResponseHeaderPlan],
    preferred_name: impl FnMut(&str) -> String,
) -> Vec<String> {
    let mut used = HashSet::new();
    unique_response_header_accessor_names_with_used(headers, preferred_name, &mut used)
}

/// Build accessor names while sharing a namespace with other header groups.
pub fn unique_response_header_accessor_names_with_used(
    headers: &[ResponseHeaderPlan],
    mut preferred_name: impl FnMut(&str) -> String,
    used: &mut HashSet<String>,
) -> Vec<String> {
    headers
        .iter()
        .map(|header| {
            let preferred = preferred_name(&header.wire_name);
            if used.insert(preferred.clone()) {
                return preferred;
            }
            for index in 2..=u32::MAX {
                let candidate = format!("{preferred}{index}");
                if used.insert(candidate.clone()) {
                    return candidate;
                }
            }
            unreachable!("response header accessor name space exhausted")
        })
        .collect()
}

/// Collect headers declared by the selected response entries.
///
/// Header names are merged case-insensitively. If different response entries
/// declare incompatible schemas for the same header, the accessor falls back
/// to a string so callers can still read the wire value without lossy coercion.
pub fn collect_response_headers<'a>(
    responses: impl IntoIterator<Item = &'a IrResponse>,
    ir: &IrSpec,
) -> Vec<ResponseHeaderPlan> {
    let mut headers: BTreeMap<String, ResponseHeaderPlan> = BTreeMap::new();
    for response in responses {
        for (wire_name, header) in &response.headers {
            let key = wire_name.to_ascii_lowercase();
            let value_kind = response_header_value_kind(&header.type_expr, ir);
            headers
                .entry(key)
                .and_modify(|existing| {
                    if existing.value_kind != value_kind {
                        existing.value_kind = ResponseHeaderValueKind::String;
                    }
                })
                .or_insert_with(|| ResponseHeaderPlan {
                    wire_name: wire_name.clone(),
                    value_kind,
                });
        }
    }
    headers.into_values().collect()
}

fn response_header_value_kind(expr: &IrTypeExpr, ir: &IrSpec) -> ResponseHeaderValueKind {
    response_header_value_kind_inner(expr, ir, &mut HashSet::new())
}

fn response_header_value_kind_inner(
    expr: &IrTypeExpr,
    ir: &IrSpec,
    seen: &mut HashSet<String>,
) -> ResponseHeaderValueKind {
    match expr {
        IrTypeExpr::ScalarEnum { representation, .. } => {
            response_header_value_kind_inner(representation, ir, seen)
        }
        IrTypeExpr::Primitive(IrPrimitive::Integer | IrPrimitive::IntegerWithFormat(_)) => {
            ResponseHeaderValueKind::Integer
        }
        IrTypeExpr::Primitive(IrPrimitive::Number | IrPrimitive::NumberWithFormat(_)) => {
            ResponseHeaderValueKind::Number
        }
        IrTypeExpr::Primitive(IrPrimitive::Boolean) => ResponseHeaderValueKind::Boolean,
        IrTypeExpr::Primitive(_)
        | IrTypeExpr::StringLiteral(_)
        | IrTypeExpr::StringEnum(_)
        | IrTypeExpr::Array(_)
        | IrTypeExpr::Map(_)
        | IrTypeExpr::Any => ResponseHeaderValueKind::String,
        IrTypeExpr::Nullable(inner) => response_header_value_kind_inner(inner, ir, seen),
        IrTypeExpr::Union(members) => {
            let mut kinds = members
                .iter()
                .map(|member| response_header_value_kind_inner(member, ir, seen));
            let Some(first) = kinds.next() else {
                return ResponseHeaderValueKind::String;
            };
            if kinds.all(|kind| kind == first) {
                first
            } else {
                ResponseHeaderValueKind::String
            }
        }
        IrTypeExpr::Named(name) => {
            if !seen.insert(name.clone()) {
                return ResponseHeaderValueKind::String;
            }
            let kind = match ir.schemas.get(name).map(|schema| &schema.kind) {
                Some(IrSchemaKind::Alias(inner)) => {
                    response_header_value_kind_inner(inner, ir, seen)
                }
                Some(IrSchemaKind::Enum(enumeration)) => match enumeration.value_type {
                    IrEnumValueType::Integer => ResponseHeaderValueKind::Integer,
                    IrEnumValueType::Number => ResponseHeaderValueKind::Number,
                    IrEnumValueType::String | IrEnumValueType::Mixed => {
                        ResponseHeaderValueKind::String
                    }
                },
                _ => ResponseHeaderValueKind::String,
            };
            seen.remove(name);
            kind
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accessor_names_are_unique_after_identifier_normalization() {
        let headers = vec![
            ResponseHeaderPlan {
                wire_name: "X-Foo".to_string(),
                value_kind: ResponseHeaderValueKind::String,
            },
            ResponseHeaderPlan {
                wire_name: "X-Foo-Header2".to_string(),
                value_kind: ResponseHeaderValueKind::String,
            },
            ResponseHeaderPlan {
                wire_name: "X_Foo".to_string(),
                value_kind: ResponseHeaderValueKind::String,
            },
        ];

        let names = unique_response_header_accessor_names(&headers, |wire_name| {
            format!("{}Header", wire_name.replace(['-', '_'], ""))
        });

        assert_eq!(names, ["XFooHeader", "XFooHeader2Header", "XFooHeader2"]);
    }

    #[test]
    fn accessor_names_can_share_a_generated_module_namespace() {
        let headers = vec![ResponseHeaderPlan {
            wire_name: "Bar".to_string(),
            value_kind: ResponseHeaderValueKind::String,
        }];
        let mut used = HashSet::new();

        let first = unique_response_header_accessor_names_with_used(
            &headers,
            |_| "getGetFooBarHeader".to_string(),
            &mut used,
        );
        let second = unique_response_header_accessor_names_with_used(
            &headers,
            |_| "getGetFooBarHeader".to_string(),
            &mut used,
        );

        assert_eq!(first, ["getGetFooBarHeader"]);
        assert_eq!(second, ["getGetFooBarHeader2"]);
    }
}
