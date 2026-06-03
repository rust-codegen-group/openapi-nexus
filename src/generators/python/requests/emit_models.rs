//! Model emission for IR schemas (Python dataclasses/enums/aliases).
//!
//! Uses sigil-stitch high-level APIs (TypeSpec, FieldSpec, FunSpec, TypeName,
//! sigil_quote!) for structured code generation with automatic import tracking.
//! Each schema produces one `.py` file via `FileSpec`.

use crate::codegen::traits::file_writer::FileInfo;
use crate::ir::types::{
    IrEnum, IrEnumValueType, IrIntersection, IrObject, IrPrimitive, IrProperty, IrSchema,
    IrSchemaKind, IrSpec, IrTaggedUnion, IrTaggedVariant, IrTypeExpr, IrUnion, TaggingStyle,
};
use heck::{ToPascalCase, ToSnakeCase};
use sigil_stitch::code_block::CodeBlock;
use sigil_stitch::lang::python::Python;
use sigil_stitch::prelude::*;

/// Generate every model file from the IR.
pub fn generate_model_files(ir: &IrSpec, header: &str) -> Result<Vec<FileInfo>, String> {
    let mut files = Vec::new();
    for (_name, schema) in &ir.schemas {
        let body = emit_model_body(schema, ir).ok_or_else(|| {
            format!(
                "unsupported schema kind for {}: {:?}",
                schema.name, schema.kind
            )
        })?;
        let stem = schema.name.to_snake_case();
        let filename = format!("{stem}.py");
        let mut content = String::with_capacity(header.len() + body.len());
        content.push_str(header);
        content.push_str(&body);
        files.push(FileInfo::model(filename, content));
    }
    Ok(files)
}

fn emit_model_body(schema: &IrSchema, ir: &IrSpec) -> Option<String> {
    let file_spec = match &schema.kind {
        IrSchemaKind::Object(obj) => emit_object(schema, obj, ir),
        IrSchemaKind::Enum(en) => emit_enum(schema, en),
        IrSchemaKind::Alias(expr) => emit_alias(schema, expr),
        IrSchemaKind::Union(u) => emit_union(schema, u),
        IrSchemaKind::Intersection(i) => emit_intersection(schema, i, ir),
        IrSchemaKind::TaggedUnion(tu) => emit_tagged_union(schema, tu, ir),
    }?;
    file_spec.render(100).ok()
}

pub fn future_annotations_header() -> CodeBlock {
    CodeBlock::of("from __future__ import annotations", ()).expect("static header")
}

// ---------------------------------------------------------------------------
// Object -> @dataclass
// ---------------------------------------------------------------------------

fn emit_object(schema: &IrSchema, obj: &IrObject, ir: &IrSpec) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();

    let mut file =
        FileSpec::builder_with("model.py", Python::new()).header(future_annotations_header());

    if needs_typing_literal_in_props(&obj.properties) {
        file = file.add_import(ImportSpec::named("typing", "Literal"));
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

    if all_fields.is_empty() {
        cls = cls.extra_member(CodeBlock::of("pass", ()).expect("pass"));
    } else {
        for (_json_name, prop) in &required {
            let field_name = python_field_name(&prop.name);
            let type_name = python_type_name(&prop.type_expr);
            cls = cls.add_field(
                FieldSpec::builder(&field_name, type_name)
                    .build()
                    .expect("required field"),
            );
        }
        for (_json_name, prop) in &optional {
            let field_name = python_field_name(&prop.name);
            let type_name = python_type_name(&prop.type_expr);
            cls = cls.add_field(
                FieldSpec::builder(&field_name, TypeName::optional(type_name))
                    .initializer(CodeBlock::of("None", ()).expect("None init"))
                    .build()
                    .expect("optional field"),
            );
        }

        cls = cls.add_method(build_to_dict_method(&all_fields, ir, &obj.properties));
        cls = cls.add_method(build_from_dict_method(
            &name,
            &all_fields,
            ir,
            &obj.properties,
        ));
    }

    file = file.add_type(cls.build().ok()?);
    file.build().ok()
}

fn build_to_dict_method(
    all_fields: &[(&String, &IrProperty)],
    ir: &IrSpec,
    properties: &indexmap::IndexMap<String, IrProperty>,
) -> FunSpec {
    let self_param = ParameterSpec::of("self", TypeName::primitive(""));
    let return_type = TypeName::generic(
        TypeName::primitive("dict"),
        vec![TypeName::primitive("str"), TypeName::primitive("object")],
    );

    let mut body = CodeBlock::builder();
    body.add_statement("result: dict[str, object] = {}", ());
    for (json_name, prop) in all_fields {
        let field_name = python_field_name(&prop.name);
        let to_expr = render_to_dict_expr(&format!("self.{field_name}"), json_name, ir, properties);
        if prop.required && !prop.nullable {
            body.add_statement(&format!("result[\"{json_name}\"] = {to_expr}"), ());
        } else {
            body.add_statement(&format!("if self.{field_name} is not None:%>"), ());
            body.add_statement(&format!("result[\"{json_name}\"] = {to_expr}%<"), ());
        }
    }
    body.add_statement("return result", ());

    FunSpec::builder("to_dict")
        .add_param(self_param)
        .returns(return_type)
        .body(body.build().expect("to_dict body"))
        .build()
        .expect("to_dict method")
}

fn build_from_dict_method(
    class_name: &str,
    all_fields: &[(&String, &IrProperty)],
    ir: &IrSpec,
    properties: &indexmap::IndexMap<String, IrProperty>,
) -> FunSpec {
    let cls_param = ParameterSpec::of("cls", TypeName::primitive(""));
    let data_param = ParameterSpec::of(
        "data",
        TypeName::generic(
            TypeName::primitive("dict"),
            vec![TypeName::primitive("str"), TypeName::primitive("object")],
        ),
    );

    let mut body = CodeBlock::builder();
    body.add_statement("return cls(%>", ());
    for (json_name, prop) in all_fields {
        let field_name = python_field_name(&prop.name);
        let is_required = prop.required && !prop.nullable;
        let expr = if is_required {
            render_from_dict_expr(json_name, ir, properties)
        } else {
            render_from_dict_optional_expr(json_name, ir, properties)
        };
        if let Some(comment_start) = expr.find("  #") {
            let (value_part, comment_part) = expr.split_at(comment_start);
            body.add_statement(&format!("{field_name}={value_part},{comment_part}"), ());
        } else {
            body.add_statement(&format!("{field_name}={expr},"), ());
        }
    }
    body.add("%<", ());
    body.add_statement(")", ());

    FunSpec::builder("from_dict")
        .annotation(CodeBlock::of("@classmethod", ()).expect("classmethod"))
        .add_param(cls_param)
        .add_param(data_param)
        .returns(TypeName::primitive(class_name))
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
                .value(CodeBlock::of(&value_code, ()).expect("enum value"))
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

fn emit_alias(schema: &IrSchema, expr: &IrTypeExpr) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();
    let rhs_type = python_type_name(expr);

    let type_alias = sigil_quote!(Python {
        type $N(name.as_str()) = ($T(rhs_type));
    })
    .ok()?;

    let mut file =
        FileSpec::builder_with("model.py", Python::new()).header(future_annotations_header());
    if needs_typing_literal(expr) {
        file = file.add_import(ImportSpec::named("typing", "Literal"));
    }
    if let Some(doc) = &schema.description {
        file = file.add_raw(&format!("# {}\n", escape_docstring(doc)));
    }
    file = file.add_code(type_alias);
    file.build().ok()
}

fn emit_type_alias_raw(schema: &IrSchema, rhs: &str) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();

    let type_alias = sigil_quote!(Python {
        type $N(name.as_str()) = $L(rhs);
    })
    .ok()?;

    let mut file =
        FileSpec::builder_with("model.py", Python::new()).header(future_annotations_header());
    if let Some(doc) = &schema.description {
        file = file.add_raw(&format!("# {}\n", escape_docstring(doc)));
    }
    file = file.add_code(type_alias);
    file.build().ok()
}

// ---------------------------------------------------------------------------
// Type alias builder with proper multi-line formatting
// ---------------------------------------------------------------------------

fn format_type_alias(name: &str, members: &[TypeName]) -> CodeBlock {
    let mut cb = CodeBlock::builder();
    if members.is_empty() {
        cb.add(
            "type %N = (%T)",
            (
                NameArg(name.to_string()),
                TypeName::importable("typing", "Any"),
            ),
        );
    } else if members.len() == 1 {
        cb.add(
            "type %N = (%T)",
            (NameArg(name.to_string()), members[0].clone()),
        );
    } else {
        cb.add(
            "type %N = (\n    %T",
            (NameArg(name.to_string()), members[0].clone()),
        );
        for member in &members[1..] {
            cb.add("\n    | %T", (member.clone(),));
        }
        cb.add("\n)", ());
    }
    cb.build_unwrap()
}

// ---------------------------------------------------------------------------
// Union -> type X = A | B | C
// ---------------------------------------------------------------------------

fn emit_union(schema: &IrSchema, u: &IrUnion) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();

    let mut members: Vec<TypeName> = u.members.iter().map(python_type_name).collect();
    if u.nullable {
        members.push(TypeName::primitive("None"));
    }

    let type_alias = format_type_alias(&name, &members);

    let mut file =
        FileSpec::builder_with("model.py", Python::new()).header(future_annotations_header());
    if needs_typing_literal_in_exprs(&u.members) {
        file = file.add_import(ImportSpec::named("typing", "Literal"));
    }
    if let Some(doc) = &schema.description {
        file = file.add_raw(&format!("# {}\n", escape_docstring(doc)));
    }
    file = file.add_code(type_alias);
    file.build().ok()
}

// ---------------------------------------------------------------------------
// Intersection -> merged @dataclass
// ---------------------------------------------------------------------------

fn emit_intersection(schema: &IrSchema, inter: &IrIntersection, ir: &IrSpec) -> Option<FileSpec> {
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

    emit_intersection_as_dataclass(schema, &all_props, ir)
}

fn emit_intersection_as_alias(schema: &IrSchema, inter: &IrIntersection) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();
    let members: Vec<TypeName> = inter.members.iter().map(python_type_name).collect();

    let type_alias = format_type_alias(&name, &members);

    let mut file =
        FileSpec::builder_with("model.py", Python::new()).header(future_annotations_header());
    if needs_typing_literal_in_exprs(&inter.members) {
        file = file.add_import(ImportSpec::named("typing", "Literal"));
    }
    file = file.add_code(type_alias);
    file.build().ok()
}

fn emit_intersection_as_dataclass(
    schema: &IrSchema,
    all_props: &indexmap::IndexMap<String, IrProperty>,
    ir: &IrSpec,
) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();

    let mut file =
        FileSpec::builder_with("model.py", Python::new()).header(future_annotations_header());

    if needs_typing_literal_in_props(all_props) {
        file = file.add_import(ImportSpec::named("typing", "Literal"));
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
        cls = cls.extra_member(CodeBlock::of("pass", ()).expect("pass"));
    } else {
        for (_json_name, prop) in &required {
            let field_name = python_field_name(&prop.name);
            let type_name = python_type_name(&prop.type_expr);
            cls = cls.add_field(
                FieldSpec::builder(&field_name, type_name)
                    .build()
                    .expect("required field"),
            );
        }
        for (_json_name, prop) in &optional {
            let field_name = python_field_name(&prop.name);
            let type_name = python_type_name(&prop.type_expr);
            cls = cls.add_field(
                FieldSpec::builder(&field_name, TypeName::optional(type_name))
                    .initializer(CodeBlock::of("None", ()).expect("None init"))
                    .build()
                    .expect("optional field"),
            );
        }

        let all_fields: Vec<(&String, &IrProperty)> =
            required.iter().chain(optional.iter()).copied().collect();
        cls = cls.add_method(build_to_dict_method(&all_fields, ir, all_props));
        cls = cls.add_method(build_from_dict_method(&name, &all_fields, ir, all_props));
    }

    file = file.add_type(cls.build().ok()?);
    file.build().ok()
}

// ---------------------------------------------------------------------------
// TaggedUnion -> type X = A | B | C
// ---------------------------------------------------------------------------

fn emit_tagged_union(schema: &IrSchema, tu: &IrTaggedUnion, ir: &IrSpec) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();
    let snake_name = schema.name.to_snake_case();

    let members: Vec<TypeName> = tu
        .variants
        .iter()
        .map(|v| python_type_name(&v.content_type))
        .collect();

    let type_alias = format_type_alias(&name, &members);

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
    let mut doc_block = String::new();
    for line in doc.lines() {
        doc_block.push_str(&format!("# {line}\n"));
    }
    file = file.add_raw(&doc_block);
    file = file.add_code(type_alias);

    if !tu.variants.is_empty() {
        let helpers = build_tagged_union_helpers(&name, &snake_name, tu, ir);
        file = file.add_code(helpers);
    }

    file.build().ok()
}

fn build_tagged_union_helpers(
    pascal_name: &str,
    snake_name: &str,
    tu: &IrTaggedUnion,
    ir: &IrSpec,
) -> CodeBlock {
    let tag_field = &tu.discriminator_field;

    let resolved_variants: Vec<(&IrTaggedVariant, String)> = tu
        .variants
        .iter()
        .filter_map(|v| {
            if let IrTypeExpr::Named(ref_name) = &v.content_type
                && is_object_schema(ref_name, ir)
            {
                return Some((v, ref_name.to_pascal_case()));
            }
            None
        })
        .collect();

    let mut cb = CodeBlock::builder();

    if resolved_variants.is_empty() {
        return cb.build_unwrap();
    }

    // from_dict
    cb.add_line();
    cb.begin_control_flow(
        &format!("def {snake_name}_from_dict(data: dict[str, object]) -> {pascal_name}"),
        (),
    );
    match &tu.tagging {
        TaggingStyle::Internal => {
            cb.add_statement(&format!("_tag = data[\"{tag_field}\"]"), ());
            for (i, (variant, py_class)) in resolved_variants.iter().enumerate() {
                let cond = format!("_tag == \"{}\"", variant.discriminator_value);
                emit_elif(&mut cb, i == 0, false, &cond);
                cb.add_statement(&format!("return {py_class}.from_dict(data)"), ());
            }
            cb.end_control_flow_no_newline();
        }
        TaggingStyle::Adjacent { content_field } => {
            cb.add_statement(&format!("_tag = data[\"{tag_field}\"]"), ());
            cb.add_statement(
                &format!("_content = data[\"{content_field}\"]  # type: ignore[assignment]"),
                (),
            );
            for (i, (variant, py_class)) in resolved_variants.iter().enumerate() {
                let cond = format!("_tag == \"{}\"", variant.discriminator_value);
                emit_elif(&mut cb, i == 0, false, &cond);
                cb.add_statement(
                    &format!("return {py_class}.from_dict(_content)  # type: ignore[arg-type]"),
                    (),
                );
            }
            cb.end_control_flow_no_newline();
        }
        TaggingStyle::External => {
            for (i, (variant, py_class)) in resolved_variants.iter().enumerate() {
                let cond = format!("\"{}\" in data", variant.discriminator_value);
                emit_elif(&mut cb, i == 0, false, &cond);
                cb.add_statement(
                    &format!(
                        "return {py_class}.from_dict(data[\"{}\"])  # type: ignore[arg-type]",
                        variant.discriminator_value
                    ),
                    (),
                );
            }
            cb.end_control_flow_no_newline();
        }
    }
    cb.add_statement(
        "raise ValueError(%V)",
        VerbatimStrArg(format!(
            "Unknown discriminator value for {pascal_name}: {{data}}"
        )),
    );
    cb.end_control_flow();

    // to_dict
    cb.add_line();
    cb.begin_control_flow(
        &format!("def {snake_name}_to_dict(obj: {pascal_name}) -> dict[str, object]"),
        (),
    );
    let last_idx = resolved_variants.len() - 1;
    match &tu.tagging {
        TaggingStyle::Internal => {
            for (i, (variant, py_class)) in resolved_variants.iter().enumerate() {
                let cond = format!("isinstance(obj, {py_class})");
                emit_elif(&mut cb, i == 0, i == last_idx, &cond);
                cb.add_statement("result = obj.to_dict()", ());
                cb.add_statement(
                    &format!(
                        "result[\"{tag_field}\"] = \"{}\"",
                        variant.discriminator_value
                    ),
                    (),
                );
                cb.add_statement("return result", ());
            }
            cb.end_control_flow_no_newline();
        }
        TaggingStyle::Adjacent { content_field } => {
            for (i, (variant, py_class)) in resolved_variants.iter().enumerate() {
                let cond = format!("isinstance(obj, {py_class})");
                emit_elif(&mut cb, i == 0, i == last_idx, &cond);
                cb.add_statement(
                    &format!(
                        "return {{\"{tag_field}\": \"{}\", \"{content_field}\": obj.to_dict()}}",
                        variant.discriminator_value
                    ),
                    (),
                );
            }
            cb.end_control_flow_no_newline();
        }
        TaggingStyle::External => {
            for (i, (variant, py_class)) in resolved_variants.iter().enumerate() {
                let cond = format!("isinstance(obj, {py_class})");
                emit_elif(&mut cb, i == 0, i == last_idx, &cond);
                cb.add_statement(
                    &format!(
                        "return {{\"{}\": obj.to_dict()}}",
                        variant.discriminator_value
                    ),
                    (),
                );
            }
            cb.end_control_flow_no_newline();
        }
    }
    cb.add_statement(
        "raise ValueError(%V)",
        VerbatimStrArg(format!("Unknown variant for {pascal_name}: {{type(obj)}}")),
    );
    cb.end_control_flow();

    cb.build_unwrap()
}

fn emit_elif(
    cb: &mut sigil_stitch::code_block::CodeBlockBuilder,
    is_first: bool,
    is_last: bool,
    cond: &str,
) {
    if !is_first {
        cb.end_control_flow_no_newline();
    }
    if is_last && !is_first {
        cb.begin_control_flow("else", ());
    } else {
        let kw = if is_first { "if" } else { "elif" };
        cb.begin_control_flow(&format!("{kw} {cond}"), ());
    }
}

// ---------------------------------------------------------------------------
// Type mapping
// ---------------------------------------------------------------------------

/// Map an IR type expression to a sigil-stitch TypeName with auto-import tracking.
pub fn python_type_name(expr: &IrTypeExpr) -> TypeName {
    match expr {
        IrTypeExpr::Named(name) => {
            let py_name = name.to_pascal_case();
            let module = format!(".{}", name.to_snake_case());
            TypeName::importable(&module, &py_name)
        }
        IrTypeExpr::Primitive(p) => python_primitive_type_name(p),
        IrTypeExpr::StringLiteral(s) => {
            let lit = format!("Literal[\"{}\"]", escape_python_string(s));
            TypeName::raw(&lit)
        }
        IrTypeExpr::StringEnum(values) => {
            let members: Vec<String> = values
                .iter()
                .map(|v| format!("\"{}\"", escape_python_string(v)))
                .collect();
            let lit = format!("Literal[{}]", members.join(", "));
            TypeName::raw(&lit)
        }
        IrTypeExpr::Array(inner) => {
            TypeName::generic(TypeName::primitive("list"), vec![python_type_name(inner)])
        }
        IrTypeExpr::Map(inner) => TypeName::generic(
            TypeName::primitive("dict"),
            vec![TypeName::primitive("str"), python_type_name(inner)],
        ),
        IrTypeExpr::Union(members) => {
            if members.is_empty() {
                TypeName::importable("typing", "Any")
            } else {
                TypeName::union(members.iter().map(python_type_name).collect())
            }
        }
        IrTypeExpr::Nullable(inner) => TypeName::optional(python_type_name(inner)),
        IrTypeExpr::Any => TypeName::importable("typing", "Any"),
    }
}

/// Like `python_type_name` but Named types import from `..models.{snake}` (for API files).
pub fn api_type_name(expr: &IrTypeExpr) -> TypeName {
    match expr {
        IrTypeExpr::Named(name) => {
            let py_name = name.to_pascal_case();
            let module = format!("..models.{}", name.to_snake_case());
            TypeName::importable(&module, &py_name)
        }
        IrTypeExpr::Array(inner) => {
            TypeName::generic(TypeName::primitive("list"), vec![api_type_name(inner)])
        }
        IrTypeExpr::Map(inner) => TypeName::generic(
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

/// Map an IR type expression to a Python type string (for serialization helpers).
pub fn python_type_str(expr: &IrTypeExpr) -> String {
    match expr {
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
    match expr {
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

fn render_to_dict_expr(
    value_expr: &str,
    json_name: &str,
    ir: &IrSpec,
    properties: &indexmap::IndexMap<String, IrProperty>,
) -> String {
    let prop = properties.get(json_name);
    let type_expr = prop.map(|p| &p.type_expr);
    match type_expr {
        Some(IrTypeExpr::Named(ref_name)) => {
            if is_object_schema(ref_name, ir) {
                format!("{value_expr}.to_dict()")
            } else {
                value_expr.to_string()
            }
        }
        Some(IrTypeExpr::Array(inner)) => {
            if let IrTypeExpr::Named(ref_name) = inner.as_ref()
                && is_object_schema(ref_name, ir)
            {
                return format!("[item.to_dict() for item in {value_expr}]");
            }
            value_expr.to_string()
        }
        Some(IrTypeExpr::Nullable(inner)) => {
            if let IrTypeExpr::Named(ref_name) = inner.as_ref()
                && is_object_schema(ref_name, ir)
            {
                return format!("{value_expr}.to_dict() if {value_expr} is not None else None");
            }
            value_expr.to_string()
        }
        Some(IrTypeExpr::Map(inner)) => {
            if let IrTypeExpr::Named(ref_name) = inner.as_ref()
                && is_object_schema(ref_name, ir)
            {
                return format!("{{k: v.to_dict() for k, v in {value_expr}.items()}}");
            }
            value_expr.to_string()
        }
        _ => value_expr.to_string(),
    }
}

fn render_from_dict_expr(
    json_name: &str,
    ir: &IrSpec,
    properties: &indexmap::IndexMap<String, IrProperty>,
) -> String {
    let prop = properties.get(json_name);
    let type_expr = prop.map(|p| &p.type_expr);
    let accessor = format!("data[\"{json_name}\"]");
    match type_expr {
        Some(IrTypeExpr::Named(ref_name)) => {
            if is_object_schema(ref_name, ir) {
                let py_name = ref_name.to_pascal_case();
                format!("{py_name}.from_dict({accessor})  # type: ignore[arg-type]")
            } else {
                format!("{accessor}  # type: ignore[assignment]")
            }
        }
        Some(IrTypeExpr::Array(inner)) => {
            if let IrTypeExpr::Named(ref_name) = inner.as_ref()
                && is_object_schema(ref_name, ir)
            {
                let py_name = ref_name.to_pascal_case();
                return format!(
                    "[{py_name}.from_dict(item) for item in {accessor}]  # type: ignore[union-attr]"
                );
            }
            format!("{accessor}  # type: ignore[assignment]")
        }
        Some(IrTypeExpr::Map(inner)) => {
            if let IrTypeExpr::Named(ref_name) = inner.as_ref()
                && is_object_schema(ref_name, ir)
            {
                let py_name = ref_name.to_pascal_case();
                return format!(
                    "{{k: {py_name}.from_dict(v) for k, v in {accessor}.items()}}  # type: ignore[union-attr]"
                );
            }
            format!("{accessor}  # type: ignore[assignment]")
        }
        _ => format!("{accessor}  # type: ignore[assignment]"),
    }
}

fn render_from_dict_optional_expr(
    json_name: &str,
    ir: &IrSpec,
    properties: &indexmap::IndexMap<String, IrProperty>,
) -> String {
    let prop = properties.get(json_name);
    let type_expr = prop.map(|p| &p.type_expr);
    let raw_type = type_expr.map(|t| match t {
        IrTypeExpr::Nullable(inner) => inner.as_ref(),
        _ => t,
    });
    let accessor = format!("data.get(\"{json_name}\")");
    match raw_type {
        Some(IrTypeExpr::Named(ref_name)) => {
            if is_object_schema(ref_name, ir) {
                let py_name = ref_name.to_pascal_case();
                format!(
                    "{py_name}.from_dict({accessor}) if {accessor} is not None else None  # type: ignore[arg-type]"
                )
            } else {
                format!("{accessor}  # type: ignore[assignment]")
            }
        }
        Some(IrTypeExpr::Array(inner)) => {
            if let IrTypeExpr::Named(ref_name) = inner.as_ref()
                && is_object_schema(ref_name, ir)
            {
                let py_name = ref_name.to_pascal_case();
                return format!(
                    "[{py_name}.from_dict(item) for item in {accessor}] if {accessor} is not None else None  # type: ignore[union-attr]"
                );
            }
            format!("{accessor}  # type: ignore[assignment]")
        }
        Some(IrTypeExpr::Map(inner)) => {
            if let IrTypeExpr::Named(ref_name) = inner.as_ref()
                && is_object_schema(ref_name, ir)
            {
                let py_name = ref_name.to_pascal_case();
                return format!(
                    "{{k: {py_name}.from_dict(v) for k, v in {accessor}.items()}} if {accessor} is not None else None  # type: ignore[union-attr]"
                );
            }
            format!("{accessor}  # type: ignore[assignment]")
        }
        _ => format!("{accessor}  # type: ignore[assignment]"),
    }
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
