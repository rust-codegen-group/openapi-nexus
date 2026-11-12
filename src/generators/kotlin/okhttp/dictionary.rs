//! Named pure dictionaries retain their wire object shape and declared values.

use heck::ToPascalCase;
use sigil_stitch::lang::kotlin::Kotlin;
use sigil_stitch::prelude::*;

use crate::generators::type_application;
use crate::ir::types::{IrSchema, IrTypeExpr};

use super::sigil_emit::{kotlin_type_name_in, package_header};

pub(super) fn file(
    schema: &IrSchema,
    values: &IrTypeExpr,
    package_name: &str,
) -> Result<FileSpec, SigilStitchError> {
    let name = schema.name.to_pascal_case();
    let models_package = format!("{package_name}.models");
    let root = TypeName::qualified(&models_package, &name);
    let value_type = kotlin_type_name_in(values, Some(&models_package));
    let adapter_name = format!("{name}JsonAdapter");
    let map = type_application(
        TypeName::qualified("kotlin.collections", "MutableMap"),
        vec![TypeName::qualified("kotlin", "String"), value_type.clone()],
    );
    // Gson's built-in map factory precedes @JsonAdapter, so a Map subtype would
    // bypass this codec and drop null entries. Keep explicit dictionary storage.
    let mut dictionary = TypeSpec::builder(&name, TypeKind::Struct)
        .visibility(Visibility::Public)
        .add_primary_constructor_param(
            ParameterSpec::builder("additionalProperties", map)
                .is_property()
                .default_value(
                    sigil_quote!(Kotlin { linkedMapOf() })?.__sigil_trim_trailing_newline(),
                )
                .build()?,
        )
        .annotate(
            AnnotationSpec::importable(TypeName::importable(
                "com.google.gson.annotations",
                "JsonAdapter",
            ))
            .args([format!("{adapter_name}::class"), "nullSafe = false".into()]),
        );
    if let Some(doc) = &schema.description {
        dictionary = dictionary.doc(doc);
    }
    let gson = TypeName::importable("com.google.gson", "Gson");
    let element = TypeName::importable("com.google.gson", "JsonElement");
    let exception = TypeName::importable("com.google.gson", "JsonParseException");
    let json_token = TypeName::importable("com.google.gson.stream", "JsonToken");
    let token = type_application(
        TypeName::importable("com.google.gson.reflect", "TypeToken"),
        vec![value_type.clone()],
    );
    // Delegate from the original reader: Gson's tree reader silently truncates
    // fractional/overflowing integer members that its streaming reader rejects.
    let read = FunSpec::builder("read").is_override()
        .add_param(ParameterSpec::of("reader", TypeName::importable("com.google.gson.stream", "JsonReader")))
        .returns(vec![TypeName::optional(root.clone())])
        .body(sigil_quote!(Kotlin {
            if (reader.peek() == $T(json_token.clone()).NULL) { reader.nextNull(); return null }
            if (reader.peek() != $T(json_token).BEGIN_OBJECT) { throw $T(exception)($S(format!("expected {name} dictionary object"))) }
            reader.beginObject()
            val result = $T(root.clone())()
            while (reader.hasNext()) {
                val key = reader.nextName()
                result.additionalProperties[key] = gson.fromJson(reader, object : $T(token.clone())() {}.type)
            }
            reader.endObject()
            return result
        })?).build()?;
    let mut value_functions = Vec::new();
    let value_function = encode_value(values, &models_package, &mut value_functions)?;
    let write = FunSpec::builder("write")
        .is_override()
        .add_param(ParameterSpec::of(
            "writer",
            TypeName::importable("com.google.gson.stream", "JsonWriter"),
        ))
        .add_param(ParameterSpec::of("value", TypeName::optional(root.clone())))
        .body(sigil_quote!(Kotlin {
            if (value == null) { writer.nullValue(); return }
            writer.beginObject()
            for (entry in value.additionalProperties.entries) {
                val tree = $N(value_function)(entry.value)
                writer.name(entry.key)
                val serializeNulls = writer.serializeNulls
                writer.serializeNulls = true
                try { gson.getAdapter($T(element)::class.java).write(writer, tree) }
                finally { writer.serializeNulls = serializeNulls }
            }
            writer.endObject()
        })?)
        .build()?;
    let mut adapter = TypeSpec::builder(&adapter_name, TypeKind::Class)
        .visibility(Visibility::Private)
        .extends(type_application(
            TypeName::importable("com.google.gson", "TypeAdapter"),
            vec![root],
        ))
        .add_field(
            FieldSpec::builder("gson", gson.clone())
                .visibility(Visibility::Private)
                .is_readonly()
                .initializer(sigil_quote!(Kotlin { $T(gson)() })?.__sigil_trim_trailing_newline())
                .build()?,
        )
        .add_method(read)
        .add_method(write);
    for function in value_functions {
        adapter = adapter.add_method(function);
    }
    FileSpec::builder_with("model.kt", Kotlin::new())
        .header(package_header(package_name)?)
        .add_type(dictionary.build()?)
        .add_type(adapter.build()?)
        .build()
}

/// Containers preserve their entries without forcing serializeNulls on ordinary
/// model codecs. Named members still serialize through their declared adapter.
fn encode_value(
    expr: &IrTypeExpr,
    models_package: &str,
    functions: &mut Vec<FunSpec>,
) -> Result<String, SigilStitchError> {
    let expr = expr.representation();
    if let IrTypeExpr::Nullable(inner) = expr {
        return encode_value(inner, models_package, functions);
    }
    let body = match expr {
        IrTypeExpr::Any | IrTypeExpr::Union(_) => sigil_quote!(Kotlin { return value })?,
        IrTypeExpr::Array(inner) => {
            let child = encode_value(inner, models_package, functions)?;
            let array = TypeName::importable("com.google.gson", "JsonArray");
            sigil_quote!(Kotlin {
                val result = $T(array)()
                for (item in value) { result.add($N(child)(item)) }
                return result
            })?
        }
        IrTypeExpr::Map(inner) => {
            let child = encode_value(inner, models_package, functions)?;
            let object = TypeName::importable("com.google.gson", "JsonObject");
            sigil_quote!(Kotlin {
                val result = $T(object)()
                for (entry in value.entries) { result.add(entry.key, $N(child)(entry.value)) }
                return result
            })?
        }
        _ => {
            let token = type_application(
                TypeName::importable("com.google.gson.reflect", "TypeToken"),
                vec![kotlin_type_name_in(expr, Some(models_package))],
            );
            sigil_quote!(Kotlin { return gson.toJsonTree(value, object : $T(token)() {}.type) })?
        }
    };
    let name = format!("dictionaryValue{}", functions.len());
    let null = TypeName::importable("com.google.gson", "JsonNull");
    functions.push(
        FunSpec::builder(&name)
            .visibility(Visibility::Private)
            .add_param(ParameterSpec::of(
                "value",
                TypeName::optional(kotlin_type_name_in(expr, Some(models_package))),
            ))
            .returns(vec![TypeName::importable("com.google.gson", "JsonElement")])
            .body(sigil_quote!(Kotlin {
                if (value == null) { return $T(null).INSTANCE }
                $C(body)
            })?)
            .build()?,
    );
    Ok(name)
}
