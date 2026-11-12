//! Explicit typed extension storage without replacing ordinary Gson field codecs.

use std::collections::BTreeSet;

use heck::ToPascalCase;
use sigil_stitch::lang::java::Java;
use sigil_stitch::prelude::*;
use sigil_stitch::spec::modifiers::DeclarationContext;

use crate::generators::type_application;
use crate::ir::types::{IrObject, IrPrimitive, IrSchema, IrTypeExpr};

use super::util::{java_field_name, java_getter_name, java_primitive_boxed};

pub(super) struct AdditionalProperties<'a> {
    name: String,
    getter: String,
    object: &'a IrObject,
    values: &'a IrTypeExpr,
    package: String,
    root: TypeName,
    factory: String,
}

impl<'a> AdditionalProperties<'a> {
    pub fn new(schema: &IrSchema, object: &'a IrObject, package: &str) -> Option<Self> {
        let values = object.additional_properties.as_ref()?;
        let used = object
            .properties
            .values()
            .flat_map(|property| {
                [
                    java_field_name(&property.name),
                    java_getter_name(&property.name),
                ]
            })
            .collect::<BTreeSet<_>>();
        let mut name = "additionalProperties".to_string();
        let mut getter = "getAdditionalProperties".to_string();
        let mut suffix = 2;
        while used.contains(&name) || used.contains(&getter) {
            name = format!("additionalProperties{suffix}");
            getter = format!("getAdditionalProperties{suffix}");
            suffix += 1;
        }
        let class = schema.name.to_pascal_case();
        let package = format!("{package}.models");
        Some(Self {
            name,
            getter,
            object,
            values,
            root: TypeName::qualified(&package, &class),
            factory: format!("{class}AdditionalPropertiesAdapterFactory"),
            package,
        })
    }

    fn map_type(&self) -> TypeName {
        type_application(
            TypeName::qualified("java.util", "Map"),
            vec![
                TypeName::qualified("java.lang", "String"),
                value_type(self.values, &self.package),
            ],
        )
    }

    pub fn annotation(&self) -> AnnotationSpec {
        AnnotationSpec::importable(TypeName::qualified(
            "com.google.gson.annotations",
            "JsonAdapter",
        ))
        .args([
            format!("value = {}.class", self.factory),
            "nullSafe = false".into(),
        ])
    }

    pub fn storage(&self) -> Result<CodeBlock, SigilStitchError> {
        let linked = TypeName::qualified("java.util", "LinkedHashMap");
        let field = FieldSpec::builder(&self.name, self.map_type())
            .initializer(sigil_quote!(Java { new $T(linked)<>() })?.__sigil_trim_trailing_newline())
            .build()?
            .emit(&Java::new(), DeclarationContext::Member)?;
        // sigil-stitch 0.7.5 has no transient field modifier. Only the static
        // modifier prefix is outside FieldSpec; types/imports/initializer stay semantic.
        sigil_quote!(Java { private transient $C(field) })
    }

    pub fn getter(&self) -> Result<FunSpec, SigilStitchError> {
        FunSpec::builder(&self.getter).visibility(Visibility::Public)
            .returns(vec![self.map_type()])
            .body(sigil_quote!(Java {
                if (this.$N(&self.name) == null) {
                    this.$N(&self.name) = new $T(TypeName::qualified("java.util", "LinkedHashMap"))<>();
                }
                return this.$N(&self.name);
            })?).build()
    }

    pub fn factory(&self, package: &str) -> Result<TypeSpec, SigilStitchError> {
        let value = value_type(self.values, &self.package);
        let token = type_application(
            TypeName::qualified("com.google.gson.reflect", "TypeToken"),
            vec![value.clone()],
        );
        let parent = type_application(
            TypeName::qualified(
                &format!("{package}.runtime"),
                "AdditionalPropertiesAdapterFactory",
            ),
            vec![self.root.clone(), value.clone()],
        );
        let constructor = FunSpec::builder(&self.factory).visibility(Visibility::Public)
            .body(sigil_quote!(Java {
                super($T(self.root.clone()).class, new $T(token)() {}, $+
                    new $T(TypeName::qualified("java.lang", "String"))[] { $for(key in self.object.properties.keys(); separator = ", ") { $S(key) } });
            })?).build()?;
        let properties = FunSpec::builder("properties")
            .visibility(Visibility::Protected)
            .annotate(AnnotationSpec::new("Override"))
            .add_param(ParameterSpec::of("value", self.root.clone()))
            .returns(vec![self.map_type()])
            .body(sigil_quote!(Java { return value.$N(&self.getter)(); })?)
            .build()?;
        let mut helpers = Vec::new();
        let function = encode_value(self.values, &self.package, &mut helpers)?;
        let encode = FunSpec::builder("encode")
            .visibility(Visibility::Protected)
            .annotate(AnnotationSpec::new("Override"))
            .add_param(ParameterSpec::of(
                "gson",
                TypeName::qualified("com.google.gson", "Gson"),
            ))
            .add_param(ParameterSpec::of("value", value))
            .returns(vec![TypeName::qualified("com.google.gson", "JsonElement")])
            .body(sigil_quote!(Java { return $N(function)(gson, value); })?)
            .build()?;
        let mut factory = TypeSpec::builder(&self.factory, TypeKind::Class)
            .extends(parent)
            .add_method(constructor)
            .add_method(properties)
            .add_method(encode);
        for helper in helpers {
            factory = factory.add_method(helper);
        }
        factory.build()
    }
}

fn value_type(expr: &IrTypeExpr, package: &str) -> TypeName {
    match expr.representation() {
        IrTypeExpr::Named(name) => TypeName::qualified(package, &name.to_pascal_case()),
        IrTypeExpr::Primitive(IrPrimitive::Binary) => TypeName::array(TypeName::primitive("byte")),
        IrTypeExpr::Primitive(primitive) => {
            TypeName::qualified("java.lang", java_primitive_boxed(primitive))
        }
        IrTypeExpr::Array(inner) => type_application(
            TypeName::qualified("java.util", "List"),
            vec![value_type(inner, package)],
        ),
        IrTypeExpr::Map(inner) => type_application(
            TypeName::qualified("java.util", "Map"),
            vec![
                TypeName::qualified("java.lang", "String"),
                value_type(inner, package),
            ],
        ),
        IrTypeExpr::Nullable(inner) => value_type(inner, package),
        IrTypeExpr::StringLiteral(_) | IrTypeExpr::StringEnum(_) => {
            TypeName::qualified("java.lang", "String")
        }
        IrTypeExpr::Any | IrTypeExpr::Union(_) => {
            TypeName::qualified("com.google.gson", "JsonElement")
        }
        IrTypeExpr::ScalarEnum { .. } => unreachable!("scalar representation is unwrapped"),
    }
}

fn encode_value(
    expr: &IrTypeExpr,
    package: &str,
    functions: &mut Vec<FunSpec>,
) -> Result<String, SigilStitchError> {
    let expr = expr.representation();
    if let IrTypeExpr::Nullable(inner) = expr {
        return encode_value(inner, package, functions);
    }
    let body = match expr {
        IrTypeExpr::Any | IrTypeExpr::Union(_) => sigil_quote!(Java { return value; })?,
        IrTypeExpr::Array(inner) => {
            let child = encode_value(inner, package, functions)?;
            sigil_quote!(Java {
                var result = new $T(TypeName::qualified("com.google.gson", "JsonArray"))();
                for (var item : value) { result.add($N(child)(gson, item)); }
                return result;
            })?
        }
        IrTypeExpr::Map(inner) => {
            let child = encode_value(inner, package, functions)?;
            sigil_quote!(Java {
                var result = new $T(TypeName::qualified("com.google.gson", "JsonObject"))();
                for (var entry : value.entrySet()) { result.add(entry.getKey(), $N(child)(gson, entry.getValue())); }
                return result;
            })?
        }
        _ => {
            let token = type_application(
                TypeName::qualified("com.google.gson.reflect", "TypeToken"),
                vec![value_type(expr, package)],
            );
            sigil_quote!(Java { return gson.toJsonTree(value, new $T(token)() {}.getType()); })?
        }
    };
    let function = format!("additionalValue{}", functions.len());
    functions.push(FunSpec::builder(&function).visibility(Visibility::Private)
        .add_param(ParameterSpec::of("gson", TypeName::qualified("com.google.gson", "Gson")))
        .add_param(ParameterSpec::of("value", value_type(expr, package)))
        .returns(vec![TypeName::qualified("com.google.gson", "JsonElement")])
        .body(sigil_quote!(Java {
            if (value == null) { return $T(TypeName::qualified("com.google.gson", "JsonNull")).INSTANCE; }
            $C(body)
        })?).build()?);
    Ok(function)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::types::IrSchemaKind;
    use sigil_stitch::assert_rendered;

    #[test]
    fn transient_storage_qualifies_types_and_disambiguates_the_authored_getter() {
        let source = "openapi: 3.1.0\ninfo: {title: Storage, version: '1'}\npaths: {}\ncomponents:\n  schemas:\n    Model:\n      type: object\n      properties:\n        additionalProperties: {type: string}\n      additionalProperties: {type: integer}\n";
        let ir =
            crate::ir::lower::lower(crate::parser::parse_content_yaml(source).unwrap()).unwrap();
        let schema = &ir.schemas["Model"];
        let IrSchemaKind::Object(object) = &schema.kind else {
            panic!("object facts retained")
        };
        let plan = AdditionalProperties::new(schema, object, "com.example.sdk").unwrap();
        assert_eq!(plan.getter, "getAdditionalProperties2");
        assert_rendered!(
            Java::new(),
            width = 300,
            plan.storage().unwrap(),
            "private transient java.util.Map<java.lang.String, java.lang.Integer> additionalProperties2 = new java.util.LinkedHashMap<>();\n"
        );
    }
}
