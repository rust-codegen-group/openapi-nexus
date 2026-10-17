use std::collections::HashSet;

use crate::codegen::traits::file_writer::FileInfo;
use crate::generators::EmissionError;
use crate::generators::closed_sum::{
    ClosedSumCasePlan, ClosedSumEncoding, ClosedSumPlan, plan_tagged_union, plan_union,
};
use crate::generators::request_inputs::{
    RequestInputField, RequestInputFieldKind, RequestInputModel, RequestInputPlan,
};
use crate::ir::types::{
    IrEnum, IrEnumValueType, IrIntersection, IrObject, IrPrimitive, IrSchema, IrSchemaKind, IrSpec,
    IrTaggedUnion, IrTypeExpr, IrUnion,
};
use heck::{ToLowerCamelCase, ToPascalCase};
use sigil_stitch::lang::kotlin::Kotlin;
use sigil_stitch::prelude::*;
use sigil_stitch::spec::closed_sum_case_spec::ClosedSumCaseSpec;
use sigil_stitch::spec::closed_sum_spec::ClosedSumSpec;
use sigil_stitch::spec::where_spec::TypeArgument;

use super::util::{escape_kt_string, kt_decl_name, kt_primitive, kt_ref, kt_type_str, unique_name};

const RENDER_WIDTH: usize = 100;

pub(crate) fn generate_model_files(
    ir: &IrSpec,
    package_name: &str,
    header: &str,
    request_inputs: &RequestInputPlan,
) -> Result<Vec<FileInfo>, EmissionError> {
    let mut files = Vec::new();
    for (_name, schema) in &ir.schemas {
        let body = emit_model_body(schema, ir, package_name)
            .map_err(|error| EmissionError::model("Kotlin", &schema.name, error))?;
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
        let field_name = kt_ref(&kt_decl_name(&field.wire_name));
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

fn emit_model_body(
    schema: &IrSchema,
    ir: &IrSpec,
    package_name: &str,
) -> Result<String, SigilStitchError> {
    let file_spec = match &schema.kind {
        IrSchemaKind::Object(obj) => emit_object(schema, obj, package_name),
        IrSchemaKind::Enum(en) => emit_enum(schema, en, package_name),
        IrSchemaKind::Alias(expr) => emit_alias(schema, expr, package_name),
        IrSchemaKind::Union(u) => emit_union(schema, u, package_name),
        IrSchemaKind::Intersection(i) => emit_intersection(schema, i, package_name),
        IrSchemaKind::TaggedUnion(tu) => emit_tagged_union(schema, tu, ir, package_name),
    }?;
    file_spec.render(RENDER_WIDTH)
}

fn package_header(package_name: &str) -> Result<CodeBlock, SigilStitchError> {
    sigil_quote!(Kotlin {
        package $L(format!("{package_name}.models"))
    })
}

// ---------------------------------------------------------------------------
// Object -> data class with primary constructor
// ---------------------------------------------------------------------------

fn emit_object(
    schema: &IrSchema,
    obj: &IrObject,
    package_name: &str,
) -> Result<FileSpec, SigilStitchError> {
    let name = schema.name.to_pascal_case();

    let mut file =
        FileSpec::builder_with("model.kt", Kotlin::new()).header(package_header(package_name)?);

    let mut tb = TypeSpec::builder(&name, TypeKind::Struct).visibility(Visibility::Public);
    if let Some(doc) = &schema.description {
        tb = tb.doc(doc);
    }

    for (_json_name, prop) in &obj.properties {
        let field_name = kt_decl_name(&prop.name);
        let required = prop.required && !prop.nullable;
        let type_str = if required {
            kt_type_str(&prop.type_expr)
        } else {
            format!("{}?", kt_type_str(&prop.type_expr))
        };

        let mut param_builder =
            ParameterSpec::builder(&field_name, TypeName::primitive(&type_str)).is_property();
        if *_json_name != field_name {
            param_builder = param_builder.annotate_storage(serialized_name_annotation(_json_name));
        }
        if !required {
            param_builder = param_builder.default_value(CodeBlock::of("null", ())?);
        }
        let param = param_builder.build()?;
        tb = tb.add_primary_constructor_param(param);
    }

    file = file.add_type(tb.build()?);
    file.build()
}

fn serialized_name_annotation(json_name: &str) -> AnnotationSpec {
    AnnotationSpec::importable(TypeName::importable(
        "com.google.gson.annotations",
        "SerializedName",
    ))
    .arg(format!("\"{}\"", escape_kt_string(json_name)))
}

// ---------------------------------------------------------------------------
// Enum -> enum class
// ---------------------------------------------------------------------------

fn emit_enum(
    schema: &IrSchema,
    en: &IrEnum,
    package_name: &str,
) -> Result<FileSpec, SigilStitchError> {
    let name = schema.name.to_pascal_case();

    let mut file =
        FileSpec::builder_with("model.kt", Kotlin::new()).header(package_header(package_name)?);

    if en.value_type == IrEnumValueType::Mixed {
        return emit_typealias(&name, "Any", schema.description.as_deref(), package_name);
    }

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
        ParameterSpec::builder("value", TypeName::primitive(base_type))
            .is_property()
            .build()?,
    );

    for v in &en.values {
        let (variant_name, literal, raw_value) = match en.value_type {
            IrEnumValueType::String => {
                let s = v.value.as_str().ok_or_else(|| SigilStitchError::Render {
                    context: format!("enum {name} value"),
                    message: "expected a string enum value".into(),
                })?;
                (
                    s.to_pascal_case().replace(['-', ' '], ""),
                    format!("\"{}\"", escape_kt_string(s)),
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
        ev = ev.annotate(serialized_name_annotation(&raw_value));
        ev = ev.constructor_argument(CodeBlock::of(&literal, ())?);
        tb = tb.add_variant(ev.build()?);
    }

    file = file.add_type(tb.build()?);
    file.build()
}

// ---------------------------------------------------------------------------
// Alias -> typealias
// ---------------------------------------------------------------------------

fn emit_alias(
    schema: &IrSchema,
    expr: &IrTypeExpr,
    package_name: &str,
) -> Result<FileSpec, SigilStitchError> {
    let name = schema.name.to_pascal_case();
    let rhs = kt_type_str(expr);
    emit_typealias(&name, &rhs, schema.description.as_deref(), package_name)
}

// ---------------------------------------------------------------------------
// Union -> sealed class with typed cases
// ---------------------------------------------------------------------------

fn emit_union(
    schema: &IrSchema,
    union: &IrUnion,
    package_name: &str,
) -> Result<FileSpec, SigilStitchError> {
    emit_closed_sum(
        &plan_union(&schema.name, schema.description.as_deref(), union),
        package_name,
    )
}

// ---------------------------------------------------------------------------
// Intersection -> data class with all merged properties
// ---------------------------------------------------------------------------

fn emit_intersection(
    schema: &IrSchema,
    inter: &IrIntersection,
    package_name: &str,
) -> Result<FileSpec, SigilStitchError> {
    let name = schema.name.to_pascal_case();
    let mut file =
        FileSpec::builder_with("model.kt", Kotlin::new()).header(package_header(package_name)?);

    let mut tb = TypeSpec::builder(&name, TypeKind::Struct).visibility(Visibility::Public);
    if let Some(doc) = &schema.description {
        tb = tb.doc(doc);
    }

    let mut used_names: HashSet<String> = HashSet::new();
    for member in &inter.members {
        let member_type = kt_type_str(member);
        let field_name = unique_name(&member_type.to_lower_camel_case(), &mut used_names);
        tb = tb.add_primary_constructor_param(
            ParameterSpec::builder(&field_name, TypeName::primitive(&member_type))
                .is_property()
                .build()?,
        );
    }

    file = file.add_type(tb.build()?);
    file.build()
}

// ---------------------------------------------------------------------------
// TaggedUnion -> sealed class with typed cases
// ---------------------------------------------------------------------------

fn emit_tagged_union(
    schema: &IrSchema,
    tu: &IrTaggedUnion,
    ir: &IrSpec,
    package_name: &str,
) -> Result<FileSpec, SigilStitchError> {
    emit_closed_sum(
        &plan_tagged_union(&schema.name, schema.description.as_deref(), tu, ir),
        package_name,
    )
}

fn emit_closed_sum(plan: &ClosedSumPlan, package_name: &str) -> Result<FileSpec, SigilStitchError> {
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
        .arg(format!("{adapter_name}::class")),
    );
    for case in &plan.cases {
        sum = sum.add_case(kotlin_closed_sum_case(case)?);
    }

    let mut file =
        FileSpec::builder_with("model.kt", Kotlin::new()).header(package_header(package_name)?);
    for import in kotlin_adapter_imports() {
        file = file.add_import(import);
    }
    file.add_closed_sum(sum.build()?)
        .add_code(kotlin_closed_sum_adapter(plan)?)
        .build()
}

fn kotlin_closed_sum_case(case: &ClosedSumCasePlan) -> Result<ClosedSumCaseSpec, SigilStitchError> {
    let mut builder = ClosedSumCaseSpec::builder(&case.name);
    if let Some(doc) = &case.doc {
        builder = builder.doc(doc);
    }
    if let Some(payload) = &case.payload {
        builder = builder.positional_payload(kotlin_type_name(payload));
    }
    builder.build()
}

fn kotlin_type_name(expr: &IrTypeExpr) -> TypeName {
    match expr {
        IrTypeExpr::Named(name) => TypeName::primitive(&name.to_pascal_case()),
        IrTypeExpr::Primitive(primitive) => TypeName::primitive(kt_primitive(primitive)),
        IrTypeExpr::Array(inner) => TypeName::application(
            TypeName::primitive("List"),
            vec![TypeArgument::Single(kotlin_type_name(inner))],
        ),
        IrTypeExpr::Map(inner) => TypeName::application(
            TypeName::primitive("Map"),
            vec![
                TypeArgument::Single(TypeName::primitive("String")),
                TypeArgument::Single(kotlin_type_name(inner)),
            ],
        ),
        IrTypeExpr::Nullable(inner) => TypeName::optional(kotlin_type_name(inner)),
        IrTypeExpr::StringLiteral(_) | IrTypeExpr::StringEnum(_) => TypeName::primitive("String"),
        IrTypeExpr::Union(_) | IrTypeExpr::Any => {
            TypeName::importable("com.google.gson", "JsonElement")
        }
    }
}

fn kotlin_adapter_imports() -> [ImportSpec; 12] {
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
        ImportSpec::named("com.google.gson", "JsonPrimitive"),
    ]
}

fn kotlin_closed_sum_adapter(plan: &ClosedSumPlan) -> Result<CodeBlock, SigilStitchError> {
    let read_dispatch = kotlin_adapter_read_dispatch(plan)?;
    let write_cases = plan
        .cases
        .iter()
        .map(|case| kotlin_adapter_write_case(plan, case))
        .collect::<Result<Vec<_>, _>>()?;
    let adapter_name = format!("{}JsonAdapter", plan.name);
    let root_name = plan.name.as_str();

    sigil_quote!(Kotlin {
        private class $N(adapter_name) : TypeAdapter<$N(root_name)>() {
            private val gson = Gson()

            override fun read(reader: JsonReader): $N(root_name)? {
                val element = JsonParser.parseReader(reader)
                $C(read_dispatch)
            }

            override fun write(writer: JsonWriter, value: $N(root_name)?) {
                if (value == null) {
                    writer.nullValue()
                    return
                }
                when (value) {
                    $C_each(write_cases)
                }
            }
        }
    })
}

fn kotlin_adapter_read_dispatch(plan: &ClosedSumPlan) -> Result<CodeBlock, SigilStitchError> {
    match &plan.encoding {
        ClosedSumEncoding::Untagged => {
            let null_case = plan.cases.iter().find(|case| case.is_null);
            let null_result = match null_case {
                Some(case) => sigil_quote!(Kotlin {
                    $N(plan.name.as_str()).$N(case.name.as_str())
                })?,
                None => sigil_quote!(Kotlin { null })?,
            };
            let mut branches = vec![sigil_quote!(Kotlin {
                element.isJsonNull -> $C(null_result)
            })?];
            for case in plan.cases.iter().filter(|case| !case.is_null) {
                let Some(payload) = &case.payload else {
                    continue;
                };
                let condition = kotlin_json_condition(payload);
                let source = sigil_quote!(Kotlin { element })?;
                let result = kotlin_adapter_read_value(plan, case, payload, source)?;
                branches.push(sigil_quote!(Kotlin {
                    $L(condition.as_str()) -> $C(result)
                })?);
            }
            sigil_quote!(Kotlin {
                return when {
                    $C_each(branches)
                    else -> throw JsonParseException($S(format!("unknown {} union value", plan.name)))
                }
            })
        }
        ClosedSumEncoding::Internal {
            discriminator_field,
        } => {
            let branches = plan
                .cases
                .iter()
                .map(|case| kotlin_tagged_read_case(plan, case, discriminator_field, None))
                .collect::<Result<Vec<_>, _>>()?;
            sigil_quote!(Kotlin {
                val jsonObject = element.asJsonObject
                return when (jsonObject[$S(discriminator_field)]?.asString) {
                    $C_each(branches)
                    else -> throw JsonParseException($S(format!("unknown {} discriminator", plan.name)))
                }
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
                    kotlin_tagged_read_case(plan, case, discriminator_field, Some(content_field))
                })
                .collect::<Result<Vec<_>, _>>()?;
            sigil_quote!(Kotlin {
                val jsonObject = element.asJsonObject
                return when (jsonObject[$S(discriminator_field)]?.asString) {
                    $C_each(branches)
                    else -> throw JsonParseException($S(format!("unknown {} discriminator", plan.name)))
                }
            })
        }
        ClosedSumEncoding::External => {
            let branches = plan
                .cases
                .iter()
                .map(|case| kotlin_external_read_case(plan, case))
                .collect::<Result<Vec<_>, _>>()?;
            sigil_quote!(Kotlin {
                val jsonObject = element.asJsonObject
                return when {
                    $C_each(branches)
                    else -> throw JsonParseException($S(format!("unknown {} variant", plan.name)))
                }
            })
        }
    }
}

fn kotlin_tagged_read_case(
    plan: &ClosedSumPlan,
    case: &ClosedSumCasePlan,
    _discriminator_field: &str,
    content_field: Option<&String>,
) -> Result<CodeBlock, SigilStitchError> {
    let wire_name = case.wire_name.as_deref().unwrap_or_default();
    let source = match content_field {
        Some(content_field) => sigil_quote!(Kotlin { jsonObject[$S(content_field.as_str())] })?,
        None => sigil_quote!(Kotlin { jsonObject })?,
    };
    let result = match &case.payload {
        Some(payload) => kotlin_adapter_read_value(plan, case, payload, source)?,
        None => sigil_quote!(Kotlin {
            $N(plan.name.as_str()).$N(case.name.as_str())
        })?,
    };
    sigil_quote!(Kotlin {
        $S(wire_name) -> $C(result)
    })
}

fn kotlin_external_read_case(
    plan: &ClosedSumPlan,
    case: &ClosedSumCasePlan,
) -> Result<CodeBlock, SigilStitchError> {
    let wire_name = case.wire_name.as_deref().unwrap_or_default();
    let source = sigil_quote!(Kotlin { jsonObject[$S(wire_name)] })?;
    let result = match &case.payload {
        Some(payload) => kotlin_adapter_read_value(plan, case, payload, source)?,
        None => sigil_quote!(Kotlin {
            $N(plan.name.as_str()).$N(case.name.as_str())
        })?,
    };
    sigil_quote!(Kotlin {
        jsonObject.has($S(wire_name)) -> $C(result)
    })
}

fn kotlin_adapter_read_value(
    plan: &ClosedSumPlan,
    case: &ClosedSumCasePlan,
    payload: &IrTypeExpr,
    source: CodeBlock,
) -> Result<CodeBlock, SigilStitchError> {
    let payload_type = kotlin_type_name(payload);
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
        return sigil_quote!(Kotlin {
            $N(root_name).$N(case_name)($C(source).asString)
        });
    }
    if matches!(payload, IrTypeExpr::Primitive(IrPrimitive::Boolean)) {
        return sigil_quote!(Kotlin {
            $N(root_name).$N(case_name)($C(source).asBoolean)
        });
    }
    if matches!(payload, IrTypeExpr::Primitive(IrPrimitive::Integer)) {
        return sigil_quote!(Kotlin {
            $N(root_name).$N(case_name)($C(source).asInt)
        });
    }
    if matches!(payload, IrTypeExpr::Primitive(IrPrimitive::IntegerWithFormat(format)) if format == "int64")
    {
        return sigil_quote!(Kotlin {
            $N(root_name).$N(case_name)($C(source).asLong)
        });
    }
    if matches!(
        payload,
        IrTypeExpr::Primitive(IrPrimitive::IntegerWithFormat(_))
    ) {
        return sigil_quote!(Kotlin {
            $N(root_name).$N(case_name)($C(source).asInt)
        });
    }
    if matches!(payload, IrTypeExpr::Primitive(IrPrimitive::NumberWithFormat(format)) if format == "float")
    {
        return sigil_quote!(Kotlin {
            $N(root_name).$N(case_name)($C(source).asFloat)
        });
    }
    if matches!(
        payload,
        IrTypeExpr::Primitive(IrPrimitive::Number | IrPrimitive::NumberWithFormat(_))
    ) {
        return sigil_quote!(Kotlin {
            $N(root_name).$N(case_name)($C(source).asDouble)
        });
    }
    sigil_quote!(Kotlin {
        $N(root_name).$N(case_name)(gson.fromJson($C(source), object : TypeToken<$T(payload_type)>() {}.type))
    })
}

fn kotlin_adapter_write_case(
    plan: &ClosedSumPlan,
    case: &ClosedSumCasePlan,
) -> Result<CodeBlock, SigilStitchError> {
    let root_name = plan.name.as_str();
    let case_name = case.name.as_str();
    let body = match &plan.encoding {
        ClosedSumEncoding::Untagged => {
            if case.is_null {
                sigil_quote!(Kotlin { writer.nullValue() })?
            } else {
                sigil_quote!(Kotlin {
                    gson.toJson(gson.toJsonTree(value.value0), writer)
                })?
            }
        }
        ClosedSumEncoding::Internal {
            discriminator_field,
        } => {
            let wire_name = case.wire_name.as_deref().unwrap_or_default();
            if case.payload.is_some() {
                sigil_quote!(Kotlin {
                    val jsonObject = gson.toJsonTree(value.value0).asJsonObject
                    jsonObject.addProperty($S(discriminator_field), $S(wire_name))
                    gson.toJson(jsonObject, writer)
                })?
            } else {
                sigil_quote!(Kotlin {
                    val jsonObject = JsonObject()
                    jsonObject.addProperty($S(discriminator_field), $S(wire_name))
                    gson.toJson(jsonObject, writer)
                })?
            }
        }
        ClosedSumEncoding::Adjacent {
            discriminator_field,
            content_field,
        } => {
            let wire_name = case.wire_name.as_deref().unwrap_or_default();
            if case.payload.is_some() {
                sigil_quote!(Kotlin {
                    val jsonObject = JsonObject()
                    jsonObject.addProperty($S(discriminator_field), $S(wire_name))
                    jsonObject.add($S(content_field), gson.toJsonTree(value.value0))
                    gson.toJson(jsonObject, writer)
                })?
            } else {
                sigil_quote!(Kotlin {
                    val jsonObject = JsonObject()
                    jsonObject.addProperty($S(discriminator_field), $S(wire_name))
                    gson.toJson(jsonObject, writer)
                })?
            }
        }
        ClosedSumEncoding::External => {
            let wire_name = case.wire_name.as_deref().unwrap_or_default();
            if case.payload.is_some() {
                sigil_quote!(Kotlin {
                    val jsonObject = JsonObject()
                    jsonObject.add($S(wire_name), gson.toJsonTree(value.value0))
                    gson.toJson(jsonObject, writer)
                })?
            } else {
                sigil_quote!(Kotlin {
                    val jsonObject = JsonObject()
                    jsonObject.add($S(wire_name), JsonNull.INSTANCE)
                    gson.toJson(jsonObject, writer)
                })?
            }
        }
    };
    sigil_quote!(Kotlin {
        is $N(root_name).$N(case_name) -> {
            $C(body)
        }
    })
}

fn kotlin_json_condition(expr: &IrTypeExpr) -> String {
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
            "element.isJsonPrimitive && element.asJsonPrimitive.isString".to_string()
        }
        IrTypeExpr::Primitive(IrPrimitive::Boolean) => {
            "element.isJsonPrimitive && element.asJsonPrimitive.isBoolean".to_string()
        }
        IrTypeExpr::Primitive(
            IrPrimitive::Integer
            | IrPrimitive::IntegerWithFormat(_)
            | IrPrimitive::Number
            | IrPrimitive::NumberWithFormat(_),
        ) => "element.isJsonPrimitive && element.asJsonPrimitive.isNumber".to_string(),
        IrTypeExpr::Array(_) => "element.isJsonArray".to_string(),
        IrTypeExpr::Any | IrTypeExpr::Union(_) => "true".to_string(),
        _ => "element.isJsonObject".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn emit_typealias(
    name: &str,
    rhs: &str,
    doc: Option<&str>,
    package_name: &str,
) -> Result<FileSpec, SigilStitchError> {
    let mut file =
        FileSpec::builder_with("model.kt", Kotlin::new()).header(package_header(package_name)?);

    let mut tb = TypeSpec::builder(name, TypeKind::TypeAlias).visibility(Visibility::Public);
    if let Some(d) = doc {
        tb = tb.doc(d);
    }
    tb = tb.extends(TypeName::primitive(rhs));

    file = file.add_type(tb.build()?);
    file.build()
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
        let adapter = kotlin_closed_sum_adapter(&plan).unwrap();
        assert_rendered!(
            Kotlin::new(),
            width = 100,
            adapter,
            r#"private class ChoiceJsonAdapter: TypeAdapter<Choice>() {
    private val gson = Gson()

    override fun read (reader: JsonReader): Choice? {
        val element = JsonParser.parseReader(reader)
        return when {
            element.isJsonNull -> null

            element.isJsonPrimitive && element.asJsonPrimitive.isString -> Choice.VariantString(element
            .asString)

            else -> throw JsonParseException("unknown Choice union value")
        }

    }

    override fun write(writer: JsonWriter, value: Choice?) {
        if (value == null) {
            writer.nullValue()
            return
        }
        when (value) {
            is Choice.VariantString -> {
                gson.toJson(gson.toJsonTree(value.value0), writer)

            }
        }
    }
}
"#,
        );
    }
}
