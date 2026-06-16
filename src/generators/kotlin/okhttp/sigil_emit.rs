use std::collections::HashSet;

use crate::codegen::traits::file_writer::FileInfo;
use crate::generators::request_inputs::{
    RequestInputField, RequestInputFieldKind, RequestInputModel, RequestInputPlan,
};
use crate::ir::types::{
    IrEnum, IrEnumValueType, IrIntersection, IrObject, IrSchema, IrSchemaKind, IrSpec,
    IrTaggedUnion, IrTypeExpr, IrUnion, TaggingStyle,
};
use heck::{ToLowerCamelCase, ToPascalCase};
use sigil_stitch::lang::kotlin::Kotlin;
use sigil_stitch::prelude::*;

use super::util::{escape_kt_string, kt_field_name, kt_type_str, unique_name};

const RENDER_WIDTH: usize = 100;

pub fn generate_model_files(
    ir: &IrSpec,
    package_name: &str,
    header: &str,
    request_inputs: &RequestInputPlan,
) -> Result<Vec<FileInfo>, String> {
    let mut files = Vec::new();
    for (_name, schema) in &ir.schemas {
        let body = emit_model_body(schema, package_name).ok_or_else(|| {
            format!(
                "unsupported schema kind for {}: {:?}",
                schema.name, schema.kind
            )
        })?;
        let class_name = schema.name.to_pascal_case();
        let filename = format!("{class_name}.kt");
        let mut content = String::with_capacity(header.len() + body.len());
        content.push_str(header);
        content.push_str(&body);
        files.push(FileInfo::model(filename, content));
    }
    for model in request_inputs.models() {
        files.push(request_input_model_file(model, package_name, header));
    }
    Ok(files)
}

fn request_input_model_file(
    model: &RequestInputModel,
    package_name: &str,
    header: &str,
) -> FileInfo {
    let class_name = model.name.to_pascal_case();
    let needs_upload = model.fields.iter().any(RequestInputField::is_upload);
    let mut content = String::new();
    content.push_str(header);
    content.push_str(&format!("package {package_name}.models\n\n"));
    if needs_upload {
        content.push_str(&format!("import {package_name}.runtime.UploadFile\n\n"));
    }
    content.push_str(&format!("data class {class_name}(\n"));
    for field in &model.fields {
        let field_name = kt_field_name(&field.wire_name);
        let mut field_type = request_input_kt_type(field);
        let default = if field.required {
            ""
        } else {
            if !field_type.ends_with('?') {
                field_type.push('?');
            }
            " = null"
        };
        content.push_str(&format!("    val {field_name}: {field_type}{default},\n"));
    }
    content.push_str(")\n");

    FileInfo::model(format!("{class_name}.kt"), content)
}

fn request_input_kt_type(field: &RequestInputField) -> String {
    match field.kind {
        RequestInputFieldKind::UploadFile { .. } => "UploadFile".to_string(),
        RequestInputFieldKind::SchemaValue => kt_type_str(&field.type_expr),
    }
}

fn emit_model_body(schema: &IrSchema, package_name: &str) -> Option<String> {
    let file_spec = match &schema.kind {
        IrSchemaKind::Object(obj) => emit_object(schema, obj, package_name),
        IrSchemaKind::Enum(en) => emit_enum(schema, en, package_name),
        IrSchemaKind::Alias(expr) => emit_alias(schema, expr, package_name),
        IrSchemaKind::Union(u) => emit_union(schema, u, package_name),
        IrSchemaKind::Intersection(i) => emit_intersection(schema, i, package_name),
        IrSchemaKind::TaggedUnion(tu) => emit_tagged_union(schema, tu, package_name),
    }?;
    file_spec.render(RENDER_WIDTH).ok()
}

fn package_header(package_name: &str) -> CodeBlock {
    sigil_quote!(Kotlin {
        package $L(format!("{package_name}.models"))
    })
    .expect("package header builds")
}

// ---------------------------------------------------------------------------
// Object -> data class with primary constructor
// ---------------------------------------------------------------------------

fn emit_object(schema: &IrSchema, obj: &IrObject, package_name: &str) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();

    let mut file =
        FileSpec::builder_with("model.kt", Kotlin::new()).header(package_header(package_name));

    let needs_serialized_name = obj.properties.iter().any(|(json_name, prop)| {
        let field_name = kt_field_name(&prop.name);
        *json_name != field_name
    });
    if needs_serialized_name {
        file = file.add_import(ImportSpec::named(
            "com.google.gson.annotations",
            "SerializedName",
        ));
    }

    let mut tb = TypeSpec::builder(&name, TypeKind::Struct).visibility(Visibility::Public);
    if let Some(doc) = &schema.description {
        tb = tb.doc(doc);
    }

    for (json_name, prop) in &obj.properties {
        let field_name = kt_field_name(&prop.name);
        let required = prop.required && !prop.nullable;
        let type_str = if required {
            kt_type_str(&prop.type_expr)
        } else {
            format!("{}?", kt_type_str(&prop.type_expr))
        };

        let annotation = if *json_name != field_name {
            format!("@SerializedName(\"{}\") ", escape_kt_string(json_name))
        } else {
            String::new()
        };

        let default = if !required { " = null" } else { "" };

        let param_str = format!("{annotation}val {field_name}: {type_str}{default}");
        tb = tb.add_primary_constructor_param(
            ParameterSpec::new(&param_str, TypeName::primitive("")).expect("param"),
        );
    }

    file = file.add_type(tb.build().ok()?);
    file.build().ok()
}

// ---------------------------------------------------------------------------
// Enum -> enum class
// ---------------------------------------------------------------------------

fn emit_enum(schema: &IrSchema, en: &IrEnum, package_name: &str) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();

    let mut file =
        FileSpec::builder_with("model.kt", Kotlin::new()).header(package_header(package_name));

    if en.value_type == IrEnumValueType::Mixed {
        return emit_typealias(&name, "Any", schema.description.as_deref(), package_name);
    }

    file = file.add_import(ImportSpec::named(
        "com.google.gson.annotations",
        "SerializedName",
    ));

    let base_type = match en.value_type {
        IrEnumValueType::String => "String",
        IrEnumValueType::Integer => "Int",
        IrEnumValueType::Number => "Double",
        IrEnumValueType::Mixed => unreachable!(),
    };

    let mut tb = TypeSpec::builder(&name, TypeKind::Enum).visibility(Visibility::Public);
    if let Some(doc) = &schema.description {
        tb = tb.doc(doc);
    }

    tb = tb.add_primary_constructor_param(
        ParameterSpec::new(&format!("val value: {base_type}"), TypeName::primitive(""))
            .expect("enum param"),
    );

    for v in &en.values {
        let (variant_name, literal, raw_value) = match en.value_type {
            IrEnumValueType::String => {
                let s = v.value.as_str()?;
                (
                    s.to_pascal_case().replace(['-', ' '], ""),
                    format!("\"{}\"", escape_kt_string(s)),
                    s.to_string(),
                )
            }
            IrEnumValueType::Integer | IrEnumValueType::Number => {
                let n = v.value.as_number()?;
                let variant = format!("N{}", n.to_string().replace(['-', '.'], "_"));
                let s = n.to_string();
                (variant, s.clone(), s)
            }
            IrEnumValueType::Mixed => unreachable!(),
        };

        let variant_name = if variant_name.is_empty() {
            "UNKNOWN".to_string()
        } else {
            variant_name
        };

        let mut ev = EnumVariantSpec::builder(&variant_name);
        ev = ev.annotation(
            CodeBlock::of(
                &format!("@SerializedName(\"{}\")", escape_kt_string(&raw_value)),
                (),
            )
            .expect("annotation"),
        );
        ev = ev.value(CodeBlock::of(&literal, ()).expect("literal"));
        tb = tb.add_variant(ev.build().expect("enum variant"));
    }

    file = file.add_type(tb.build().ok()?);
    file.build().ok()
}

// ---------------------------------------------------------------------------
// Alias -> typealias
// ---------------------------------------------------------------------------

fn emit_alias(schema: &IrSchema, expr: &IrTypeExpr, package_name: &str) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();
    let rhs = kt_type_str(expr);
    emit_typealias(&name, &rhs, schema.description.as_deref(), package_name)
}

// ---------------------------------------------------------------------------
// Union -> typealias to Any
// ---------------------------------------------------------------------------

fn emit_union(schema: &IrSchema, _union: &IrUnion, package_name: &str) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();
    emit_typealias(&name, "Any", schema.description.as_deref(), package_name)
}

// ---------------------------------------------------------------------------
// Intersection -> data class with all merged properties
// ---------------------------------------------------------------------------

fn emit_intersection(
    schema: &IrSchema,
    inter: &IrIntersection,
    package_name: &str,
) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();
    let mut file =
        FileSpec::builder_with("model.kt", Kotlin::new()).header(package_header(package_name));

    let mut tb = TypeSpec::builder(&name, TypeKind::Struct).visibility(Visibility::Public);
    if let Some(doc) = &schema.description {
        tb = tb.doc(doc);
    }

    let mut used_names: HashSet<String> = HashSet::new();
    for member in &inter.members {
        let member_type = kt_type_str(member);
        let field_name = unique_name(&member_type.to_lower_camel_case(), &mut used_names);
        tb = tb.add_primary_constructor_param(
            ParameterSpec::new(
                &format!("val {field_name}: {member_type}"),
                TypeName::primitive(""),
            )
            .expect("param"),
        );
    }

    file = file.add_type(tb.build().ok()?);
    file.build().ok()
}

// ---------------------------------------------------------------------------
// TaggedUnion -> sealed class
// ---------------------------------------------------------------------------

fn emit_tagged_union(
    schema: &IrSchema,
    tu: &IrTaggedUnion,
    package_name: &str,
) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();

    if tu.variants.is_empty() {
        return emit_typealias(&name, "Any", schema.description.as_deref(), package_name);
    }

    let hint = match &tu.tagging {
        TaggingStyle::Internal => {
            format!("Discriminator: {} (internal).", tu.discriminator_field)
        }
        TaggingStyle::Adjacent { content_field } => format!(
            "Discriminator: {} / content: {} (adjacent).",
            tu.discriminator_field, content_field
        ),
        TaggingStyle::External => "Discriminator: variant key (external).".to_string(),
    };
    let doc = match &schema.description {
        Some(desc) => format!("{desc}\n\n{hint}"),
        None => hint,
    };

    // Sealed classes with variant subtypes don't map cleanly to sigil-stitch's
    // type builder (no nested type support). Emit as a typealias to Any with
    // documentation describing the discriminator pattern.
    emit_typealias(&name, "Any", Some(&doc), package_name)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn emit_typealias(
    name: &str,
    rhs: &str,
    doc: Option<&str>,
    package_name: &str,
) -> Option<FileSpec> {
    let mut file =
        FileSpec::builder_with("model.kt", Kotlin::new()).header(package_header(package_name));

    let mut tb = TypeSpec::builder(name, TypeKind::TypeAlias).visibility(Visibility::Public);
    if let Some(d) = doc {
        tb = tb.doc(d);
    }
    tb = tb.extends(TypeName::primitive(rhs));

    file = file.add_type(tb.build().ok()?);
    file.build().ok()
}
