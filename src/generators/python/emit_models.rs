//! Shared model emission for both Python transports (dataclasses/enums/aliases).
//!
//! Uses sigil-stitch high-level APIs (TypeSpec, FieldSpec, FunSpec, TypeName,
//! sigil_quote!) for structured code generation with automatic import tracking.
//! Each schema produces one `.py` file via `FileSpec`.

use super::PYTHON_LINE_LENGTH;
use super::additional_properties::AdditionalProperties;
use super::value::{Direction, UntaggedUnionStrategy, ValuePlan, optional_type};
use crate::codegen::traits::file_writer::FileInfo;
pub use crate::generators::python::type_names::python_type_name;
use crate::generators::request_inputs::{RequestInputModel, RequestInputPlan};
use crate::generators::type_application;
use crate::generators::{ArtifactKind, GeneratorError};
use crate::ir::types::{
    IrEnum, IrEnumValueType, IrIntersection, IrObject, IrPrimitive, IrProperty, IrSchema,
    IrSchemaKind, IrSpec, IrTaggedUnion, IrTaggedVariant, IrTypeExpr, IrUnion, TaggingStyle,
};
use heck::{ToPascalCase, ToSnakeCase};
use sigil_stitch::code_block::{CodeBlock, StringLitArg};
use sigil_stitch::lang::python::{Python, keyword_argument};
use sigil_stitch::prelude::*;

/// Generate every model file from the IR.
pub fn generate_model_files(
    ir: &IrSpec,
    header: &str,
    request_inputs: &RequestInputPlan,
) -> Result<Vec<FileInfo>, GeneratorError> {
    generate_model_files_with_strategy(
        ir,
        header,
        request_inputs,
        UntaggedUnionStrategy::FirstMatch,
    )
}

/// Generate models using an explicit untagged-union conversion policy.
pub fn generate_model_files_with_strategy(
    ir: &IrSpec,
    header: &str,
    request_inputs: &RequestInputPlan,
    strategy: UntaggedUnionStrategy,
) -> Result<Vec<FileInfo>, GeneratorError> {
    let values = ValuePlan::new(ir, strategy);
    generate_model_files_with_plan(ir, header, request_inputs, &values)
}

pub(super) fn generate_model_files_with_plan(
    ir: &IrSpec,
    header: &str,
    request_inputs: &RequestInputPlan,
    values: &ValuePlan<'_>,
) -> Result<Vec<FileInfo>, GeneratorError> {
    let mut files = Vec::new();
    for (_name, schema) in &ir.schemas {
        let body = if values.tagged.contains(&schema.name) {
            super::tagged_wire::emit(schema, values)
                .and_then(|file| file.render(PYTHON_LINE_LENGTH))
                .map_err(|error| {
                    GeneratorError::emission(ArtifactKind::Model, &schema.name, error)
                })?
        } else {
            emit_model_body(schema, ir, values).ok_or_else(|| {
                GeneratorError::UnsupportedSchema {
                    schema: schema.name.clone(),
                    kind: format!("{:?}", schema.kind),
                }
            })?
        };
        let stem = schema.name.to_snake_case();
        let filename = format!("{stem}.py");
        let mut content = String::with_capacity(header.len() + body.len());
        content.push_str(header);
        content.push_str(&body);
        files.push(FileInfo::model(filename, content));
    }
    for model in request_inputs.models() {
        files.push(request_input_model_file(model, header, values));
    }
    files.extend(values.files(header)?);
    Ok(files)
}

fn request_input_model_file(
    model: &RequestInputModel,
    header: &str,
    values: &ValuePlan<'_>,
) -> FileInfo {
    let mut declaration =
        TypeSpec::builder(&model.name.to_pascal_case(), TypeKind::Class).annotate(
            AnnotationSpec::importable(TypeName::importable("dataclasses", "dataclass")),
        );
    for field in model
        .fields
        .iter()
        .filter(|field| field.required)
        .chain(model.fields.iter().filter(|field| !field.required))
    {
        let ty = if field.is_upload() {
            TypeName::importable("..runtime", "UploadFile")
        } else {
            values.model_type(&field.type_expr, &model.name)
        };
        let mut spec = FieldSpec::builder(
            &python_field_name(&field.wire_name),
            if field.required {
                ty
            } else {
                optional_type(ty)
            },
        );
        if !field.required {
            spec = spec.initializer(
                sigil_quote!(Python { None })
                    .expect("Python optional request input field default builds"),
            );
        }
        declaration =
            declaration.add_field(spec.build().expect("Python request input field builds"));
    }
    let filename = format!("{}.py", model.name.to_snake_case());
    let source = FileSpec::builder_with(&filename, Python::new())
        .header(future_annotations_header())
        .add_type(
            declaration
                .build()
                .expect("Python request input declaration builds"),
        )
        .build()
        .expect("Python request input file builds")
        .render(PYTHON_LINE_LENGTH)
        .expect("Python request input renders");
    FileInfo::model(filename, format!("{header}{source}"))
}

fn emit_model_body(schema: &IrSchema, ir: &IrSpec, values: &ValuePlan<'_>) -> Option<String> {
    let file_spec = match &schema.kind {
        IrSchemaKind::Object(obj) => emit_object(schema, obj, ir, values),
        IrSchemaKind::Enum(en) => emit_enum(schema, en),
        IrSchemaKind::Alias(expr) => emit_alias(schema, expr, values),
        IrSchemaKind::Union(u) => emit_union(schema, u, values),
        IrSchemaKind::Intersection(i) => emit_intersection(schema, i, ir, values),
        IrSchemaKind::TaggedUnion(tu) => emit_tagged_union(schema, tu, ir, values),
    }?;
    file_spec.render(PYTHON_LINE_LENGTH).ok()
}

pub fn future_annotations_header() -> CodeBlock {
    sigil_quote!(Python { from __future__ import annotations }).expect("static header")
}

// ---------------------------------------------------------------------------
// Object -> @dataclass
// ---------------------------------------------------------------------------

fn emit_object(
    schema: &IrSchema,
    obj: &IrObject,
    ir: &IrSpec,
    values: &ValuePlan<'_>,
) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();
    let additional = AdditionalProperties::new(obj);

    let mut file =
        FileSpec::builder_with("model.py", Python::new()).header(future_annotations_header());

    if needs_typing_literal_in_props(&obj.properties)
        || obj
            .additional_properties
            .as_ref()
            .is_some_and(needs_typing_literal)
    {
        file = file.add_import(ImportSpec::named("typing", "Literal"));
    }
    if obj
        .properties
        .values()
        .any(|property| values.needs_conversion(&property.type_expr))
        || additional.is_some()
    {
        file = file.add_import(ImportSpec::named(".", "_union"));
    }

    let dataclass_tn = TypeName::importable("dataclasses", "dataclass");
    let mut cls = TypeSpec::builder(&name, TypeKind::Class)
        .annotate(AnnotationSpec::importable(dataclass_tn));

    if let Some(doc) = &schema.description {
        cls = cls.doc(&format!("{}.", escape_docstring(doc)));
    }

    let mut required: Vec<(&String, &IrProperty)> = Vec::new();
    let mut optional: Vec<(&String, &IrProperty)> = Vec::new();
    for (json_name, prop) in &obj.properties {
        if prop.required && !prop.nullable {
            required.push((json_name, prop));
        } else {
            optional.push((json_name, prop));
        }
    }

    let all_fields: Vec<(&String, &IrProperty)> =
        required.iter().chain(optional.iter()).copied().collect();

    if all_fields.is_empty() && additional.is_none() {
        cls = cls.extra_member(sigil_quote!(Python { pass }).expect("pass"));
    } else {
        for (_json_name, prop) in &required {
            let field_name = python_field_name(&prop.name);
            let type_name = values.model_type(&prop.type_expr, &schema.name);
            cls = cls.add_field(
                FieldSpec::builder(&field_name, type_name)
                    .build()
                    .expect("required field"),
            );
        }
        for (_json_name, prop) in &optional {
            let field_name = python_field_name(&prop.name);
            let type_name = values.model_type(&prop.type_expr, &schema.name);
            cls = cls.add_field(
                FieldSpec::builder(&field_name, optional_type(type_name))
                    .initializer(optional_property_default(prop, values))
                    .build()
                    .expect("optional field"),
            );
        }

        if let Some(additional) = &additional {
            cls = cls.add_field(additional.field(&schema.name, values));
        }

        cls = cls.add_method(build_to_dict_method(
            &all_fields,
            ir,
            values,
            &schema.name,
            additional.as_ref(),
        ));
        cls = cls.add_method(build_from_dict_method(
            &name,
            &all_fields,
            ir,
            values,
            &schema.name,
            additional.as_ref(),
        ));
    }

    file = file.add_type(cls.build().ok()?);
    file.build().ok()
}

fn optional_property_default(prop: &IrProperty, values: &ValuePlan<'_>) -> CodeBlock {
    if values.optional_raw_tagged_field(prop) {
        sigil_quote!(Python { _union.AbsentRawValue.ABSENT })
    } else {
        sigil_quote!(Python { None })
    }
    .expect("Python optional property default builds")
}

fn build_to_dict_method(
    all_fields: &[(&String, &IrProperty)],
    ir: &IrSpec,
    values: &ValuePlan<'_>,
    schema_name: &str,
    additional: Option<&AdditionalProperties<'_>>,
) -> FunSpec {
    let self_param = ParameterSpec::of("self", TypeName::primitive(""));
    let return_type = type_application(
        TypeName::primitive("dict"),
        vec![TypeName::primitive("str"), TypeName::primitive("object")],
    );

    let mut body = CodeBlock::builder();
    body.add_code(
        sigil_quote!(Python { result: dict[str, object] = {} }).expect("result initialization"),
    );
    for (json_name, prop) in all_fields {
        let field_name = python_field_name(&prop.name);
        if values.needs_conversion(&prop.type_expr) {
            let access = sigil_quote!(Python { self.$N(field_name.as_str()) })
                .expect("Python planned field access builds");
            let converted = values.call(&values.property_expr(prop), access, Direction::To);
            let raw_optional = values.optional_raw_tagged_field(prop);
            // Newly planned object paths retain ordinary nullable-field omission.
            let is_required =
                prop.required && (values.has_union(&prop.type_expr) || !prop.nullable);
            body.add_code(
                sigil_quote!(Python {
                    $if(is_required) {
                        result[$S(json_name.as_str())] = $C(converted)
                    } $else_if(raw_optional) {
                        if self.$N(field_name) is not _union.AbsentRawValue.ABSENT {
                            result[$S(json_name.as_str())] = $C(converted)
                        }
                    } $else {
                        if self.$N(field_name) is not None {
                            result[$S(json_name.as_str())] = $C(converted)
                        }
                    }
                })
                .expect("Python planned field serialization builds"),
            );
            continue;
        }
        let access = sigil_quote!(Python { self.$N(field_name.as_str()) })
            .expect("Python recursive model field access builds");
        if let Some(converted) =
            values.model_field_conversion(&prop.type_expr, access, schema_name, Direction::To)
        {
            body.add_code(
                sigil_quote!(Python {
                    $if(prop.required && !prop.nullable) {
                        result[$S(json_name.as_str())] = $C(converted)
                    } $else {
                        if self.$N(field_name) is not None {
                            result[$S(json_name.as_str())] = $C(converted)
                        }
                    }
                })
                .expect("Python recursive model field serialization builds"),
            );
            continue;
        }
        let access = sigil_quote!(Python { self.$N(field_name.as_str()) })
            .expect("Python ordinary field access builds");
        let to_expr = render_to_dict_expr(access.clone(), &prop.type_expr, ir);
        let statement = if prop.required && !prop.nullable {
            sigil_quote!(Python { result[$S(json_name.as_str())] = $C(to_expr) })
                .expect("Python required field serialization builds")
        } else {
            sigil_quote!(Python {
                if $C(access) is not None {
                    result[$S(json_name.as_str())] = $C(to_expr)
                }
            })
            .expect("Python optional field serialization builds")
        };
        body.add_code(statement);
    }
    if let Some(additional) = additional {
        body.add_code(additional.write_entries(values));
    }
    body.add_code(sigil_quote!(Python { return result }).expect("return result"));

    FunSpec::builder("to_dict")
        .add_param(self_param)
        .returns(vec![return_type])
        .body(body.build().expect("to_dict body"))
        .build()
        .expect("to_dict method")
}

fn build_from_dict_method(
    class_name: &str,
    all_fields: &[(&String, &IrProperty)],
    ir: &IrSpec,
    values: &ValuePlan<'_>,
    schema_name: &str,
    additional: Option<&AdditionalProperties<'_>>,
) -> FunSpec {
    let cls_param = ParameterSpec::of("cls", TypeName::primitive(""));
    let data_param = ParameterSpec::of(
        "data",
        type_application(
            TypeName::primitive("dict"),
            vec![TypeName::primitive("str"), TypeName::primitive("object")],
        ),
    );

    let mut body = CodeBlock::builder();
    body.add_statement("return cls(%>", ());
    for (json_name, prop) in all_fields {
        let field_name = python_field_name(&prop.name);
        let is_required = prop.required && !prop.nullable;
        if values.needs_conversion(&prop.type_expr) {
            let has_union = values.has_union(&prop.type_expr);
            let raw_optional = values.optional_raw_tagged_field(prop);
            let requires_key = if has_union {
                prop.required
            } else {
                is_required
            };
            let access = if requires_key {
                sigil_quote!(Python { data[$S(json_name.as_str())] })
            } else {
                sigil_quote!(Python { data.get($S(json_name.as_str())) })
            }
            .expect("Python planned field wire access builds");
            let expression = values.property_expr(prop);
            let converted = values.call(&expression, access.clone(), Direction::From);
            let ty = values.model_type(&prop.type_expr, schema_name);
            let ty = if is_required { ty } else { optional_type(ty) };
            // Union fields keep their existing explicit-null policy. Ordinary
            // optional/nullable objects never pass None into their converter.
            let converted = sigil_quote!(Python {
                $if(requires_key) { $C(converted) }
                $else_if(raw_optional) { _union.AbsentRawValue.ABSENT if $S(json_name.as_str()) not in data else $C(converted) }
                $else_if(has_union) { None if $S(json_name.as_str()) not in data else $C(converted) }
                $else { None if $C(access) is None else $C(converted) }
            }).expect("Python optional planned field projection builds");
            let converted = values.coerce(&expression, ty, converted);
            let label = sigil_quote!(Python { $N(field_name) })
                .expect("Python planned keyword label builds");
            let argument = keyword_argument(label, converted);
            body.add_code(
                sigil_quote!(Python { $C(argument), })
                    .expect("Python planned field deserialization builds"),
            );
            continue;
        }
        let access = if is_required {
            sigil_quote!(Python { data[$S(json_name.as_str())] })
        } else {
            sigil_quote!(Python { data.get($S(json_name.as_str())) })
        }
        .expect("Python recursive model wire field access builds");
        if let Some(converted) = values.model_field_conversion(
            &prop.type_expr,
            access.clone(),
            schema_name,
            Direction::From,
        ) {
            let value = sigil_quote!(Python {
                $if(is_required) { $C(converted) }
                $else { $C(converted) if $C(access) is not None else None }
            })
            .expect("Python recursive model field value builds");
            let label = sigil_quote!(Python { $N(field_name) })
                .expect("Python recursive model keyword label builds");
            let argument = keyword_argument(label, value);
            body.add_code(
                sigil_quote!(Python { $C(argument), })
                    .expect("Python recursive model field deserialization builds"),
            );
            continue;
        }
        let (expr, comment) = render_from_dict_expr(json_name, &prop.type_expr, is_required, ir);
        let label = sigil_quote!(Python { $N(field_name.as_str()) })
            .expect("Python ordinary keyword label builds");
        let argument = keyword_argument(label, expr);
        let argument = if let Some(comment) = comment {
            sigil_quote!(Python { $C(argument), $comment(comment) })
        } else {
            sigil_quote!(Python { $C(argument), })
        }
        .expect("Python ordinary keyword argument builds");
        body.add_code(argument);
    }
    if let Some(additional) = additional {
        body.add_code(additional.read_entries(schema_name, values));
    }
    body.add("%<", ());
    body.add_statement(")", ());

    FunSpec::builder("from_dict")
        .annotate(AnnotationSpec::new("classmethod"))
        .add_param(cls_param)
        .add_param(data_param)
        .returns(vec![TypeName::primitive(class_name)])
        .body(body.build().expect("from_dict body"))
        .build()
        .expect("from_dict method")
}

// ---------------------------------------------------------------------------
// Enum -> class(str, Enum) or class(int, Enum)
// ---------------------------------------------------------------------------

fn emit_enum(schema: &IrSchema, en: &IrEnum) -> Option<FileSpec> {
    if en.value_type == IrEnumValueType::Mixed {
        return emit_type_alias_raw(schema, "object");
    }

    let name = schema.name.to_pascal_case();
    let base = match en.value_type {
        IrEnumValueType::String => TypeName::primitive("str"),
        IrEnumValueType::Integer | IrEnumValueType::Number => TypeName::primitive("int"),
        IrEnumValueType::Mixed => unreachable!(),
    };

    let mut ts = TypeSpec::builder(&name, TypeKind::Enum)
        .extends(base)
        .extends(TypeName::importable("enum", "Enum"));

    if let Some(doc) = &schema.description {
        ts = ts.doc(&format!("{}.", escape_docstring(doc)));
    }

    for v in &en.values {
        let (member_name, value_code) = match en.value_type {
            IrEnumValueType::String => {
                let s = v.value.as_str()?;
                (
                    python_enum_member_name(s),
                    format!("\"{}\"", escape_python_string(s)),
                )
            }
            IrEnumValueType::Integer | IrEnumValueType::Number => {
                let n = v
                    .value
                    .as_i64()
                    .or_else(|| v.value.as_f64().map(|f| f as i64))?;
                (format!("N{n}").replace('-', "NEG"), format!("{n}"))
            }
            IrEnumValueType::Mixed => unreachable!(),
        };
        ts = ts.add_variant(
            EnumVariantSpec::builder(&member_name)
                .discriminant(CodeBlock::of(&value_code, ()).expect("enum value"))
                .build()
                .expect("enum variant"),
        );
    }

    let file = FileSpec::builder_with("model.py", Python::new())
        .header(future_annotations_header())
        .add_type(ts.build().ok()?);
    file.build().ok()
}

// ---------------------------------------------------------------------------
// Alias -> type X = Y (PEP 695)
// ---------------------------------------------------------------------------

fn emit_alias(schema: &IrSchema, expr: &IrTypeExpr, values: &ValuePlan<'_>) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();
    let rhs_type = values.model_type(expr, &schema.name);
    let mut file =
        FileSpec::builder_with("model.py", Python::new()).header(future_annotations_header());
    if needs_typing_literal(expr) {
        file = file.add_import(ImportSpec::named("typing", "Literal"));
    }
    if let Some(doc) = &schema.description {
        file = file.add_raw(&format!("# {}\n", escape_docstring(doc)));
    }
    file = file.add_type(
        TypeSpec::builder(&name, TypeKind::TypeAlias)
            .extends(rhs_type)
            .build()
            .ok()?,
    );
    file.build().ok()
}

fn emit_type_alias_raw(schema: &IrSchema, rhs: &str) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();
    let mut file =
        FileSpec::builder_with("model.py", Python::new()).header(future_annotations_header());
    if let Some(doc) = &schema.description {
        file = file.add_raw(&format!("# {}\n", escape_docstring(doc)));
    }
    file = file.add_type(
        TypeSpec::builder(&name, TypeKind::TypeAlias)
            .extends(TypeName::primitive(rhs))
            .build()
            .ok()?,
    );
    file.build().ok()
}

pub(super) fn type_alias_spec(name: &str, members: Vec<TypeName>) -> TypeSpec {
    let target = match members.len() {
        0 => TypeName::importable("typing", "Any"),
        1 => members.into_iter().next().expect("one alias member"),
        _ => TypeName::union(members),
    };
    TypeSpec::builder(name, TypeKind::TypeAlias)
        .extends(target)
        .build()
        .expect("Python type alias builds")
}

fn emit_union(schema: &IrSchema, u: &IrUnion, values: &ValuePlan<'_>) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();

    let mut members: Vec<TypeName> = if values.strategy == UntaggedUnionStrategy::Raw {
        vec![TypeName::primitive("object")]
    } else {
        u.members
            .iter()
            .map(|member| values.model_type(member, &schema.name))
            .collect()
    };
    if u.nullable {
        members.push(TypeName::primitive("None"));
    }

    let type_alias = type_alias_spec(&name, members.clone());

    let mut file =
        FileSpec::builder_with("model.py", Python::new()).header(future_annotations_header());
    if needs_typing_literal_in_exprs(&u.members) {
        file = file.add_import(ImportSpec::named("typing", "Literal"));
    }
    if let Some(doc) = &schema.description {
        file = file.add_raw(&format!("# {}\n", escape_docstring(doc)));
    }
    file = file
        .add_type(type_alias)
        .add_import(ImportSpec::named(".", "_union"));
    let expr = IrTypeExpr::Named(schema.name.clone());
    for direction in [Direction::From, Direction::To] {
        let helper = format!(
            "{}_{}",
            schema.name.to_snake_case(),
            match direction {
                Direction::From => "from_dict",
                Direction::To => "to_dict",
            }
        );
        let converted = values.call(
            &expr,
            sigil_quote!(Python { value }).expect("union helper input builds"),
            direction,
        );
        let output = match direction {
            Direction::From => TypeName::primitive(&name),
            Direction::To => TypeName::primitive("object"),
        };
        let body = match direction {
            Direction::From => {
                let converted = values.coerce(&expr, output.clone(), converted);
                sigil_quote!(Python { return $C(converted) })
            }
            Direction::To => sigil_quote!(Python { return $C(converted) }),
        }
        .expect("Python union helper body builds");
        file = file.add_function(
            FunSpec::builder(&helper)
                .add_param(ParameterSpec::of(
                    "value",
                    match direction {
                        Direction::From => TypeName::primitive("object"),
                        Direction::To => TypeName::primitive(&name),
                    },
                ))
                .returns(vec![output])
                .body(body)
                .build()
                .expect("Python union helper builds"),
        );
    }
    file.build().ok()
}

// ---------------------------------------------------------------------------
// Intersection -> merged @dataclass
// ---------------------------------------------------------------------------

fn emit_intersection(
    schema: &IrSchema,
    inter: &IrIntersection,
    ir: &IrSpec,
    values: &ValuePlan<'_>,
) -> Option<FileSpec> {
    let mut all_props: indexmap::IndexMap<String, IrProperty> = indexmap::IndexMap::new();
    for member in &inter.members {
        if let IrTypeExpr::Named(ref_name) = member
            && let Some(s) = ir.schemas.get(ref_name.as_str())
            && let IrSchemaKind::Object(obj) = &s.kind
        {
            for (k, v) in &obj.properties {
                all_props.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
    }

    if all_props.is_empty() {
        return emit_intersection_as_alias(schema, inter);
    }

    emit_intersection_as_dataclass(schema, &all_props, ir, values)
}

fn emit_intersection_as_alias(schema: &IrSchema, inter: &IrIntersection) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();
    let members: Vec<TypeName> = inter.members.iter().map(python_type_name).collect();

    let type_alias = type_alias_spec(&name, members.clone());

    let mut file =
        FileSpec::builder_with("model.py", Python::new()).header(future_annotations_header());
    if needs_typing_literal_in_exprs(&inter.members) {
        file = file.add_import(ImportSpec::named("typing", "Literal"));
    }
    file = file.add_type(type_alias);
    file.build().ok()
}

fn emit_intersection_as_dataclass(
    schema: &IrSchema,
    all_props: &indexmap::IndexMap<String, IrProperty>,
    ir: &IrSpec,
    values: &ValuePlan<'_>,
) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();

    let mut file =
        FileSpec::builder_with("model.py", Python::new()).header(future_annotations_header());

    if needs_typing_literal_in_props(all_props) {
        file = file.add_import(ImportSpec::named("typing", "Literal"));
    }
    if all_props
        .values()
        .any(|property| values.needs_conversion(&property.type_expr))
    {
        file = file.add_import(ImportSpec::named(".", "_union"));
    }

    let dataclass_tn = TypeName::importable("dataclasses", "dataclass");
    let mut cls = TypeSpec::builder(&name, TypeKind::Class)
        .annotate(AnnotationSpec::importable(dataclass_tn));

    if let Some(doc) = &schema.description {
        cls = cls.doc(&format!("{}.", escape_docstring(doc)));
    }

    let mut required: Vec<(&String, &IrProperty)> = Vec::new();
    let mut optional: Vec<(&String, &IrProperty)> = Vec::new();
    for (json_name, prop) in all_props {
        if prop.required && !prop.nullable {
            required.push((json_name, prop));
        } else {
            optional.push((json_name, prop));
        }
    }

    if required.is_empty() && optional.is_empty() {
        cls = cls.extra_member(sigil_quote!(Python { pass }).expect("pass"));
    } else {
        for (_json_name, prop) in &required {
            let field_name = python_field_name(&prop.name);
            let type_name = values.model_type(&prop.type_expr, &schema.name);
            cls = cls.add_field(
                FieldSpec::builder(&field_name, type_name)
                    .build()
                    .expect("required field"),
            );
        }
        for (_json_name, prop) in &optional {
            let field_name = python_field_name(&prop.name);
            let type_name = values.model_type(&prop.type_expr, &schema.name);
            cls = cls.add_field(
                FieldSpec::builder(&field_name, optional_type(type_name))
                    .initializer(optional_property_default(prop, values))
                    .build()
                    .expect("optional field"),
            );
        }

        let all_fields: Vec<(&String, &IrProperty)> =
            required.iter().chain(optional.iter()).copied().collect();
        cls = cls.add_method(build_to_dict_method(
            &all_fields,
            ir,
            values,
            &schema.name,
            None,
        ));
        cls = cls.add_method(build_from_dict_method(
            &name,
            &all_fields,
            ir,
            values,
            &schema.name,
            None,
        ));
    }

    file = file.add_type(cls.build().ok()?);
    file.build().ok()
}

// ---------------------------------------------------------------------------
// TaggedUnion -> type X = A | B | C
// ---------------------------------------------------------------------------

fn emit_tagged_union(
    schema: &IrSchema,
    tu: &IrTaggedUnion,
    ir: &IrSpec,
    values: &ValuePlan<'_>,
) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();
    let snake_name = schema.name.to_snake_case();

    let members: Vec<TypeName> = tu
        .variants
        .iter()
        .map(|v| values.model_type(&v.content_type, &schema.name))
        .collect();

    let type_alias = type_alias_spec(&name, members.clone());

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

    let mut file =
        FileSpec::builder_with("model.py", Python::new()).header(future_annotations_header());
    let exprs: Vec<&IrTypeExpr> = tu.variants.iter().map(|v| &v.content_type).collect();
    if exprs.iter().any(|e| needs_typing_literal(e)) {
        file = file.add_import(ImportSpec::named("typing", "Literal"));
    }
    let comments = doc
        .lines()
        .map(|line| sigil_quote!(Python { $comment(line) }))
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    file = file.add_code(sigil_quote!(Python { $C_each(comments) }).ok()?);
    file = file.add_type(type_alias);

    let expr = IrTypeExpr::Named(schema.name.clone());
    if values.has_union(&expr) {
        file = file.add_import(ImportSpec::named(".", "_union"));
        for direction in [Direction::From, Direction::To] {
            let helper = format!(
                "{snake_name}_{}",
                match direction {
                    Direction::From => "from_dict",
                    Direction::To => "to_dict",
                }
            );
            let converted = values.call(
                &expr,
                sigil_quote!(Python { value }).expect("Python tagged payload input builds"),
                direction,
            );
            let output = match direction {
                Direction::From => TypeName::primitive(&name),
                Direction::To => type_application(
                    TypeName::primitive("dict"),
                    vec![TypeName::primitive("str"), TypeName::primitive("object")],
                ),
            };
            let converted = match direction {
                Direction::From => values.coerce(&expr, output.clone(), converted),
                Direction::To => sigil_quote!(Python { $T(TypeName::importable("typing", "cast"))($T(output.clone()), $C(converted)) }).expect("Python tagged output projection builds"),
            };
            file = file.add_function(
                FunSpec::builder(&helper)
                    .add_param(ParameterSpec::of("value", TypeName::primitive("object")))
                    .returns(vec![output])
                    .body(
                        sigil_quote!(Python { return $C(converted) })
                            .expect("Python tagged union payload helper body builds"),
                    )
                    .build()
                    .expect("Python tagged union payload helper builds"),
            );
        }
    } else if !tu.variants.is_empty() {
        for helper in build_tagged_union_helpers(schema, tu, ir, values) {
            file = file.add_function(helper);
        }
    }

    file.build().ok()
}

fn build_tagged_union_helpers(
    schema: &IrSchema,
    tu: &IrTaggedUnion,
    ir: &IrSpec,
    values: &ValuePlan<'_>,
) -> Vec<FunSpec> {
    let name = schema.name.to_pascal_case();
    let snake = schema.name.to_snake_case();
    let tag_field = &tu.discriminator_field;
    let variants: Vec<(&IrTaggedVariant, TypeName)> = tu
        .variants
        .iter()
        .filter_map(|variant| match &variant.content_type {
            IrTypeExpr::Named(name) if is_object_schema(name, ir) => Some((
                variant,
                values.model_type(&variant.content_type, &schema.name),
            )),
            _ => None,
        })
        .collect();
    if variants.is_empty() {
        return Vec::new();
    }
    let dictionary = type_application(
        TypeName::primitive("dict"),
        vec![TypeName::primitive("str"), TypeName::primitive("object")],
    );
    let mut decoders = Vec::new();
    let mut encoders = Vec::new();
    for (variant, model) in variants {
        let tag = &variant.discriminator_value;
        let condition = match &tu.tagging {
            TaggingStyle::Internal | TaggingStyle::Adjacent { .. } => {
                sigil_quote!(Python { _tag == $S(tag.as_str()) })
            }
            TaggingStyle::External => sigil_quote!(Python { $S(tag.as_str()) in data }),
        }
        .expect("Python tagged payload selection builds");
        let input = match &tu.tagging {
            TaggingStyle::Internal => sigil_quote!(Python { data }),
            TaggingStyle::Adjacent { .. } => sigil_quote!(Python {
                $T(TypeName::importable("typing", "cast"))($T(dictionary.clone()), _content)
            }),
            TaggingStyle::External => sigil_quote!(Python {
                $T(TypeName::importable("typing", "cast"))($T(dictionary.clone()), data[$S(tag.as_str())])
            }),
        }
        .expect("Python tagged payload wire access builds");
        decoders.push(
            sigil_quote!(Python {
                if $C(condition) {
                    return $T(model.clone()).from_dict($C(input))
                }
            })
            .expect("Python tagged payload decoder branch builds"),
        );
        let output = match &tu.tagging {
            TaggingStyle::Internal => sigil_quote!(Python {
                result = obj.to_dict()
                result[$S(tag_field.as_str())] = $S(tag.as_str())
                return result
            }),
            TaggingStyle::Adjacent { content_field } => sigil_quote!(Python {
                return {$S(tag_field.as_str()): $S(tag.as_str()), $S(content_field.as_str()): obj.to_dict()}
            }),
            TaggingStyle::External => sigil_quote!(Python {
                return {$S(tag.as_str()): obj.to_dict()}
            }),
        }
        .expect("Python tagged payload wire encoding builds");
        encoders.push(
            sigil_quote!(Python {
                if isinstance(obj, $T(model)) {
                    $C(output)
                }
            })
            .expect("Python tagged payload encoder branch builds"),
        );
    }
    // Preserve the adjacent decoder's eager content lookup, including its
    // KeyError precedence for an unknown discriminator with missing content.
    let content_fields: Vec<&str> = match &tu.tagging {
        TaggingStyle::Adjacent { content_field } => vec![content_field.as_str()],
        _ => Vec::new(),
    };
    let unknown_tag = format!("Unknown discriminator value for {name}: ");
    // Keep `+` on the continuation line when broken; `%L` is only the
    // static operator token, while the message remains a semantic `%S` arg.
    let unknown_tag_message = CodeBlock::of("%S%W%Lstr(data)", (StringLitArg(unknown_tag), "+ "))
        .expect("Python tagged payload unknown-tag message builds");
    let from_body = sigil_quote!(Python {
        $if(!matches!(tu.tagging, TaggingStyle::External)) {
            _tag = data[$S(tag_field.as_str())]
        }
        $for(content_field in &content_fields) {
            _content = data[$S(*content_field)]
        }
        $C_each(decoders)
        raise ValueError($C(unknown_tag_message))
    })
    .expect("Python tagged payload decoder body builds");
    let unknown_variant = format!("Unknown variant for {name}: ");
    let unknown_variant_message = CodeBlock::of(
        "%S%W%Lstr(type(obj))",
        (StringLitArg(unknown_variant), "+ "),
    )
    .expect("Python tagged payload unknown-variant message builds");
    let to_body = sigil_quote!(Python {
        $C_each(encoders)
        raise ValueError($C(unknown_variant_message))
    })
    .expect("Python tagged payload encoder body builds");
    vec![
        FunSpec::builder(&format!("{snake}_from_dict"))
            .add_param(ParameterSpec::of("data", dictionary.clone()))
            .returns(vec![TypeName::primitive(&name)])
            .body(from_body)
            .build()
            .expect("Python tagged payload decoder builds"),
        FunSpec::builder(&format!("{snake}_to_dict"))
            .add_param(ParameterSpec::of("obj", TypeName::primitive(&name)))
            .returns(vec![dictionary])
            .body(to_body)
            .build()
            .expect("Python tagged payload encoder builds"),
    ]
}

// ---------------------------------------------------------------------------
// Type mapping
// ---------------------------------------------------------------------------

/// Like `python_type_name` but Named types import from `..models.{snake}` (for API files).
pub fn api_type_name(expr: &IrTypeExpr) -> TypeName {
    match expr.representation() {
        IrTypeExpr::Named(name) => {
            let py_name = name.to_pascal_case();
            let module = format!("..models.{}", name.to_snake_case());
            TypeName::importable(&module, &py_name)
        }
        IrTypeExpr::Array(inner) => {
            type_application(TypeName::primitive("list"), vec![api_type_name(inner)])
        }
        IrTypeExpr::Map(inner) => type_application(
            TypeName::primitive("dict"),
            vec![TypeName::primitive("str"), api_type_name(inner)],
        ),
        IrTypeExpr::Union(members) => {
            if members.is_empty() {
                TypeName::importable("typing", "Any")
            } else {
                TypeName::union(members.iter().map(api_type_name).collect())
            }
        }
        IrTypeExpr::Nullable(inner) => TypeName::optional(api_type_name(inner)),
        _ => python_type_name(expr),
    }
}

/// Map an IR type expression to a Python type string (for serialization helpers).
pub fn python_type_str(expr: &IrTypeExpr) -> String {
    match expr {
        IrTypeExpr::ScalarEnum { representation, .. } => python_type_str(representation),
        IrTypeExpr::Named(name) => name.to_pascal_case(),
        IrTypeExpr::Primitive(p) => python_primitive(p).to_string(),
        IrTypeExpr::StringLiteral(s) => {
            format!("Literal[\"{}\"]", escape_python_string(s))
        }
        IrTypeExpr::StringEnum(values) => {
            let members: Vec<String> = values
                .iter()
                .map(|v| format!("\"{}\"", escape_python_string(v)))
                .collect();
            format!("Literal[{}]", members.join(", "))
        }
        IrTypeExpr::Array(inner) => {
            let inner_ty = python_type_str(inner);
            format!("list[{inner_ty}]")
        }
        IrTypeExpr::Map(inner) => {
            let inner_ty = python_type_str(inner);
            format!("dict[str, {inner_ty}]")
        }
        IrTypeExpr::Union(members) => {
            let parts: Vec<String> = members.iter().map(python_type_str).collect();
            if parts.is_empty() {
                "Any".to_string()
            } else {
                parts.join(" | ")
            }
        }
        IrTypeExpr::Nullable(inner) => {
            let inner_ty = python_type_str(inner);
            format!("{inner_ty} | None")
        }
        IrTypeExpr::Any => "Any".to_string(),
    }
}

fn python_primitive(p: &IrPrimitive) -> &'static str {
    match p {
        IrPrimitive::String | IrPrimitive::StringWithFormat(_) => "str",
        IrPrimitive::Integer | IrPrimitive::IntegerWithFormat(_) => "int",
        IrPrimitive::Number | IrPrimitive::NumberWithFormat(_) => "float",
        IrPrimitive::Boolean => "bool",
        IrPrimitive::Binary => "bytes",
        IrPrimitive::Date => "datetime.date",
        IrPrimitive::DateTime => "datetime.datetime",
        IrPrimitive::Uuid => "uuid.UUID",
    }
}

fn needs_typing_literal(expr: &IrTypeExpr) -> bool {
    match expr.representation() {
        IrTypeExpr::StringLiteral(_) | IrTypeExpr::StringEnum(_) => true,
        IrTypeExpr::Array(inner) | IrTypeExpr::Map(inner) | IrTypeExpr::Nullable(inner) => {
            needs_typing_literal(inner)
        }
        IrTypeExpr::Union(members) => members.iter().any(needs_typing_literal),
        _ => false,
    }
}

fn needs_typing_literal_in_props(props: &indexmap::IndexMap<String, IrProperty>) -> bool {
    props.values().any(|p| needs_typing_literal(&p.type_expr))
}

fn needs_typing_literal_in_exprs(exprs: &[IrTypeExpr]) -> bool {
    exprs.iter().any(needs_typing_literal)
}

// ---------------------------------------------------------------------------
// Serialization helpers
// ---------------------------------------------------------------------------

fn render_to_dict_expr(access: CodeBlock, type_expr: &IrTypeExpr, ir: &IrSpec) -> CodeBlock {
    if let IrTypeExpr::Nullable(inner) = type_expr {
        return match named_conversion(inner, access.clone(), ir, Direction::To) {
            Some(converted) => sigil_quote!(Python {
                $C(converted) if $C(access) is not None else None
            })
            .expect("Python nullable serialization builds"),
            None => access,
        };
    }
    convert_ordinary(type_expr, access.clone(), ir, Direction::To).unwrap_or(access)
}

fn render_from_dict_expr(
    json_name: &str,
    type_expr: &IrTypeExpr,
    required: bool,
    ir: &IrSpec,
) -> (CodeBlock, Option<&'static str>) {
    let access = if required {
        sigil_quote!(Python { data[$S(json_name)] })
    } else {
        sigil_quote!(Python { data.get($S(json_name)) })
    }
    .expect("Python wire accessor builds");
    let type_expr = match type_expr {
        IrTypeExpr::Nullable(inner) if !required => inner,
        _ => type_expr,
    };
    let Some(converted) = convert_ordinary(type_expr, access.clone(), ir, Direction::From) else {
        return (access, Some("type: ignore[assignment]"));
    };
    let code = if required {
        converted
    } else {
        sigil_quote!(Python { $C(converted) if $C(access) is not None else None })
            .expect("Python optional wire conversion builds")
    };
    (code, None)
}

fn convert_ordinary(
    type_expr: &IrTypeExpr,
    access: CodeBlock,
    ir: &IrSpec,
    direction: Direction,
) -> Option<CodeBlock> {
    match type_expr {
        IrTypeExpr::Array(inner) => {
            let item = sigil_quote!(Python { item }).expect("Python item reference builds");
            let child = named_conversion(inner, item, ir, direction)?;
            let access = if direction == Direction::From {
                wire_cast(
                    type_application(
                        TypeName::primitive("list"),
                        vec![TypeName::primitive("object")],
                    ),
                    access,
                )
            } else {
                access
            };
            Some(
                sigil_quote!(Python { [$C(child) for item in $C(access)] })
                    .expect("Python array conversion builds"),
            )
        }
        IrTypeExpr::Map(inner) => {
            let item = sigil_quote!(Python { v }).expect("Python value reference builds");
            let child = named_conversion(inner, item, ir, direction)?;
            let access = if direction == Direction::From {
                wire_cast(wire_dict_type(), access)
            } else {
                access
            };
            Some(
                sigil_quote!(Python { {k: $C(child) for k, v in $C(access).items()} })
                    .expect("Python map conversion builds"),
            )
        }
        _ => named_conversion(type_expr, access, ir, direction),
    }
}

fn wire_dict_type() -> TypeName {
    type_application(
        TypeName::primitive("dict"),
        vec![TypeName::primitive("str"), TypeName::primitive("object")],
    )
}

fn wire_cast(ty: TypeName, access: CodeBlock) -> CodeBlock {
    sigil_quote!(Python { $T(TypeName::importable("typing", "cast"))($T(ty), $C(access)) })
        .expect("Python wire value type projection builds")
}

fn named_conversion(
    type_expr: &IrTypeExpr,
    access: CodeBlock,
    ir: &IrSpec,
    direction: Direction,
) -> Option<CodeBlock> {
    let IrTypeExpr::Named(name) = type_expr else {
        return None;
    };
    let access = if direction == Direction::From {
        // Keep the decoder's unchecked wire contract without depending on the
        // physical line where a type-ignore comment would end up after wrapping.
        wire_cast(wire_dict_type(), access)
    } else {
        access
    };
    let block = if is_object_schema(name, ir) {
        match direction {
            Direction::From => {
                let class = name.to_pascal_case();
                sigil_quote!(Python { $N(class).from_dict($C(access)) })
            }
            Direction::To => sigil_quote!(Python { $C(access).to_dict() }),
        }
    } else if is_tagged_union_schema(name, ir) {
        let suffix = match direction {
            Direction::From => "from_dict",
            Direction::To => "to_dict",
        };
        let function = format!("{}_{suffix}", name.to_snake_case());
        sigil_quote!(Python { $N(function)($C(access)) })
    } else {
        return None;
    };
    Some(block.expect("Python named conversion builds"))
}

pub fn is_object_schema(name: &str, ir: &IrSpec) -> bool {
    ir.schemas.get(name).is_some_and(|s| match &s.kind {
        IrSchemaKind::Object(_) => true,
        IrSchemaKind::Intersection(inter) => inter.members.iter().any(|m| {
            if let IrTypeExpr::Named(ref_name) = m {
                ir.schemas
                    .get(ref_name.as_str())
                    .is_some_and(|ms| matches!(ms.kind, IrSchemaKind::Object(_)))
            } else {
                false
            }
        }),
        _ => false,
    })
}

pub fn is_tagged_union_schema(name: &str, ir: &IrSpec) -> bool {
    ir.schemas
        .get(name)
        .is_some_and(|s| matches!(s.kind, IrSchemaKind::TaggedUnion(_)))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub fn python_field_name(name: &str) -> String {
    let snake = name.to_snake_case();
    if snake.is_empty() {
        return "field_".to_string();
    }
    match snake.as_str() {
        "and" | "as" | "assert" | "async" | "await" | "break" | "class" | "continue" | "def"
        | "del" | "elif" | "else" | "except" | "finally" | "for" | "from" | "global" | "if"
        | "import" | "in" | "is" | "lambda" | "nonlocal" | "not" | "or" | "pass" | "raise"
        | "return" | "try" | "while" | "with" | "yield" | "type" => {
            format!("{snake}_")
        }
        _ => snake,
    }
}

fn python_enum_member_name(value: &str) -> String {
    let upper = value
        .to_uppercase()
        .replace(|c: char| !c.is_alphanumeric(), "_");
    if upper.is_empty() {
        return "EMPTY".to_string();
    }
    if upper.starts_with(|c: char| c.is_ascii_digit()) {
        return format!("N{upper}");
    }
    upper
}

fn escape_python_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn escape_docstring(s: &str) -> String {
    s.replace("\"\"\"", "\\\"\\\"\\\"")
        .lines()
        .next()
        .unwrap_or("")
        .to_string()
}
