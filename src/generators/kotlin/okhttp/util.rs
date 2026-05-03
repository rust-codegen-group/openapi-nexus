use std::collections::HashSet;

use crate::ir::types::{IrPrimitive, IrTypeExpr};
use heck::{ToLowerCamelCase, ToPascalCase};

pub fn kt_type_str(expr: &IrTypeExpr) -> String {
    match expr {
        IrTypeExpr::Named(name) => name.to_pascal_case(),
        IrTypeExpr::Primitive(p) => kt_primitive(p).to_string(),
        IrTypeExpr::Array(inner) => format!("List<{}>", kt_type_str(inner)),
        IrTypeExpr::Map(inner) => format!("Map<String, {}>", kt_type_str(inner)),
        IrTypeExpr::Nullable(inner) => format!("{}?", kt_type_str(inner)),
        IrTypeExpr::StringLiteral(_) | IrTypeExpr::StringEnum(_) => "String".to_string(),
        IrTypeExpr::Union(_) | IrTypeExpr::Any => "Any".to_string(),
    }
}

pub fn kt_primitive(p: &IrPrimitive) -> &'static str {
    match p {
        IrPrimitive::String | IrPrimitive::StringWithFormat(_) => "String",
        IrPrimitive::Date | IrPrimitive::DateTime => "String",
        IrPrimitive::Uuid => "String",
        IrPrimitive::Binary => "ByteArray",
        IrPrimitive::Integer => "Int",
        IrPrimitive::IntegerWithFormat(format) => match format.as_str() {
            "int64" => "Long",
            _ => "Int",
        },
        IrPrimitive::Number => "Double",
        IrPrimitive::NumberWithFormat(format) => match format.as_str() {
            "float" => "Float",
            _ => "Double",
        },
        IrPrimitive::Boolean => "Boolean",
    }
}

pub fn kt_field_name(name: &str) -> String {
    let camel = name.to_lower_camel_case();
    if camel.is_empty() {
        return "value".to_string();
    }
    if is_kotlin_reserved(&camel) {
        format!("`{camel}`")
    } else {
        camel
    }
}

pub fn kt_ident(name: &str) -> String {
    let camel = name.to_lower_camel_case();
    if camel.is_empty() {
        return "arg".to_string();
    }
    if is_kotlin_reserved(&camel) {
        format!("`{camel}`")
    } else {
        camel
    }
}

pub fn is_kotlin_reserved(name: &str) -> bool {
    matches!(
        name,
        "as" | "break"
            | "class"
            | "continue"
            | "do"
            | "else"
            | "false"
            | "for"
            | "fun"
            | "if"
            | "in"
            | "interface"
            | "is"
            | "null"
            | "object"
            | "package"
            | "return"
            | "super"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "typealias"
            | "typeof"
            | "val"
            | "var"
            | "when"
            | "while"
    )
}

pub fn escape_kt_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

pub fn unique_name(desired: &str, used: &mut HashSet<String>) -> String {
    if used.insert(desired.to_string()) {
        return desired.to_string();
    }
    for i in 2..=u32::MAX {
        let candidate = format!("{desired}{i}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!()
}

pub fn sanitize_operation_id(op_id: &str, method: &str, path: &str) -> String {
    if !op_id.is_empty() {
        return op_id.to_string();
    }
    let path_part: String = path
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    format!("{method}_{path_part}")
}

pub fn render_value_as_string(value_expr: &str, t: &IrTypeExpr) -> String {
    match t {
        IrTypeExpr::Primitive(
            IrPrimitive::String
            | IrPrimitive::Date
            | IrPrimitive::DateTime
            | IrPrimitive::Uuid
            | IrPrimitive::StringWithFormat(_),
        )
        | IrTypeExpr::StringLiteral(_)
        | IrTypeExpr::StringEnum(_) => value_expr.to_string(),
        IrTypeExpr::Primitive(IrPrimitive::Boolean)
        | IrTypeExpr::Primitive(IrPrimitive::Integer)
        | IrTypeExpr::Primitive(IrPrimitive::IntegerWithFormat(_))
        | IrTypeExpr::Primitive(IrPrimitive::Number)
        | IrTypeExpr::Primitive(IrPrimitive::NumberWithFormat(_)) => {
            format!("{value_expr}.toString()")
        }
        IrTypeExpr::Nullable(inner) => {
            let inner_str = render_value_as_string(value_expr, inner);
            if inner_str == value_expr {
                format!("{value_expr} ?: \"\"")
            } else {
                inner_str
            }
        }
        IrTypeExpr::Array(_) => format!("{value_expr}.joinToString(\",\")"),
        IrTypeExpr::Named(_) => format!("{value_expr}.toString()"),
        _ => format!("{value_expr}.toString()"),
    }
}
