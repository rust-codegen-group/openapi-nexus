//! Mixed objects retain explicit extensions without replacing named-field codecs.

use std::collections::BTreeSet;

use heck::ToPascalCase;
use sigil_stitch::prelude::*;

use crate::generators::type_application;
use crate::ir::types::{IrObject, IrSchema, IrSpec, IrTypeExpr};

use super::sigil_emit::kotlin_type_name_in;
use super::util::kt_decl_name;

pub(super) struct AdditionalProperties<'a> {
    name: String,
    object: &'a IrObject,
    values: &'a IrTypeExpr,
    package: String,
    root: TypeName,
    factory: String,
}

impl<'a> AdditionalProperties<'a> {
    pub fn new(
        schema: &IrSchema,
        object: &'a IrObject,
        package: &str,
        ir: &IrSpec,
    ) -> Option<Self> {
        let values = object.additional_properties.as_ref()?;
        let used = object
            .properties
            .values()
            .map(|property| kt_decl_name(&property.name))
            .collect::<BTreeSet<_>>();
        let mut name = "additionalProperties".to_string();
        let mut suffix = 2;
        while used.contains(&name) {
            name = format!("additionalProperties{suffix}");
            suffix += 1;
        }
        let class = schema.name.to_pascal_case();
        let base_factory = format!("{class}AdditionalPropertiesAdapterFactory");
        let mut factory = base_factory.clone();
        let mut suffix = 2;
        while ir
            .schemas
            .values()
            .any(|schema| schema.name.to_pascal_case() == factory)
        {
            factory = format!("{base_factory}{suffix}");
            suffix += 1;
        }
        let package = format!("{package}.models");
        Some(Self {
            name,
            object,
            values,
            root: TypeName::qualified(&package, &class),
            factory,
            package,
        })
    }

    fn map_type(&self) -> TypeName {
        type_application(
            TypeName::qualified("kotlin.collections", "MutableMap"),
            vec![
                TypeName::qualified("kotlin", "String"),
                value_type(self.values, &self.package),
            ],
        )
    }

    pub fn annotation(&self) -> AnnotationSpec {
        // AnnotationSpec accepts only textual arguments; keep this class-literal
        // spelling bounded here. The helper-name golden fixes its final shape.
        AnnotationSpec::importable(TypeName::qualified(
            "com.google.gson.annotations",
            "JsonAdapter",
        ))
        .args([
            format!("{}::class", self.factory),
            "nullSafe = false".into(),
        ])
    }

    pub fn parameter(&self) -> Result<ParameterSpec, SigilStitchError> {
        ParameterSpec::builder(&self.name, self.map_type())
            .is_mutable_property()
            .annotate_storage(AnnotationSpec::importable(TypeName::qualified(
                "kotlin.jvm",
                "Transient",
            )))
            .default_value(sigil_quote!(Kotlin { linkedMapOf() })?.__sigil_trim_trailing_newline())
            .build()
    }

    pub fn legacy_constructor(&self) -> Result<FunSpec, SigilStitchError> {
        let mut constructor = FunSpec::builder("constructor").is_constructor();
        let fields = self
            .object
            .properties
            .values()
            .map(|property| kt_decl_name(&property.name))
            .collect::<Vec<_>>();
        for property in self.object.properties.values() {
            let ty = named_field_type(&property.type_expr, &self.package);
            let ty = if property.required && !property.nullable {
                ty
            } else {
                optional(ty)
            };
            constructor =
                constructor.add_param(ParameterSpec::of(&kt_decl_name(&property.name), ty));
        }
        constructor
            .delegation(
                sigil_quote!(Kotlin {
                    this($for(field in &fields; separator = ", ") { $N(field) }, linkedMapOf())
                })?
                .__sigil_trim_trailing_newline(),
            )
            .body(sigil_quote!(Kotlin {})?)
            .build()
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
        let constructor = FunSpec::builder("constructor").is_constructor()
            .delegation(sigil_quote!(Kotlin {
                super($T(self.root.clone())::class.java, object : $T(token)() {}, $for(key in self.object.properties.keys(); separator = ", ") { $S(key) })
            })?.__sigil_trim_trailing_newline())
            .body(sigil_quote!(Kotlin {})?).build()?;
        let properties = FunSpec::builder("properties")
            .is_override()
            .add_param(ParameterSpec::of("value", self.root.clone()))
            .returns(vec![self.map_type()])
            .body(sigil_quote!(Kotlin { return value.$N(&self.name) })?)
            .build()?;
        let setter = FunSpec::builder("setProperties")
            .is_override()
            .add_param(ParameterSpec::of("value", self.root.clone()))
            .add_param(ParameterSpec::of("entries", self.map_type()))
            .body(sigil_quote!(Kotlin { value.$N(&self.name) = entries })?)
            .build()?;
        let mut helpers = Vec::new();
        let function = encode_value(self.values, &self.package, &mut helpers)?;
        let encode = FunSpec::builder("encode")
            .is_override()
            .add_param(ParameterSpec::of(
                "gson",
                TypeName::qualified("com.google.gson", "Gson"),
            ))
            .add_param(ParameterSpec::of("value", optional(value)))
            .returns(vec![TypeName::qualified("com.google.gson", "JsonElement")])
            .body(sigil_quote!(Kotlin { return $N(function)(gson, value) })?)
            .build()?;
        let mut factory = TypeSpec::builder(&self.factory, TypeKind::Class)
            .visibility(Visibility::Private)
            .extends(parent)
            .add_method(constructor)
            .add_method(properties)
            .add_method(setter)
            .add_method(encode);
        for helper in helpers {
            factory = factory.add_method(helper);
        }
        factory.build()
    }
}

fn optional(ty: TypeName) -> TypeName {
    if matches!(ty, TypeName::Optional(_)) {
        ty
    } else {
        TypeName::optional(ty)
    }
}

/// Ordinary fields historically use Any, not the extension storage's JsonElement.
fn named_field_type(expr: &IrTypeExpr, package: &str) -> TypeName {
    match expr.representation() {
        IrTypeExpr::Any | IrTypeExpr::Union(_) => TypeName::qualified("kotlin", "Any"),
        IrTypeExpr::Array(inner) => type_application(
            TypeName::qualified("kotlin.collections", "List"),
            vec![named_field_type(inner, package)],
        ),
        IrTypeExpr::Map(inner) => type_application(
            TypeName::qualified("kotlin.collections", "Map"),
            vec![
                TypeName::qualified("kotlin", "String"),
                named_field_type(inner, package),
            ],
        ),
        IrTypeExpr::Nullable(inner) => optional(named_field_type(inner, package)),
        _ => kotlin_type_name_in(expr, Some(package)),
    }
}

fn value_type(expr: &IrTypeExpr, package: &str) -> TypeName {
    match expr.representation() {
        IrTypeExpr::Any | IrTypeExpr::Union(_) => {
            TypeName::qualified("com.google.gson", "JsonElement")
        }
        IrTypeExpr::Array(inner) => type_application(
            TypeName::qualified("kotlin.collections", "List"),
            vec![value_type(inner, package)],
        ),
        IrTypeExpr::Map(inner) => type_application(
            TypeName::qualified("kotlin.collections", "Map"),
            vec![
                TypeName::qualified("kotlin", "String"),
                value_type(inner, package),
            ],
        ),
        IrTypeExpr::Nullable(inner) => optional(value_type(inner, package)),
        _ => kotlin_type_name_in(expr, Some(package)),
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
        IrTypeExpr::Any | IrTypeExpr::Union(_) => sigil_quote!(Kotlin { return value })?,
        IrTypeExpr::Array(inner) => {
            let child = encode_value(inner, package, functions)?;
            sigil_quote!(Kotlin {
                val result = $T(TypeName::qualified("com.google.gson", "JsonArray"))()
                for (item in value) { result.add($N(child)(gson, item)) }
                return result
            })?
        }
        IrTypeExpr::Map(inner) => {
            let child = encode_value(inner, package, functions)?;
            sigil_quote!(Kotlin {
                val result = $T(TypeName::qualified("com.google.gson", "JsonObject"))()
                for (entry in value.entries) { result.add(entry.key, $N(child)(gson, entry.value)) }
                return result
            })?
        }
        _ => {
            let token = type_application(
                TypeName::qualified("com.google.gson.reflect", "TypeToken"),
                vec![value_type(expr, package)],
            );
            sigil_quote!(Kotlin { return gson.toJsonTree(value, object : $T(token)() {}.type) })?
        }
    };
    let function = format!("additionalValue{}", functions.len());
    functions.push(FunSpec::builder(&function).visibility(Visibility::Private)
        .add_param(ParameterSpec::of("gson", TypeName::qualified("com.google.gson", "Gson")))
        .add_param(ParameterSpec::of("value", optional(value_type(expr, package))))
        .returns(vec![TypeName::qualified("com.google.gson", "JsonElement")])
        .body(sigil_quote!(Kotlin {
            if (value == null) { return $T(TypeName::qualified("com.google.gson", "JsonNull")).INSTANCE }
            $C(body)
        })?).build()?);
    Ok(function)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::types::IrSchemaKind;
    use sigil_stitch::{assert_rendered, lang::kotlin::Kotlin};

    #[test]
    fn mixed_storage_and_legacy_constructor_render_without_renaming_authored_members() {
        let source = "openapi: 3.1.0\ninfo: {title: Storage, version: '1'}\npaths: {}\ncomponents:\n  schemas:\n    Model:\n      type: object\n      required: [additionalProperties]\n      properties:\n        additionalProperties: {type: string}\n      additionalProperties: {type: integer}\n";
        let ir =
            crate::ir::lower::lower(crate::parser::parse_content_yaml(source).unwrap()).unwrap();
        let schema = &ir.schemas["Model"];
        let IrSchemaKind::Object(object) = &schema.kind else {
            panic!("object facts retained")
        };
        let plan = AdditionalProperties::new(schema, object, "com.example.sdk", &ir).unwrap();
        let declaration = TypeSpec::builder("Model", TypeKind::Struct)
            .visibility(Visibility::Public)
            .add_primary_constructor_param(
                ParameterSpec::builder(
                    "additionalProperties",
                    TypeName::qualified("kotlin", "String"),
                )
                .is_property()
                .build()
                .unwrap(),
            )
            .add_primary_constructor_param(plan.parameter().unwrap())
            .add_method(plan.legacy_constructor().unwrap())
            .build()
            .unwrap()
            .emit(&Kotlin::new())
            .unwrap();
        let block = sigil_quote!(Kotlin { $C_each(declaration) }).unwrap();
        assert_rendered!(
            Kotlin::new(),
            width = 300,
            block,
            "data class Model(val additionalProperties: kotlin.String, @field:kotlin.jvm.Transient\n    var additionalProperties2: kotlin.collections.MutableMap<kotlin.String, kotlin.Int> = linkedMapOf()) {\n    constructor(additionalProperties: kotlin.String) : this(additionalProperties, linkedMapOf()) {\n\n    }\n}\n"
        );
    }
}
