use std::collections::HashSet;

use crate::codegen::traits::file_writer::FileInfo;
use crate::generators::closed_sum::{
    ClosedSumCasePlan, ClosedSumEncoding, ClosedSumPlan, plan_tagged_union, plan_union,
};
use crate::generators::request_inputs::{
    RequestInputField, RequestInputFieldKind, RequestInputModel, RequestInputPlan,
};
use crate::generators::{ArtifactKind, GeneratorError};
use crate::ir::types::{
    IrEnum, IrEnumValueType, IrIntersection, IrObject, IrPrimitive, IrSchema, IrSchemaKind, IrSpec,
    IrTaggedUnion, IrTypeExpr, IrUnion,
};
use heck::{ToLowerCamelCase, ToPascalCase};
use sigil_stitch::lang::java::Java;
use sigil_stitch::prelude::*;
use sigil_stitch::spec::closed_sum_case_spec::ClosedSumCaseSpec;
use sigil_stitch::spec::closed_sum_spec::ClosedSumSpec;
use sigil_stitch::spec::where_spec::TypeArgument;

use super::util::{
    build_java_getter, escape_java_string, java_boxed_type_str, java_field_name, java_getter_name,
    java_primitive, java_primitive_boxed, java_type_str, type_uses_list, type_uses_map,
    unique_name,
};

const RENDER_WIDTH: usize = 100;

pub(crate) fn generate_model_files(
    ir: &IrSpec,
    package_name: &str,
    header: &str,
    request_inputs: &RequestInputPlan,
) -> Result<Vec<FileInfo>, GeneratorError> {
    let mut files = Vec::new();
    for (_name, schema) in &ir.schemas {
        let body = emit_model_body(schema, ir, package_name)
            .map_err(|error| GeneratorError::emission(ArtifactKind::Model, &schema.name, error))?;
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
    let needs_list = model
        .fields
        .iter()
        .any(|field| type_uses_list(&field.type_expr));
    if needs_list {
        content.push_str("import java.util.List;\n");
    }
    let needs_map = model
        .fields
        .iter()
        .any(|field| type_uses_map(&field.type_expr));
    if needs_map {
        content.push_str("import java.util.Map;\n");
    }
    if needs_list || needs_map {
        content.push('\n');
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
        let getter = java_getter_name(&field.wire_name);
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

fn emit_model_body(
    schema: &IrSchema,
    ir: &IrSpec,
    package_name: &str,
) -> Result<String, SigilStitchError> {
    let file_spec = match &schema.kind {
        IrSchemaKind::Object(obj) => emit_object(schema, obj, package_name),
        IrSchemaKind::Enum(en) => emit_enum(schema, en, package_name),
        IrSchemaKind::Alias(expr) => emit_alias(schema, expr, package_name),
        IrSchemaKind::Union(u) => emit_union(schema, u, ir, package_name),
        IrSchemaKind::Intersection(i) => emit_intersection(schema, i, package_name),
        IrSchemaKind::TaggedUnion(tu) => emit_tagged_union(schema, tu, ir, package_name),
    }?;
    file_spec.render(RENDER_WIDTH)
}

fn package_header(package_name: &str) -> Result<CodeBlock, SigilStitchError> {
    sigil_quote!(Java {
        package $L(format!("{package_name}.models"));
    })
}

// ---------------------------------------------------------------------------
// Object -> class with private fields, constructor, getters
// ---------------------------------------------------------------------------

fn emit_object(
    schema: &IrSchema,
    obj: &IrObject,
    package_name: &str,
) -> Result<FileSpec, SigilStitchError> {
    let name = schema.name.to_pascal_case();

    let mut file =
        FileSpec::builder_with("model.java", Java::new()).header(package_header(package_name)?);

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
            fb = fb.annotation(CodeBlock::of(&ann, ())?);
        }
        tb = tb.add_field(fb.build()?);
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
        ctor = ctor.add_param(ParameterSpec::new(
            &field_name,
            TypeName::primitive(&type_str),
        )?);
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
    })?;
    ctor = ctor.body(ctor_body);
    tb = tb.add_method(ctor.build()?);

    // Getters
    for (_json_name, prop) in &obj.properties {
        let field_name = java_field_name(&prop.name);
        let required = prop.required && !prop.nullable;
        let type_str = if required {
            java_type_str(&prop.type_expr)
        } else {
            java_boxed_type_str(&prop.type_expr)
        };
        let getter_name = java_getter_name(&prop.name);
        tb = tb.add_method(build_java_getter(&getter_name, &type_str, &field_name));
    }

    file = file.add_type(tb.build()?);
    file.build()
}

// ---------------------------------------------------------------------------
// Enum -> enum with value field
// ---------------------------------------------------------------------------

fn emit_enum(
    schema: &IrSchema,
    en: &IrEnum,
    package_name: &str,
) -> Result<FileSpec, SigilStitchError> {
    let name = schema.name.to_pascal_case();

    let mut file =
        FileSpec::builder_with("model.java", Java::new()).header(package_header(package_name)?);

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
                let s = v.value.as_str().ok_or_else(|| SigilStitchError::Render {
                    context: format!("enum {name} value"),
                    message: "expected a string enum value".into(),
                })?;
                (
                    enum_variant_name(s),
                    format!("\"{}\"", escape_java_string(s)),
                    s.to_string(),
                )
            }
            IrEnumValueType::Integer | IrEnumValueType::Number => {
                let n = v
                    .value
                    .as_number()
                    .ok_or_else(|| SigilStitchError::Render {
                        context: format!("enum {name} value"),
                        message: "expected a numeric enum value".into(),
                    })?;
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
        ev = ev.annotation(CodeBlock::of(
            &format!("@SerializedName(\"{}\")", escape_java_string(&raw_value)),
            (),
        )?);
        ev = ev.constructor_argument(CodeBlock::of(&literal, ())?);
        tb = tb.add_variant(ev.build()?);
    }

    // Field, constructor, and getter (rendered after variants in Java)
    tb = tb.add_field(
        FieldSpec::builder("value", TypeName::primitive(base_type))
            .visibility(Visibility::Private)
            .is_readonly()
            .build()?,
    );

    let mut ctor = FunSpec::builder(&name);
    ctor = ctor.add_param(ParameterSpec::new("value", TypeName::primitive(base_type))?);
    let ctor_body = sigil_quote!(Java {
        this.value = value;
    })?;
    ctor = ctor.body(ctor_body);
    tb = tb.add_method(ctor.build()?);

    tb = tb.add_method(build_java_getter("getValue", base_type, "value"));

    file = file.add_type(tb.build()?);
    file.build()
}

// ---------------------------------------------------------------------------
// Alias -> wrapper class (Java has no typealias)
// ---------------------------------------------------------------------------

fn emit_alias(
    schema: &IrSchema,
    expr: &IrTypeExpr,
    package_name: &str,
) -> Result<FileSpec, SigilStitchError> {
    let name = schema.name.to_pascal_case();
    let rhs = java_type_str(expr);
    emit_comment_class(&name, &rhs, schema.description.as_deref(), package_name)
}

// ---------------------------------------------------------------------------
// Union -> sealed interface with typed cases
// ---------------------------------------------------------------------------

fn emit_union(
    schema: &IrSchema,
    union: &IrUnion,
    ir: &IrSpec,
    package_name: &str,
) -> Result<FileSpec, SigilStitchError> {
    if union.members.is_empty() && !union.nullable {
        return emit_empty_model(schema, package_name);
    }
    emit_closed_sum(
        &plan_union(&schema.name, schema.description.as_deref(), union),
        &ir.schemas,
        package_name,
    )
}

// ---------------------------------------------------------------------------
// Intersection -> class with all merged properties
// ---------------------------------------------------------------------------

fn emit_intersection(
    schema: &IrSchema,
    inter: &IrIntersection,
    package_name: &str,
) -> Result<FileSpec, SigilStitchError> {
    let name = schema.name.to_pascal_case();
    let mut file =
        FileSpec::builder_with("model.java", Java::new()).header(package_header(package_name)?);

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
    if member_bindings
        .iter()
        .any(|(member_type, _)| member_type.contains("List<"))
    {
        file = file.add_import(ImportSpec::named("java.util", "List"));
    }
    if member_bindings
        .iter()
        .any(|(member_type, _)| member_type.contains("Map<"))
    {
        file = file.add_import(ImportSpec::named("java.util", "Map"));
    }

    for (member_type, field_name) in &member_bindings {
        tb = tb.add_field(
            FieldSpec::builder(field_name, TypeName::primitive(member_type))
                .visibility(Visibility::Private)
                .build()?,
        );
    }

    // Constructor
    let mut ctor = FunSpec::builder(&name);
    ctor = ctor.visibility(Visibility::Public);
    for (member_type, field_name) in &member_bindings {
        ctor = ctor.add_param(ParameterSpec::new(
            field_name,
            TypeName::primitive(member_type),
        )?);
    }
    let assignment_fields: Vec<String> = member_bindings
        .iter()
        .map(|(_member_type, field_name)| field_name.clone())
        .collect();
    let ctor_body = sigil_quote!(Java {
        $for(field_name in &assignment_fields) {
            this.$L(field_name.as_str()) = $L(field_name.as_str());
        }
    })?;
    ctor = ctor.body(ctor_body);
    tb = tb.add_method(ctor.build()?);

    // Getters
    for (member_type, field_name) in &member_bindings {
        let getter_name = format!("get{}", field_name.to_pascal_case());
        tb = tb.add_method(build_java_getter(&getter_name, member_type, field_name));
    }

    file = file.add_type(tb.build()?);
    file.build()
}

// ---------------------------------------------------------------------------
// TaggedUnion -> sealed interface with typed cases
// ---------------------------------------------------------------------------

fn emit_tagged_union(
    schema: &IrSchema,
    tu: &IrTaggedUnion,
    ir: &IrSpec,
    package_name: &str,
) -> Result<FileSpec, SigilStitchError> {
    if tu.variants.is_empty() {
        return emit_empty_model(schema, package_name);
    }
    emit_closed_sum(
        &plan_tagged_union(&schema.name, schema.description.as_deref(), tu, ir),
        &ir.schemas,
        package_name,
    )
}

fn emit_empty_model(schema: &IrSchema, package_name: &str) -> Result<FileSpec, SigilStitchError> {
    let name = schema.name.to_pascal_case();
    let mut file =
        FileSpec::builder_with("model.java", Java::new()).header(package_header(package_name)?);
    let mut type_builder = TypeSpec::builder(&name, TypeKind::Class).visibility(Visibility::Public);
    if let Some(doc) = &schema.description {
        type_builder = type_builder.doc(doc);
    }
    let constructor = FunSpec::builder(&name)
        .visibility(Visibility::Private)
        .body(sigil_quote!(Java {
            throw new AssertionError();
        })?)
        .build()?;
    type_builder = type_builder.add_method(constructor);
    file = file.add_type(type_builder.build()?);
    file.build()
}

fn emit_closed_sum(
    plan: &ClosedSumPlan,
    schemas: &indexmap::IndexMap<String, IrSchema>,
    package_name: &str,
) -> Result<FileSpec, SigilStitchError> {
    let mut sum = ClosedSumSpec::builder(&plan.name).visibility(Visibility::Public);
    for doc in &plan.docs {
        sum = sum.doc(doc);
    }
    let adapter_name = format!("{}JsonAdapter", plan.name);
    sum = sum.annotate(
        AnnotationSpec::importable(TypeName::importable(
            "com.google.gson.annotations",
            "JsonAdapter",
        ))
        .args([
            format!("value = {adapter_name}.class"),
            "nullSafe = false".to_string(),
        ]),
    );
    for case in &plan.cases {
        sum = sum.add_case(java_closed_sum_case(case)?);
    }

    let mut file =
        FileSpec::builder_with("model.java", Java::new()).header(package_header(package_name)?);
    for import in java_adapter_imports() {
        file = file.add_import(import);
    }
    file.add_closed_sum(sum.build()?)
        .add_code(java_closed_sum_adapter(plan, schemas)?)
        .build()
}

fn java_closed_sum_case(case: &ClosedSumCasePlan) -> Result<ClosedSumCaseSpec, SigilStitchError> {
    let mut builder = ClosedSumCaseSpec::builder(&case.name);
    if let Some(doc) = &case.doc {
        builder = builder.doc(doc);
    }
    if let Some(payload) = &case.payload {
        builder = builder.positional_payload(java_type_name(payload, false));
    }
    builder.build()
}

fn java_type_name(expr: &IrTypeExpr, boxed: bool) -> TypeName {
    match expr {
        IrTypeExpr::Named(name) => TypeName::primitive(&name.to_pascal_case()),
        IrTypeExpr::Primitive(primitive) => TypeName::primitive(if boxed {
            java_primitive_boxed(primitive)
        } else {
            java_primitive(primitive)
        }),
        IrTypeExpr::Array(inner) => TypeName::application(
            TypeName::importable("java.util", "List"),
            vec![TypeArgument::Single(java_type_name(inner, true))],
        ),
        IrTypeExpr::Map(inner) => TypeName::application(
            TypeName::importable("java.util", "Map"),
            vec![
                TypeArgument::Single(TypeName::primitive("String")),
                TypeArgument::Single(java_type_name(inner, true)),
            ],
        ),
        IrTypeExpr::Nullable(inner) => java_type_name(inner, true),
        IrTypeExpr::StringLiteral(_) | IrTypeExpr::StringEnum(_) => TypeName::primitive("String"),
        IrTypeExpr::Union(_) | IrTypeExpr::Any => {
            TypeName::importable("com.google.gson", "JsonElement")
        }
    }
}

fn java_adapter_imports() -> [ImportSpec; 11] {
    [
        ImportSpec::named("com.google.gson", "Gson"),
        ImportSpec::named("com.google.gson", "JsonElement"),
        ImportSpec::named("com.google.gson", "JsonNull"),
        ImportSpec::named("com.google.gson", "JsonObject"),
        ImportSpec::named("com.google.gson", "JsonParseException"),
        ImportSpec::named("com.google.gson", "JsonParser"),
        ImportSpec::named("com.google.gson", "TypeAdapter"),
        ImportSpec::named("com.google.gson.reflect", "TypeToken"),
        ImportSpec::named("com.google.gson.stream", "JsonReader"),
        ImportSpec::named("com.google.gson.stream", "JsonWriter"),
        ImportSpec::named("com.google.gson.annotations", "JsonAdapter"),
    ]
}

fn java_closed_sum_adapter(
    plan: &ClosedSumPlan,
    schemas: &indexmap::IndexMap<String, IrSchema>,
) -> Result<CodeBlock, SigilStitchError> {
    let read_dispatch =
        java_adapter_read_dispatch(plan, schemas).map_err(|error| SigilStitchError::Render {
            context: format!("Java {} Gson adapter read dispatch", plan.name),
            message: error.to_string(),
        })?;
    let write_cases = plan
        .cases
        .iter()
        .map(|case| {
            java_adapter_write_case(plan, case).map_err(|error| SigilStitchError::Render {
                context: format!("Java {} Gson adapter write case {}", plan.name, case.name),
                message: error.to_string(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let adapter_name = format!("{}JsonAdapter", plan.name);
    let root_name = plan.name.as_str();

    sigil_quote!(Java {
        final class $N(adapter_name) extends TypeAdapter<$N(root_name)> {
            private final Gson gson = new Gson();

            @Override
            public $N(root_name) read(JsonReader reader) throws java.io.IOException {
                JsonElement element = JsonParser.parseReader(reader);
                $C(read_dispatch)
            }

            @Override
            public void write(JsonWriter writer, $N(root_name) value) throws java.io.IOException {
                if (value == null) {
                    writer.nullValue();
                    return;
                }
                $C_each(write_cases)
            }
        }
    })
    .map_err(|error| SigilStitchError::Render {
        context: format!("Java {} Gson adapter", plan.name),
        message: error.to_string(),
    })
}

fn java_adapter_read_dispatch(
    plan: &ClosedSumPlan,
    schemas: &indexmap::IndexMap<String, IrSchema>,
) -> Result<CodeBlock, SigilStitchError> {
    match &plan.encoding {
        ClosedSumEncoding::Untagged => {
            let null_case = plan.cases.iter().find(|case| case.is_null);
            let null_result = match null_case {
                Some(case) => sigil_quote!(Java {
                    return $N(plan.name.as_str()).$N(case.name.as_str()).INSTANCE;
                })?,
                None => sigil_quote!(Java { return null; })?,
            };
            let mut branches = vec![sigil_quote!(Java {
                if (element.isJsonNull()) {
                    $C(null_result)
                }
            })?];
            let mut catch_all = None;
            let cases = plan
                .cases
                .iter()
                .filter(|case| !case.is_null && !is_catch_all_case(case))
                .chain(
                    plan.cases
                        .iter()
                        .filter(|case| !case.is_null && is_catch_all_case(case)),
                );
            for case in cases {
                let Some(payload) = &case.payload else {
                    continue;
                };
                let condition = java_json_condition(payload, schemas)?;
                let source = sigil_quote!(Java { element })?;
                let result = java_adapter_read_value(plan, case, payload, source)?;
                if is_catch_all_case(case) {
                    catch_all = Some(result);
                } else {
                    branches.push(sigil_quote!(Java {
                        if ($C(condition)) {
                            $C(result)
                        }
                    })?);
                }
            }
            let fallback = match catch_all {
                Some(result) => result,
                None => sigil_quote!(Java {
                    throw new JsonParseException($S(format!(
                        "unknown {} union value",
                        plan.name
                    )));
                })?,
            };
            sigil_quote!(Java {
                $C_each(branches)
                $C(fallback)
            })
        }
        ClosedSumEncoding::Internal {
            discriminator_field,
        } => {
            let branches = plan
                .cases
                .iter()
                .map(|case| java_tagged_read_case(plan, case, discriminator_field, None))
                .collect::<Result<Vec<_>, _>>()?;
            sigil_quote!(Java {
                JsonObject object = element.getAsJsonObject();
                String discriminator = object.has($S(discriminator_field)) ? object.get($S(discriminator_field)).getAsString() : null;
                $C_each(branches)
                throw new JsonParseException($S(format!("unknown {} discriminator", plan.name)));
            })
        }
        ClosedSumEncoding::Adjacent {
            discriminator_field,
            content_field,
        } => {
            let branches = plan
                .cases
                .iter()
                .map(|case| {
                    java_tagged_read_case(plan, case, discriminator_field, Some(content_field))
                })
                .collect::<Result<Vec<_>, _>>()?;
            sigil_quote!(Java {
                JsonObject object = element.getAsJsonObject();
                String discriminator = object.has($S(discriminator_field)) ? object.get($S(discriminator_field)).getAsString() : null;
                $C_each(branches)
                throw new JsonParseException($S(format!("unknown {} discriminator", plan.name)));
            })
        }
        ClosedSumEncoding::External => {
            let branches = plan
                .cases
                .iter()
                .map(|case| java_external_read_case(plan, case))
                .collect::<Result<Vec<_>, _>>()?;
            sigil_quote!(Java {
                JsonObject object = element.getAsJsonObject();
                $C_each(branches)
                throw new JsonParseException($S(format!("unknown {} variant", plan.name)));
            })
        }
    }
}

fn java_tagged_read_case(
    plan: &ClosedSumPlan,
    case: &ClosedSumCasePlan,
    _discriminator_field: &str,
    content_field: Option<&String>,
) -> Result<CodeBlock, SigilStitchError> {
    let wire_name = case.wire_name.as_deref().unwrap_or_default();
    let source = match content_field {
        Some(content_field) => sigil_quote!(Java { object.get($S(content_field.as_str())) })?,
        None => sigil_quote!(Java { object })?,
    };
    let result = match &case.payload {
        Some(payload) => java_adapter_read_value(plan, case, payload, source)?,
        None => sigil_quote!(Java {
            return $N(plan.name.as_str()).$N(case.name.as_str()).INSTANCE;
        })?,
    };
    sigil_quote!(Java {
        if ($S(wire_name).equals(discriminator)) {
            $C(result)
        }
    })
}

fn java_external_read_case(
    plan: &ClosedSumPlan,
    case: &ClosedSumCasePlan,
) -> Result<CodeBlock, SigilStitchError> {
    let wire_name = case.wire_name.as_deref().unwrap_or_default();
    let source = sigil_quote!(Java { object.get($S(wire_name)) })?;
    let result = match &case.payload {
        Some(payload) => java_adapter_read_value(plan, case, payload, source)?,
        None => sigil_quote!(Java {
            return $N(plan.name.as_str()).$N(case.name.as_str()).INSTANCE;
        })?,
    };
    sigil_quote!(Java {
        if (object.has($S(wire_name))) {
            $C(result)
        }
    })
}

fn java_adapter_read_value(
    plan: &ClosedSumPlan,
    case: &ClosedSumCasePlan,
    payload: &IrTypeExpr,
    source: CodeBlock,
) -> Result<CodeBlock, SigilStitchError> {
    let payload_type = java_type_name(payload, true);
    let root_name = plan.name.as_str();
    let case_name = case.name.as_str();
    if matches!(
        payload,
        IrTypeExpr::Primitive(
            IrPrimitive::String
                | IrPrimitive::StringWithFormat(_)
                | IrPrimitive::Date
                | IrPrimitive::DateTime
                | IrPrimitive::Uuid,
        ) | IrTypeExpr::StringLiteral(_)
            | IrTypeExpr::StringEnum(_)
    ) {
        return sigil_quote!(Java {
        return new $N(root_name).$N(case_name)($C(source).getAsString());
        });
    }
    if matches!(payload, IrTypeExpr::Primitive(IrPrimitive::Boolean)) {
        return sigil_quote!(Java {
        return new $N(root_name).$N(case_name)($C(source).getAsBoolean());
        });
    }
    if matches!(
        payload,
        IrTypeExpr::Primitive(
            IrPrimitive::Integer
                | IrPrimitive::IntegerWithFormat(_)
                | IrPrimitive::Number
                | IrPrimitive::NumberWithFormat(_)
        )
    ) {
        return sigil_quote!(Java {
            return new $N(root_name).$N(case_name)(gson.fromJson($C(source), $T(payload_type).class));
        });
    }
    sigil_quote!(Java {
        return new $N(root_name).$N(case_name)(gson.fromJson($C(source), new TypeToken<$T(payload_type)>() {}.getType()));
    })
}

fn java_adapter_write_case(
    plan: &ClosedSumPlan,
    case: &ClosedSumCasePlan,
) -> Result<CodeBlock, SigilStitchError> {
    let root_name = plan.name.as_str();
    let case_name = case.name.as_str();
    let body = match &plan.encoding {
        ClosedSumEncoding::Untagged => {
            if case.is_null {
                sigil_quote!(Java { writer.nullValue(); })?
            } else {
                sigil_quote!(Java {
                    gson.toJson(gson.toJsonTree((( $N(root_name).$N(case_name) ) value).value0()), writer);
                })?
            }
        }
        ClosedSumEncoding::Internal {
            discriminator_field,
        } => {
            let wire_name = case.wire_name.as_deref().unwrap_or_default();
            if case.payload.is_some() {
                sigil_quote!(Java {
                    JsonObject object = gson.toJsonTree((( $N(root_name).$N(case_name) ) value).value0()).getAsJsonObject();
                    object.addProperty($S(discriminator_field), $S(wire_name));
                    gson.toJson(object, writer);
                })?
            } else {
                sigil_quote!(Java {
                    JsonObject object = new JsonObject();
                    object.addProperty($S(discriminator_field), $S(wire_name));
                    gson.toJson(object, writer);
                })?
            }
        }
        ClosedSumEncoding::Adjacent {
            discriminator_field,
            content_field,
        } => {
            let wire_name = case.wire_name.as_deref().unwrap_or_default();
            if case.payload.is_some() {
                sigil_quote!(Java {
                    JsonObject object = new JsonObject();
                    object.addProperty($S(discriminator_field), $S(wire_name));
                    object.add($S(content_field), gson.toJsonTree((( $N(root_name).$N(case_name) ) value).value0()));
                    gson.toJson(object, writer);
                })?
            } else {
                sigil_quote!(Java {
                    JsonObject object = new JsonObject();
                    object.addProperty($S(discriminator_field), $S(wire_name));
                    gson.toJson(object, writer);
                })?
            }
        }
        ClosedSumEncoding::External => {
            let wire_name = case.wire_name.as_deref().unwrap_or_default();
            if case.payload.is_some() {
                sigil_quote!(Java {
                    JsonObject object = new JsonObject();
                    object.add($S(wire_name), gson.toJsonTree((( $N(root_name).$N(case_name) ) value).value0()));
                    gson.toJson(object, writer);
                })?
            } else {
                sigil_quote!(Java {
                    JsonObject object = new JsonObject();
                    object.add($S(wire_name), JsonNull.INSTANCE);
                    gson.toJson(object, writer);
                })?
            }
        }
    };
    sigil_quote!(Java {
        if (value instanceof $N(root_name).$N(case_name)) {
            $C(body)
            return;
        }
    })
}

fn java_json_condition(
    expr: &IrTypeExpr,
    schemas: &indexmap::IndexMap<String, IrSchema>,
) -> Result<CodeBlock, SigilStitchError> {
    match expr {
        IrTypeExpr::Primitive(
            IrPrimitive::String
            | IrPrimitive::StringWithFormat(_)
            | IrPrimitive::Date
            | IrPrimitive::DateTime
            | IrPrimitive::Uuid,
        )
        | IrTypeExpr::StringLiteral(_)
        | IrTypeExpr::StringEnum(_) => {
            let base = sigil_quote!(Java {
                element.isJsonPrimitive() && element.getAsJsonPrimitive().isString()
            })?;
            match expr {
                IrTypeExpr::StringLiteral(value) => sigil_quote!(Java {
                    $C(base) && element.getAsString().equals($S(value))
                }),
                IrTypeExpr::StringEnum(values) => {
                    let checks = values
                        .iter()
                        .map(|value| sigil_quote!(Java { element.getAsString().equals($S(value)) }))
                        .collect::<Result<Vec<_>, _>>()?;
                    let Some((first, rest)) = checks.split_first() else {
                        return sigil_quote!(Java { false });
                    };
                    let values = java_join_conditions(first.clone(), rest.to_vec(), true)?;
                    sigil_quote!(Java { ($C(base) && $C(values)) })
                }
                _ => Ok(base),
            }
        }
        IrTypeExpr::Primitive(IrPrimitive::Boolean) => {
            sigil_quote!(Java {
                element.isJsonPrimitive() && element.getAsJsonPrimitive().isBoolean()
            })
        }
        IrTypeExpr::Primitive(
            IrPrimitive::Integer
            | IrPrimitive::IntegerWithFormat(_)
            | IrPrimitive::Number
            | IrPrimitive::NumberWithFormat(_),
        ) => sigil_quote!(Java {
            element.isJsonPrimitive() && element.getAsJsonPrimitive().isNumber()
        }),
        IrTypeExpr::Primitive(IrPrimitive::Binary) => {
            sigil_quote!(Java { element.isJsonPrimitive() && element.getAsJsonPrimitive().isString() })
        }
        IrTypeExpr::Array(_) => sigil_quote!(Java { element.isJsonArray() }),
        IrTypeExpr::Map(_) => sigil_quote!(Java { element.isJsonObject() }),
        IrTypeExpr::Nullable(inner) => {
            let inner = java_json_condition(inner, schemas)?;
            sigil_quote!(Java { element.isJsonNull() || $C(inner) })
        }
        IrTypeExpr::Any => sigil_quote!(Java { true }),
        IrTypeExpr::Union(members) => {
            let conditions = members
                .iter()
                .map(|member| java_json_condition(member, schemas))
                .collect::<Result<Vec<_>, _>>()?;
            let Some((first, rest)) = conditions.split_first() else {
                return sigil_quote!(Java { false });
            };
            java_join_conditions(first.clone(), rest.to_vec(), true)
        }
        IrTypeExpr::Named(name) => match schemas.get(name).map(|schema| &schema.kind) {
            Some(IrSchemaKind::Object(object)) => {
                let required = object
                    .properties
                    .iter()
                    .filter(|(_, property)| property.required)
                    .map(|(wire_name, _)| wire_name)
                    .collect::<Vec<_>>();
                let checks = required
                    .iter()
                    .map(|wire_name| {
                        sigil_quote!(Java {
                            element.getAsJsonObject().has($S(wire_name.as_str()))
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let object_check = sigil_quote!(Java { element.isJsonObject() })?;
                Ok(java_join_conditions(object_check, checks, false)?)
            }
            Some(IrSchemaKind::Enum(en)) => match en.value_type {
                IrEnumValueType::String => sigil_quote!(Java {
                    element.isJsonPrimitive() && element.getAsJsonPrimitive().isString()
                }),
                IrEnumValueType::Integer | IrEnumValueType::Number => sigil_quote!(Java {
                    element.isJsonPrimitive() && element.getAsJsonPrimitive().isNumber()
                }),
                IrEnumValueType::Mixed => sigil_quote!(Java { element.isJsonPrimitive() }),
            },
            Some(IrSchemaKind::Alias(inner)) => java_json_condition(inner, schemas),
            Some(IrSchemaKind::Union(union)) => {
                let conditions = union
                    .members
                    .iter()
                    .map(|member| java_json_condition(member, schemas))
                    .collect::<Result<Vec<_>, _>>()?;
                let Some((first, rest)) = conditions.split_first() else {
                    return sigil_quote!(Java { false });
                };
                java_join_conditions(first.clone(), rest.to_vec(), true)
            }
            _ => sigil_quote!(Java { element.isJsonObject() }),
        },
    }
}

fn is_catch_all_case(case: &ClosedSumCasePlan) -> bool {
    matches!(case.payload.as_ref(), Some(IrTypeExpr::Any))
}

fn java_join_conditions(
    first: CodeBlock,
    rest: Vec<CodeBlock>,
    disjunction: bool,
) -> Result<CodeBlock, SigilStitchError> {
    let mut result = first;
    for condition in rest {
        result = if disjunction {
            sigil_quote!(Java { ($C(result) || $C(condition)) })?
        } else {
            sigil_quote!(Java { ($C(result) && $C(condition)) })?
        };
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn emit_comment_class(
    name: &str,
    underlying_type: &str,
    doc: Option<&str>,
    package_name: &str,
) -> Result<FileSpec, SigilStitchError> {
    let mut file =
        FileSpec::builder_with("model.java", Java::new()).header(package_header(package_name)?);
    if underlying_type.contains("List<") {
        file = file.add_import(ImportSpec::named("java.util", "List"));
    }
    if underlying_type.contains("Map<") {
        file = file.add_import(ImportSpec::named("java.util", "Map"));
    }

    let mut tb = TypeSpec::builder(name, TypeKind::Struct).visibility(Visibility::Public);
    if let Some(d) = doc {
        tb = tb.doc(d);
    }

    // Single field wrapping the underlying type
    tb = tb.add_field(
        FieldSpec::builder("value", TypeName::primitive(underlying_type))
            .visibility(Visibility::Private)
            .build()?,
    );

    // Constructor
    let mut ctor = FunSpec::builder(name);
    ctor = ctor.visibility(Visibility::Public);
    ctor = ctor.add_param(ParameterSpec::new(
        "value",
        TypeName::primitive(underlying_type),
    )?);
    let body = sigil_quote!(Java {
        this.value = value;
    })?;
    ctor = ctor.body(body);
    tb = tb.add_method(ctor.build()?);

    // Getter
    tb = tb.add_method(build_java_getter("getValue", underlying_type, "value"));

    file = file.add_type(tb.build()?);
    file.build()
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
    } else if upper.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        format!("N{upper}")
    } else {
        upper
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sigil_stitch::assert_rendered;

    #[test]
    fn closed_sum_adapter_render_is_pinned() {
        let plan = ClosedSumPlan {
            name: "Choice".to_string(),
            docs: Vec::new(),
            cases: vec![ClosedSumCasePlan {
                name: "VariantString".to_string(),
                payload: Some(IrTypeExpr::Primitive(IrPrimitive::String)),
                doc: None,
                wire_name: None,
                is_null: false,
            }],
            encoding: ClosedSumEncoding::Untagged,
        };
        let adapter = java_closed_sum_adapter(&plan, &indexmap::IndexMap::new()).unwrap();
        assert_rendered!(
            Java::new(),
            width = 100,
            adapter,
            r#"final class ChoiceJsonAdapter extends TypeAdapter<Choice> {
    private final Gson gson = new Gson();

    @Override
    public Choice read (JsonReader reader) throws java.io.IOException {
        JsonElement element = JsonParser.parseReader(reader);
        if (element.isJsonNull()) {
            return null;

        }
        if (element.isJsonPrimitive() && element.getAsJsonPrimitive().isString()
        ) {
            return new Choice.VariantString(element
            .getAsString());

        }
        throw new JsonParseException("unknown Choice union value");


    }

    @Override
    public void write(JsonWriter writer, Choice value) throws java.io.IOException {
        if (value == null) {
            writer.nullValue();
            return;
        }
        if (value instanceof Choice.VariantString) {
            gson.toJson(gson.toJsonTree(((Choice.VariantString) value).value0()), writer);

            return;
        }
    }
}
"#,
        );
    }
}
