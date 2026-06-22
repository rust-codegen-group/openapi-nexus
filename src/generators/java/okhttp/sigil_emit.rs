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
use sigil_stitch::lang::java::Java;
use sigil_stitch::prelude::*;

use super::util::{
    build_java_getter, escape_java_string, java_boxed_type_str, java_field_name, java_type_str,
    type_uses_list, type_uses_map, unique_name,
};

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
        let filename = format!("{class_name}.java");
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
    content.push_str(&format!("package {package_name}.models;\n\n"));
    if needs_upload {
        content.push_str(&format!("import {package_name}.runtime.UploadFile;\n\n"));
    }
    content.push_str(&format!("public final class {class_name} {{\n"));
    for field in &model.fields {
        content.push_str(&format!(
            "    private final {} {};\n",
            request_input_java_type(field),
            java_field_name(&field.wire_name)
        ));
    }
    content.push('\n');
    content.push_str(&format!("    public {class_name}("));
    let params = model
        .fields
        .iter()
        .map(|field| {
            format!(
                "{} {}",
                request_input_java_type(field),
                java_field_name(&field.wire_name)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    content.push_str(&params);
    content.push_str(") {\n");
    for field in &model.fields {
        let name = java_field_name(&field.wire_name);
        content.push_str(&format!("        this.{name} = {name};\n"));
    }
    content.push_str("    }\n\n");
    for field in &model.fields {
        let field_name = java_field_name(&field.wire_name);
        let getter = format!("get{}", field.wire_name.to_pascal_case());
        content.push_str(&format!(
            "    public {} {}() {{\n        return {};\n    }}\n\n",
            request_input_java_type(field),
            getter,
            field_name
        ));
    }
    content.push_str("}\n");

    FileInfo::model(format!("{class_name}.java"), content)
}

fn request_input_java_type(field: &RequestInputField) -> String {
    match field.kind {
        RequestInputFieldKind::UploadFile { .. } => "UploadFile".to_string(),
        RequestInputFieldKind::SchemaValue => java_boxed_type_str(&field.type_expr),
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
    sigil_quote!(Java {
        package $L(format!("{package_name}.models"));
    })
    .expect("package header builds")
}

// ---------------------------------------------------------------------------
// Object -> class with private fields, constructor, getters
// ---------------------------------------------------------------------------

fn emit_object(schema: &IrSchema, obj: &IrObject, package_name: &str) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();

    let mut file =
        FileSpec::builder_with("model.java", Java::new()).header(package_header(package_name));

    let needs_serialized_name = obj.properties.iter().any(|(json_name, prop)| {
        let field_name = java_field_name(&prop.name);
        *json_name != field_name
    });
    if needs_serialized_name {
        file = file.add_import(ImportSpec::named(
            "com.google.gson.annotations",
            "SerializedName",
        ));
    }

    let needs_list = obj
        .properties
        .iter()
        .any(|(_, prop)| type_uses_list(&prop.type_expr));
    if needs_list {
        file = file.add_import(ImportSpec::named("java.util", "List"));
    }

    let needs_map = obj
        .properties
        .iter()
        .any(|(_, prop)| type_uses_map(&prop.type_expr));
    if needs_map {
        file = file.add_import(ImportSpec::named("java.util", "Map"));
    }

    let mut tb = TypeSpec::builder(&name, TypeKind::Struct).visibility(Visibility::Public);
    if let Some(doc) = &schema.description {
        tb = tb.doc(doc);
    }

    // Fields
    for (json_name, prop) in &obj.properties {
        let field_name = java_field_name(&prop.name);
        let required = prop.required && !prop.nullable;
        let type_str = if required {
            java_type_str(&prop.type_expr)
        } else {
            java_boxed_type_str(&prop.type_expr)
        };

        let annotation = if *json_name != field_name {
            Some(format!(
                "@SerializedName(\"{}\")",
                escape_java_string(json_name)
            ))
        } else {
            None
        };

        let mut fb = FieldSpec::builder(&field_name, TypeName::primitive(&type_str))
            .visibility(Visibility::Private);
        if let Some(ann) = annotation {
            fb = fb.annotation(CodeBlock::of(&ann, ()).expect("annotation"));
        }
        tb = tb.add_field(fb.build().expect("field"));
    }

    // Constructor
    let mut ctor = FunSpec::builder(&name);
    ctor = ctor.visibility(Visibility::Public);
    for (_json_name, prop) in &obj.properties {
        let field_name = java_field_name(&prop.name);
        let required = prop.required && !prop.nullable;
        let type_str = if required {
            java_type_str(&prop.type_expr)
        } else {
            java_boxed_type_str(&prop.type_expr)
        };
        ctor = ctor.add_param(
            ParameterSpec::new(&format!("{type_str} {field_name}"), TypeName::primitive(""))
                .expect("ctor param"),
        );
    }
    let assignment_fields: Vec<String> = obj
        .properties
        .iter()
        .map(|(_json_name, prop)| java_field_name(&prop.name))
        .collect();
    let ctor_body = sigil_quote!(Java {
        $for(field_name in &assignment_fields) {
            this.$L(field_name.as_str()) = $L(field_name.as_str());
        }
    })
    .expect("ctor body");
    ctor = ctor.body(ctor_body);
    tb = tb.add_method(ctor.build().expect("constructor"));

    // Getters
    for (_json_name, prop) in &obj.properties {
        let field_name = java_field_name(&prop.name);
        let required = prop.required && !prop.nullable;
        let type_str = if required {
            java_type_str(&prop.type_expr)
        } else {
            java_boxed_type_str(&prop.type_expr)
        };
        let getter_name = format!("get{}", prop.name.to_pascal_case());
        tb = tb.add_method(build_java_getter(&getter_name, &type_str, &field_name));
    }

    file = file.add_type(tb.build().ok()?);
    file.build().ok()
}

// ---------------------------------------------------------------------------
// Enum -> enum with value field
// ---------------------------------------------------------------------------

fn emit_enum(schema: &IrSchema, en: &IrEnum, package_name: &str) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();

    let mut file =
        FileSpec::builder_with("model.java", Java::new()).header(package_header(package_name));

    if en.value_type == IrEnumValueType::Mixed {
        return emit_comment_class(&name, "Object", schema.description.as_deref(), package_name);
    }

    file = file.add_import(ImportSpec::named(
        "com.google.gson.annotations",
        "SerializedName",
    ));

    let base_type = match en.value_type {
        IrEnumValueType::String => "String",
        IrEnumValueType::Integer => "int",
        IrEnumValueType::Number => "double",
        IrEnumValueType::Mixed => unreachable!(),
    };

    let mut tb = TypeSpec::builder(&name, TypeKind::Enum).visibility(Visibility::Public);
    if let Some(doc) = &schema.description {
        tb = tb.doc(doc);
    }

    for v in &en.values {
        let (variant_name, literal, raw_value) = match en.value_type {
            IrEnumValueType::String => {
                let s = v.value.as_str()?;
                (
                    enum_variant_name(s),
                    format!("\"{}\"", escape_java_string(s)),
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
                &format!("@SerializedName(\"{}\")", escape_java_string(&raw_value)),
                (),
            )
            .expect("annotation"),
        );
        ev = ev.value(CodeBlock::of(&literal, ()).expect("literal"));
        tb = tb.add_variant(ev.build().expect("enum variant"));
    }

    // Field, constructor, and getter (rendered after variants in Java)
    tb = tb.add_field(
        FieldSpec::builder("value", TypeName::primitive(base_type))
            .visibility(Visibility::Private)
            .is_readonly()
            .build()
            .expect("value field"),
    );

    let mut ctor = FunSpec::builder(&name);
    ctor = ctor.add_param(
        ParameterSpec::new("value", TypeName::primitive(base_type)).expect("ctor param"),
    );
    let ctor_body = sigil_quote!(Java {
        this.value = value;
    })
    .expect("ctor body");
    ctor = ctor.body(ctor_body);
    tb = tb.add_method(ctor.build().expect("enum ctor"));

    tb = tb.add_method(build_java_getter("getValue", base_type, "value"));

    file = file.add_type(tb.build().ok()?);
    file.build().ok()
}

// ---------------------------------------------------------------------------
// Alias -> wrapper class (Java has no typealias)
// ---------------------------------------------------------------------------

fn emit_alias(schema: &IrSchema, expr: &IrTypeExpr, package_name: &str) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();
    let rhs = java_type_str(expr);
    emit_comment_class(&name, &rhs, schema.description.as_deref(), package_name)
}

// ---------------------------------------------------------------------------
// Union -> Object wrapper class
// ---------------------------------------------------------------------------

fn emit_union(schema: &IrSchema, _union: &IrUnion, package_name: &str) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();
    emit_comment_class(&name, "Object", schema.description.as_deref(), package_name)
}

// ---------------------------------------------------------------------------
// Intersection -> class with all merged properties
// ---------------------------------------------------------------------------

fn emit_intersection(
    schema: &IrSchema,
    inter: &IrIntersection,
    package_name: &str,
) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();
    let mut file =
        FileSpec::builder_with("model.java", Java::new()).header(package_header(package_name));

    let mut tb = TypeSpec::builder(&name, TypeKind::Struct).visibility(Visibility::Public);
    if let Some(doc) = &schema.description {
        tb = tb.doc(doc);
    }

    let mut used_names: HashSet<String> = HashSet::new();
    let member_bindings: Vec<(String, String)> = inter
        .members
        .iter()
        .map(|member| {
            let member_type = java_type_str(member);
            let field_name = unique_name(&member_type.to_lower_camel_case(), &mut used_names);
            (member_type, field_name)
        })
        .collect();

    for (member_type, field_name) in &member_bindings {
        tb = tb.add_field(
            FieldSpec::builder(field_name, TypeName::primitive(member_type))
                .visibility(Visibility::Private)
                .build()
                .expect("field"),
        );
    }

    // Constructor
    let mut ctor = FunSpec::builder(&name);
    ctor = ctor.visibility(Visibility::Public);
    for (member_type, field_name) in &member_bindings {
        ctor = ctor.add_param(
            ParameterSpec::new(
                &format!("{member_type} {field_name}"),
                TypeName::primitive(""),
            )
            .expect("param"),
        );
    }
    let assignment_fields: Vec<String> = member_bindings
        .iter()
        .map(|(_member_type, field_name)| field_name.clone())
        .collect();
    let ctor_body = sigil_quote!(Java {
        $for(field_name in &assignment_fields) {
            this.$L(field_name.as_str()) = $L(field_name.as_str());
        }
    })
    .expect("ctor body");
    ctor = ctor.body(ctor_body);
    tb = tb.add_method(ctor.build().expect("constructor"));

    // Getters
    for (member_type, field_name) in &member_bindings {
        let getter_name = format!("get{}", field_name.to_pascal_case());
        tb = tb.add_method(build_java_getter(&getter_name, member_type, field_name));
    }

    file = file.add_type(tb.build().ok()?);
    file.build().ok()
}

// ---------------------------------------------------------------------------
// TaggedUnion -> wrapper class (sealed interfaces not well-supported by sigil-stitch)
// ---------------------------------------------------------------------------

fn emit_tagged_union(
    schema: &IrSchema,
    tu: &IrTaggedUnion,
    package_name: &str,
) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();

    if tu.variants.is_empty() {
        return emit_comment_class(&name, "Object", schema.description.as_deref(), package_name);
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

    emit_comment_class(&name, "Object", Some(&doc), package_name)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn emit_comment_class(
    name: &str,
    underlying_type: &str,
    doc: Option<&str>,
    package_name: &str,
) -> Option<FileSpec> {
    let mut file =
        FileSpec::builder_with("model.java", Java::new()).header(package_header(package_name));

    let mut tb = TypeSpec::builder(name, TypeKind::Struct).visibility(Visibility::Public);
    if let Some(d) = doc {
        tb = tb.doc(d);
    }

    // Single field wrapping the underlying type
    tb = tb.add_field(
        FieldSpec::builder("value", TypeName::primitive(underlying_type))
            .visibility(Visibility::Private)
            .build()
            .expect("value field"),
    );

    // Constructor
    let mut ctor = FunSpec::builder(name);
    ctor = ctor.visibility(Visibility::Public);
    ctor = ctor.add_param(
        ParameterSpec::new(&format!("{underlying_type} value"), TypeName::primitive(""))
            .expect("param"),
    );
    let body = sigil_quote!(Java {
        this.value = value;
    })
    .expect("ctor body");
    ctor = ctor.body(body);
    tb = tb.add_method(ctor.build().expect("constructor"));

    // Getter
    tb = tb.add_method(build_java_getter("getValue", underlying_type, "value"));

    file = file.add_type(tb.build().ok()?);
    file.build().ok()
}

fn enum_variant_name(s: &str) -> String {
    let upper: String = s
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    if upper.is_empty() {
        "UNKNOWN".to_string()
    } else if upper.chars().next().unwrap().is_ascii_digit() {
        format!("N{upper}")
    } else {
        upper
    }
}
