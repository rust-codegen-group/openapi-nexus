use std::collections::HashSet;

use crate::ir::types::{IrPrimitive, IrTypeExpr};
use heck::{ToLowerCamelCase, ToPascalCase};
#[allow(unused_imports)]
use sigil_stitch::lang::java::Java;
use sigil_stitch::prelude::*;

pub fn java_type_str(expr: &IrTypeExpr) -> String {
    match expr {
        IrTypeExpr::Named(name) => name.to_pascal_case(),
        IrTypeExpr::Primitive(p) => java_primitive(p).to_string(),
        IrTypeExpr::Array(inner) => format!("List<{}>", java_boxed_type_str(inner)),
        IrTypeExpr::Map(inner) => format!("Map<String, {}>", java_boxed_type_str(inner)),
        IrTypeExpr::Nullable(inner) => java_boxed_type_str(inner),
        IrTypeExpr::StringLiteral(_) | IrTypeExpr::StringEnum(_) => "String".to_string(),
        IrTypeExpr::Union(_) | IrTypeExpr::Any => "Object".to_string(),
    }
}

pub fn java_boxed_type_str(expr: &IrTypeExpr) -> String {
    match expr {
        IrTypeExpr::Primitive(p) => java_primitive_boxed(p).to_string(),
        _ => java_type_str(expr),
    }
}

pub fn java_primitive(p: &IrPrimitive) -> &'static str {
    match p {
        IrPrimitive::String | IrPrimitive::StringWithFormat(_) => "String",
        IrPrimitive::Date | IrPrimitive::DateTime => "String",
        IrPrimitive::Uuid => "String",
        IrPrimitive::Binary => "byte[]",
        IrPrimitive::Integer => "int",
        IrPrimitive::IntegerWithFormat(format) => match format.as_str() {
            "int64" => "long",
            _ => "int",
        },
        IrPrimitive::Number => "double",
        IrPrimitive::NumberWithFormat(format) => match format.as_str() {
            "float" => "float",
            _ => "double",
        },
        IrPrimitive::Boolean => "boolean",
    }
}

pub fn java_primitive_boxed(p: &IrPrimitive) -> &'static str {
    match p {
        IrPrimitive::String | IrPrimitive::StringWithFormat(_) => "String",
        IrPrimitive::Date | IrPrimitive::DateTime => "String",
        IrPrimitive::Uuid => "String",
        IrPrimitive::Binary => "byte[]",
        IrPrimitive::Integer => "Integer",
        IrPrimitive::IntegerWithFormat(format) => match format.as_str() {
            "int64" => "Long",
            _ => "Integer",
        },
        IrPrimitive::Number => "Double",
        IrPrimitive::NumberWithFormat(format) => match format.as_str() {
            "float" => "Float",
            _ => "Double",
        },
        IrPrimitive::Boolean => "Boolean",
    }
}

pub fn java_field_name(name: &str) -> String {
    let camel = name.to_lower_camel_case();
    if camel.is_empty() {
        return "value".to_string();
    }
    if is_java_reserved(&camel) {
        format!("{camel}_")
    } else {
        camel
    }
}

pub fn java_ident(name: &str) -> String {
    let camel = name.to_lower_camel_case();
    if camel.is_empty() {
        return "arg".to_string();
    }
    if is_java_reserved(&camel) {
        format!("{camel}_")
    } else {
        camel
    }
}

pub fn is_java_reserved(name: &str) -> bool {
    matches!(
        name,
        "abstract"
            | "assert"
            | "boolean"
            | "break"
            | "byte"
            | "case"
            | "catch"
            | "char"
            | "class"
            | "const"
            | "continue"
            | "default"
            | "do"
            | "double"
            | "else"
            | "enum"
            | "extends"
            | "final"
            | "finally"
            | "float"
            | "for"
            | "goto"
            | "if"
            | "implements"
            | "import"
            | "instanceof"
            | "int"
            | "interface"
            | "long"
            | "native"
            | "new"
            | "package"
            | "private"
            | "protected"
            | "public"
            | "return"
            | "short"
            | "static"
            | "strictfp"
            | "super"
            | "switch"
            | "synchronized"
            | "this"
            | "throw"
            | "throws"
            | "transient"
            | "try"
            | "void"
            | "volatile"
            | "while"
    )
}

pub fn escape_java_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
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
            format!("String.valueOf({value_expr})")
        }
        IrTypeExpr::Nullable(inner) => render_value_as_string(value_expr, inner),
        IrTypeExpr::Array(inner) => {
            if matches!(
                inner.as_ref(),
                IrTypeExpr::Primitive(
                    IrPrimitive::String
                        | IrPrimitive::Date
                        | IrPrimitive::DateTime
                        | IrPrimitive::Uuid
                        | IrPrimitive::StringWithFormat(_)
                ) | IrTypeExpr::StringLiteral(_)
                    | IrTypeExpr::StringEnum(_)
            ) {
                format!("String.join(\",\", {value_expr})")
            } else {
                format!("String.valueOf({value_expr})")
            }
        }
        IrTypeExpr::Named(_) => format!("String.valueOf({value_expr})"),
        _ => format!("String.valueOf({value_expr})"),
    }
}

pub fn build_java_getter(getter_name: &str, type_str: &str, field_name: &str) -> FunSpec {
    let mut getter = FunSpec::builder(getter_name);
    getter = getter.visibility(Visibility::Public);
    getter = getter.returns(TypeName::primitive(type_str));
    let body = sigil_quote!(Java {
        return this.$L(field_name);
    })
    .expect("getter body");
    getter = getter.body(body);
    getter.build().expect("getter")
}

pub fn type_uses_list(expr: &IrTypeExpr) -> bool {
    match expr {
        IrTypeExpr::Array(_) => true,
        IrTypeExpr::Nullable(inner) => type_uses_list(inner),
        IrTypeExpr::Map(inner) => type_uses_list(inner),
        _ => false,
    }
}

pub fn type_uses_map(expr: &IrTypeExpr) -> bool {
    match expr {
        IrTypeExpr::Map(_) => true,
        IrTypeExpr::Nullable(inner) => type_uses_map(inner),
        IrTypeExpr::Array(inner) => type_uses_map(inner),
        _ => false,
    }
}
