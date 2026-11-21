use heck::{ToPascalCase, ToSnakeCase};
use sigil_stitch::prelude::TypeName;

use crate::generators::type_application;
use crate::ir::types::{IrPrimitive, IrTypeExpr};

pub fn python_type_name(expr: &IrTypeExpr) -> TypeName {
    python_type_name_with_current(expr, None)
}

pub(crate) fn python_type_name_for_schema(expr: &IrTypeExpr, current_schema: &str) -> TypeName {
    python_type_name_with_current(expr, Some(current_schema))
}

fn python_type_name_with_current(expr: &IrTypeExpr, current_schema: Option<&str>) -> TypeName {
    match expr {
        IrTypeExpr::ScalarEnum { representation, .. } => {
            python_type_name_with_current(representation, current_schema)
        }
        IrTypeExpr::Named(name) => {
            let py_name = name.to_pascal_case();
            if current_schema == Some(name.as_str()) {
                TypeName::primitive(&py_name)
            } else {
                let module = format!(".{}", name.to_snake_case());
                TypeName::importable(&module, &py_name)
            }
        }
        IrTypeExpr::Primitive(p) => python_primitive_type_name(p),
        IrTypeExpr::StringLiteral(value) => TypeName::string_literal(value.clone()),
        IrTypeExpr::StringEnum(values) => TypeName::union(
            values
                .iter()
                .cloned()
                .map(TypeName::string_literal)
                .collect(),
        ),
        IrTypeExpr::Array(inner) => type_application(
            TypeName::primitive("list"),
            vec![python_type_name_with_current(inner, current_schema)],
        ),
        IrTypeExpr::Map(inner) => type_application(
            TypeName::primitive("dict"),
            vec![
                TypeName::primitive("str"),
                python_type_name_with_current(inner, current_schema),
            ],
        ),
        IrTypeExpr::Union(members) => {
            if members.is_empty() {
                TypeName::importable("typing", "Any")
            } else {
                TypeName::union(
                    members
                        .iter()
                        .map(|member| python_type_name_with_current(member, current_schema))
                        .collect(),
                )
            }
        }
        IrTypeExpr::Nullable(inner) => {
            TypeName::optional(python_type_name_with_current(inner, current_schema))
        }
        IrTypeExpr::Any => TypeName::importable("typing", "Any"),
    }
}

fn python_primitive_type_name(p: &IrPrimitive) -> TypeName {
    match p {
        IrPrimitive::String | IrPrimitive::StringWithFormat(_) => TypeName::primitive("str"),
        IrPrimitive::Integer | IrPrimitive::IntegerWithFormat(_) => TypeName::primitive("int"),
        IrPrimitive::Number | IrPrimitive::NumberWithFormat(_) => TypeName::primitive("float"),
        IrPrimitive::Boolean => TypeName::primitive("bool"),
        IrPrimitive::Binary => TypeName::primitive("bytes"),
        IrPrimitive::Date => TypeName::importable("datetime", "date"),
        IrPrimitive::DateTime => TypeName::importable("datetime", "datetime"),
        IrPrimitive::Uuid => TypeName::importable("uuid", "UUID"),
    }
}
