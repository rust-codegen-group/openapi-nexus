//! API emission for IR operations (Python API classes).
//!
//! Uses sigil-stitch high-level APIs (TypeSpec, FunSpec, TypeName, FileSpec) for
//! structured code generation with automatic import tracking. Groups operations
//! by tag, emits one `apis/{tag}_api.py` per tag.

use std::collections::{BTreeMap, HashSet};

use crate::codegen::traits::file_writer::FileInfo;
use crate::generators::multipart::{MultipartValueEncoding, multipart_parts_for_request_body};
use crate::generators::request_inputs::{RequestInputPlan, request_input_for_operation};
use crate::ir::types::{
    IrOperation, IrParameter, IrPrimitive, IrRequestBody, IrResponse, IrSpec, IrTypeExpr,
    ParameterLocation,
};
use heck::{ToPascalCase, ToSnakeCase};
use sigil_stitch::code_block::CodeBlock;
use sigil_stitch::lang::python::Python;
use sigil_stitch::prelude::*;

use super::emit_models::{
    api_type_name, future_annotations_header, is_object_schema, python_field_name,
};

/// Generate every API file from the IR.
pub fn generate_api_files(
    ir: &IrSpec,
    header: &str,
    request_inputs: &RequestInputPlan,
) -> Result<Vec<FileInfo>, String> {
    let by_tag = group_by_tag(&ir.operations);
    let mut files = Vec::with_capacity(by_tag.len());
    for (tag, ops) in &by_tag {
        let stem = tag.to_snake_case();
        let filename = format!("{stem}_api.py");
        let body = emit_api_file(tag, ops, ir, header, request_inputs);
        files.push(FileInfo::api(filename, body));
    }
    Ok(files)
}

fn group_by_tag(operations: &[IrOperation]) -> BTreeMap<String, Vec<&IrOperation>> {
    let mut out: BTreeMap<String, Vec<&IrOperation>> = BTreeMap::new();
    for op in operations {
        let tags: Vec<String> = if op.tags.is_empty() {
            vec!["default".to_string()]
        } else {
            op.tags.clone()
        };
        for tag in tags {
            out.entry(tag).or_default().push(op);
        }
    }
    out
}

fn emit_api_file(
    tag: &str,
    ops: &[&IrOperation],
    ir: &IrSpec,
    header: &str,
    request_inputs: &RequestInputPlan,
) -> String {
    let class_name = format!("{}Api", tag.to_pascal_case());
    let plans: Vec<OpPlan> = ops
        .iter()
        .map(|op| plan_operation(op, ir, request_inputs))
        .collect();

    let client_type = TypeName::importable("..runtime.client", "Client");
    let error_type = TypeName::importable("..runtime.errors", "ApiError");

    // __init__ method via FunSpec
    let init_body = CodeBlock::of("self._client = client", ()).expect("static body");
    let init = FunSpec::builder("__init__")
        .add_param(ParameterSpec::of("self", TypeName::primitive("")))
        .add_param(ParameterSpec::of("client", client_type))
        .returns(TypeName::primitive("None"))
        .body(init_body)
        .build()
        .expect("__init__ FunSpec builds");

    let mut cls = TypeSpec::builder(&class_name, TypeKind::Class).add_method(init);

    for plan in &plans {
        cls = cls.add_method(build_api_method(plan, ir, &error_type));
    }

    let mut fb = FileSpec::builder_with(&format!("{}_api.py", tag.to_snake_case()), Python::new())
        .header(future_annotations_header())
        .add_type(cls.build().expect("API TypeSpec builds"));
    if plans.iter().any(|plan| {
        plan.body.as_ref().is_some_and(|body| {
            body.multipart_parts.as_ref().is_some_and(|parts| {
                parts
                    .iter()
                    .any(|part| part.value_encoding == MultipartValueEncoding::Json)
            })
        })
    }) {
        fb = fb.add_import(ImportSpec::side_effect("json"));
    }
    let file = fb.build().expect("API FileSpec builds");

    let body = file.render(120).unwrap_or_default();
    let mut content = String::with_capacity(header.len() + body.len());
    content.push_str(header);
    content.push_str(&body);
    content
}

fn build_api_method(plan: &OpPlan<'_>, ir: &IrSpec, error_type: &TypeName) -> FunSpec {
    let mut fun = FunSpec::builder(&plan.method_name);

    // self (bare, no type annotation)
    fun = fun.add_param(ParameterSpec::of("self", TypeName::primitive("")));

    // Positional params (path params)
    for p in &plan.path_params {
        fun = fun.add_param(ParameterSpec::of(
            &p.var_name,
            api_type_name(&p.param.type_expr),
        ));
    }

    // Keyword-only separator
    let has_keyword_params =
        !plan.query_params.is_empty() || !plan.header_params.is_empty() || plan.body.is_some();
    if has_keyword_params {
        fun = fun.add_param(ParameterSpec::of("*", TypeName::primitive("")));
    }

    // Required query/header params first
    for p in plan.query_params.iter().chain(&plan.header_params) {
        if p.param.required {
            fun = fun.add_param(ParameterSpec::of(
                &p.var_name,
                api_type_name(&p.param.type_expr),
            ));
        }
    }

    // Body param
    if let Some(b) = &plan.body {
        let ty = api_type_name(&b.type_expr);
        if b.required {
            fun = fun.add_param(ParameterSpec::of(&b.var_name, ty));
        } else {
            fun = fun.add_param(
                ParameterSpec::builder(&b.var_name, TypeName::optional(ty))
                    .default_value(CodeBlock::of("None", ()).expect("None"))
                    .build()
                    .expect("optional body param"),
            );
        }
    }

    // Optional query/header params last
    for p in plan.query_params.iter().chain(&plan.header_params) {
        if !p.param.required {
            let param_ty = api_type_name(&p.param.type_expr);
            let param_ty = if is_already_optional(&p.param.type_expr) {
                param_ty
            } else {
                TypeName::optional(param_ty)
            };
            fun = fun.add_param(
                ParameterSpec::builder(&p.var_name, param_ty)
                    .default_value(CodeBlock::of("None", ()).expect("None"))
                    .build()
                    .expect("optional param"),
            );
        }
    }

    // Return type — auto-tracked via TypeName
    let return_type = if plan.typed_responses.is_empty() {
        TypeName::primitive("None")
    } else {
        response_type_name(&plan.typed_responses[0])
    };
    fun = fun.returns(return_type);

    // Docstring
    if let Some(summary) = &plan.op.summary {
        fun = fun.doc(&format!("{summary}."));
    }

    // Method body (imperative control flow, stays as CodeBlock)
    fun = fun.body(build_method_body(plan, ir, error_type));

    fun.build().expect("API method FunSpec builds")
}

fn build_method_body(plan: &OpPlan<'_>, ir: &IrSpec, error_type: &TypeName) -> CodeBlock {
    let mut cb = CodeBlock::builder();

    // Path interpolation
    if plan.path_params.is_empty() {
        cb.add_statement(&format!("path = \"{}\"", plan.op.path), ());
    } else {
        let mut path_template = plan.op.path.clone();
        for p in &plan.path_params {
            let placeholder = format!("{{{}}}", p.param.name);
            let replacement = format!("{{{}}}", p.var_name);
            path_template = path_template.replace(&placeholder, &replacement);
        }
        cb.add_statement("path = %V", VerbatimStrArg(path_template));
    }

    // Query params
    let has_query = !plan.query_params.is_empty();
    if has_query {
        cb.add_statement("params: dict[str, str] = {}", ());
        for p in &plan.query_params {
            let stringify = render_stringify(&p.var_name, &p.param.type_expr);
            if p.param.required {
                cb.add_statement(&format!("params[\"{}\"] = {stringify}", p.param.name), ());
            } else {
                cb.add_statement(&format!("if {} is not None:%>", p.var_name), ());
                cb.add_statement(&format!("params[\"{}\"] = {stringify}%<", p.param.name), ());
            }
        }
    }

    // Header params
    let body_content_type = plan.body.as_ref().and_then(|body| {
        let base = media_type_base(&body.media_type);
        if base != "multipart/form-data" {
            Some(body.media_type.as_str())
        } else {
            None
        }
    });
    let has_headers = !plan.header_params.is_empty() || body_content_type.is_some();
    if has_headers {
        cb.add_statement("headers: dict[str, str] = {}", ());
        if let Some(media_type) = body_content_type {
            cb.add_statement(&format!("headers[\"Content-Type\"] = \"{media_type}\""), ());
        }
        for p in &plan.header_params {
            let stringify = render_stringify(&p.var_name, &p.param.type_expr);
            if p.param.required {
                cb.add_statement(&format!("headers[\"{}\"] = {stringify}", p.param.name), ());
            } else {
                cb.add_statement(&format!("if {} is not None:%>", p.var_name), ());
                cb.add_statement(
                    &format!("headers[\"{}\"] = {stringify}%<", p.param.name),
                    (),
                );
            }
        }
    }

    // Body serialization
    let body_expr = if let Some(b) = &plan.body {
        if is_object_type(&b.type_expr, ir) {
            if b.required {
                format!("{}.to_dict()", b.var_name)
            } else {
                format!(
                    "{}.to_dict() if {} is not None else None",
                    b.var_name, b.var_name
                )
            }
        } else if is_array_of_objects(&b.type_expr, ir) {
            if b.required {
                format!("[item.to_dict() for item in {}]", b.var_name)
            } else {
                format!(
                    "[item.to_dict() for item in {}] if {} is not None else None",
                    b.var_name, b.var_name
                )
            }
        } else {
            b.var_name.clone()
        }
    } else {
        String::new()
    };

    // Request call
    let mut request_args = vec![
        format!("\"{}\"", plan.op.method.to_uppercase()),
        "path".to_string(),
    ];
    if has_query {
        request_args.push("params=params".to_string());
    }
    if let Some(body) = &plan.body {
        if media_type_base(&body.media_type) == "multipart/form-data" {
            if let Some(parts) = &body.multipart_parts {
                emit_multipart_data(&mut cb, body, parts, ir);
                request_args.push("files=files if files else None".to_string());
            } else {
                cb.add_statement(
                    "raise ValueError(\"unsupported multipart request body: schema must be object-shaped\")",
                    (),
                );
            }
        } else {
            match body.encoding {
                BodyEncoding::Json => request_args.push(format!("json={body_expr}")),
                BodyEncoding::FormUrlEncoded
                | BodyEncoding::TextPlain
                | BodyEncoding::OctetStream => request_args.push(format!("data={body_expr}")),
                BodyEncoding::Xml | BodyEncoding::Other => {
                    if body.required {
                        cb.add_statement(
                            &format!(
                                "raise ValueError(\"unsupported request body media type: {}\")",
                                body.media_type
                            ),
                            (),
                        );
                    } else {
                        cb.add_statement(&format!("if {} is not None:%>", body.var_name), ());
                        cb.add_statement(
                            &format!(
                                "raise ValueError(\"unsupported request body media type: {}\")%<",
                                body.media_type
                            ),
                            (),
                        );
                    }
                }
                BodyEncoding::Multipart => unreachable!("multipart handled separately"),
            }
        }
    }
    if has_headers {
        request_args.push("headers=headers".to_string());
    }

    cb.add_code(
        sigil_quote!(Python {
            response = self._client.request($for(arg in &request_args; separator = ", ") { $L(arg.as_str()) })
        })
        .expect("request call"),
    );

    // Error handling
    cb.add_statement("if response.status_code >= 400:%>", ());
    cb.add_statement(
        "raise %T(response.status_code, response.reason, response.content)%<",
        (error_type.clone(),),
    );

    // Response parsing
    if !plan.typed_responses.is_empty() {
        let tr = &plan.typed_responses[0];
        let parse_expr = render_response_parse(tr, ir);
        cb.add_statement(&format!("return {parse_expr}"), ());
    } else {
        cb.add_statement("return None", ());
    }

    cb.build().expect("API method body builds")
}

fn emit_multipart_data(
    cb: &mut sigil_stitch::code_block::CodeBlockBuilder,
    body: &BodyBinding,
    parts: &[MultipartPart],
    ir: &IrSpec,
) {
    cb.add_statement("files: dict[str, object] = {}", ());
    if !body.required {
        cb.add_statement(&format!("if {} is not None:%>", body.var_name), ());
    }
    for part in parts {
        let access = format!("{}.{}", body.var_name, part.field_name);
        if part.required {
            emit_required_multipart_part(cb, part, &access, ir);
        } else {
            cb.add_statement(&format!("if {access} is not None:%>"), ());
            emit_required_multipart_part(cb, part, &access, ir);
            cb.add_statement("%<", ());
        }
    }
    if !body.required {
        cb.add_statement("%<", ());
    }
}

fn emit_required_multipart_part(
    cb: &mut sigil_stitch::code_block::CodeBlockBuilder,
    part: &MultipartPart,
    access: &str,
    ir: &IrSpec,
) {
    cb.add_code(multipart_part_assignment(part, access, ir));
}

fn multipart_part_assignment(part: &MultipartPart, access: &str, ir: &IrSpec) -> CodeBlock {
    let binary_stmt = format!(
        "files[\"{}\"] = ({}.filename_or_default(\"{}\"), {}.data, \"{}\")",
        part.wire_name, access, part.wire_name, access, part.content_type
    );
    let json_value = render_multipart_json_value(access, &part.type_expr, ir);
    let json_stmt = format!(
        "files[\"{}\"] = (None, json.dumps({json_value}), \"{}\")",
        part.wire_name, part.content_type
    );
    let unsupported_stmt = "raise ValueError(\"unsupported multipart part content type\")";
    let scalar_stmt = format!(
        "files[\"{}\"] = (None, str({access}), \"{}\")",
        part.wire_name, part.content_type
    );

    sigil_quote!(Python {
        $if(part.is_binary) {
            $L(binary_stmt.as_str())
        } $else_if(part.value_encoding == MultipartValueEncoding::Json) {
            $L(json_stmt.as_str())
        } $else_if(part.value_encoding == MultipartValueEncoding::Unsupported) {
            $L(unsupported_stmt)
        } $else {
            $L(scalar_stmt.as_str())
        }
    })
    .expect("multipart part assignment builds")
}

fn render_multipart_json_value(access: &str, expr: &IrTypeExpr, ir: &IrSpec) -> String {
    match expr {
        IrTypeExpr::Named(name) if is_object_schema(name, ir) => format!("{access}.to_dict()"),
        IrTypeExpr::Nullable(inner) => render_multipart_json_value(access, inner, ir),
        IrTypeExpr::Array(inner) => {
            if let IrTypeExpr::Named(name) = inner.as_ref()
                && is_object_schema(name, ir)
            {
                format!("[item.to_dict() for item in {access}]")
            } else {
                access.to_string()
            }
        }
        _ => access.to_string(),
    }
}

fn render_stringify(var: &str, type_expr: &IrTypeExpr) -> String {
    match type_expr {
        IrTypeExpr::Primitive(
            IrPrimitive::String
            | IrPrimitive::Date
            | IrPrimitive::DateTime
            | IrPrimitive::Uuid
            | IrPrimitive::StringWithFormat(_),
        )
        | IrTypeExpr::StringLiteral(_)
        | IrTypeExpr::StringEnum(_)
        | IrTypeExpr::Named(_) => format!("str({var})"),
        IrTypeExpr::Primitive(IrPrimitive::Boolean) => format!("str({var}).lower()"),
        IrTypeExpr::Primitive(
            IrPrimitive::Integer
            | IrPrimitive::IntegerWithFormat(_)
            | IrPrimitive::Number
            | IrPrimitive::NumberWithFormat(_),
        ) => format!("str({var})"),
        IrTypeExpr::Nullable(inner) => render_stringify(var, inner),
        IrTypeExpr::Array(_) => format!("\",\".join(str(v) for v in {var})"),
        _ => format!("str({var})"),
    }
}

fn response_type_name(response: &TypedResponse) -> TypeName {
    match response.decoding {
        ResponseDecoding::Json => api_type_name(&response.type_expr),
        ResponseDecoding::Text => TypeName::primitive("str"),
        ResponseDecoding::Bytes => TypeName::primitive("bytes"),
    }
}

fn render_response_parse(response: &TypedResponse, ir: &IrSpec) -> String {
    match response.decoding {
        ResponseDecoding::Json => render_json_response_parse(&response.type_expr, ir),
        ResponseDecoding::Text => "response.text".to_string(),
        ResponseDecoding::Bytes => "response.content".to_string(),
    }
}

fn render_json_response_parse(type_expr: &IrTypeExpr, ir: &IrSpec) -> String {
    match type_expr {
        IrTypeExpr::Named(name) => {
            let py_name = name.to_pascal_case();
            if is_object_schema(name, ir) {
                format!("{py_name}.from_dict(response.json())")
            } else {
                "response.json()  # type: ignore[return-value]".to_string()
            }
        }
        IrTypeExpr::Array(inner) => {
            if let IrTypeExpr::Named(name) = inner.as_ref()
                && is_object_schema(name, ir)
            {
                let py_name = name.to_pascal_case();
                return format!("[{py_name}.from_dict(item) for item in response.json()]");
            }
            "response.json()  # type: ignore[return-value]".to_string()
        }
        IrTypeExpr::Primitive(IrPrimitive::String | IrPrimitive::StringWithFormat(_)) => {
            "response.text".to_string()
        }
        _ => "response.json()  # type: ignore[return-value]".to_string(),
    }
}

fn is_object_type(type_expr: &IrTypeExpr, ir: &IrSpec) -> bool {
    if let IrTypeExpr::Named(name) = type_expr {
        return is_object_schema(name, ir);
    }
    false
}

fn is_array_of_objects(type_expr: &IrTypeExpr, ir: &IrSpec) -> bool {
    if let IrTypeExpr::Array(inner) = type_expr
        && let IrTypeExpr::Named(name) = inner.as_ref()
    {
        return is_object_schema(name, ir);
    }
    false
}

// ---------------------------------------------------------------------------
// Planning
// ---------------------------------------------------------------------------

struct OpPlan<'a> {
    op: &'a IrOperation,
    method_name: String,
    path_params: Vec<ParamBinding<'a>>,
    query_params: Vec<ParamBinding<'a>>,
    header_params: Vec<ParamBinding<'a>>,
    body: Option<BodyBinding>,
    typed_responses: Vec<TypedResponse>,
}

struct ParamBinding<'a> {
    param: &'a IrParameter,
    var_name: String,
}

struct BodyBinding {
    var_name: String,
    type_expr: IrTypeExpr,
    required: bool,
    media_type: String,
    encoding: BodyEncoding,
    multipart_parts: Option<Vec<MultipartPart>>,
}

struct MultipartPart {
    wire_name: String,
    field_name: String,
    type_expr: IrTypeExpr,
    is_binary: bool,
    required: bool,
    content_type: String,
    value_encoding: MultipartValueEncoding,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BodyEncoding {
    Json,
    Multipart,
    FormUrlEncoded,
    Xml,
    TextPlain,
    OctetStream,
    Other,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ResponseDecoding {
    Json,
    Text,
    Bytes,
}

struct TypedResponse {
    type_expr: IrTypeExpr,
    decoding: ResponseDecoding,
}

fn plan_operation<'a>(
    op: &'a IrOperation,
    ir: &IrSpec,
    request_inputs: &RequestInputPlan,
) -> OpPlan<'a> {
    let op_id = sanitize_operation_id(&op.operation_id, &op.method, &op.path);
    let method_name = op_id.to_snake_case();

    let mut used_names: HashSet<String> = HashSet::new();
    used_names.insert("self".to_string());

    let mut path_params = Vec::new();
    let mut query_params = Vec::new();
    let mut header_params = Vec::new();

    for p in &op.parameters {
        let var_name = unique_name(&python_param_name(&p.name), &mut used_names);
        let binding = ParamBinding { param: p, var_name };
        match p.location {
            ParameterLocation::Path => path_params.push(binding),
            ParameterLocation::Query => query_params.push(binding),
            ParameterLocation::Header => header_params.push(binding),
            ParameterLocation::Cookie => header_params.push(binding),
        }
    }

    let body = op
        .request_body
        .as_ref()
        .and_then(|b| plan_body(op, b, ir, request_inputs, &mut used_names));

    let typed_responses = op.responses.iter().filter_map(plan_response).collect();

    OpPlan {
        op,
        method_name,
        path_params,
        query_params,
        header_params,
        body,
        typed_responses,
    }
}

fn plan_body(
    op: &IrOperation,
    b: &IrRequestBody,
    ir: &IrSpec,
    request_inputs: &RequestInputPlan,
    used_names: &mut HashSet<String>,
) -> Option<BodyBinding> {
    let (media_type, t) = pick_body_content(b)?;
    let encoding = body_encoding(&media_type);
    let var_name = unique_name("body", used_names);
    let multipart_parts = if media_type_base(&media_type) == "multipart/form-data" {
        multipart_parts_for(b, &media_type, ir)
    } else {
        None
    };
    Some(BodyBinding {
        var_name,
        type_expr: if encoding == BodyEncoding::Multipart {
            request_input_for_operation(request_inputs, op, &media_type)
                .map(|input| IrTypeExpr::Named(input.name.clone()))
                .unwrap_or(t)
        } else {
            t
        },
        required: b.required,
        media_type,
        encoding,
        multipart_parts,
    })
}

fn plan_response(r: &IrResponse) -> Option<TypedResponse> {
    let (media_type, t) = pick_response_content(r)?;
    Some(TypedResponse {
        type_expr: t,
        decoding: response_decoding(&media_type),
    })
}

fn body_encoding(media_type: &str) -> BodyEncoding {
    let base = media_type_base(media_type);
    if base == "multipart/form-data" {
        BodyEncoding::Multipart
    } else if is_json_media_type(media_type) {
        BodyEncoding::Json
    } else if base == "application/x-www-form-urlencoded" {
        BodyEncoding::FormUrlEncoded
    } else if is_xml_media_type(media_type) {
        BodyEncoding::Xml
    } else if base == "text/plain" {
        BodyEncoding::TextPlain
    } else if base == "application/octet-stream" {
        BodyEncoding::OctetStream
    } else {
        BodyEncoding::Other
    }
}

fn response_decoding(media_type: &str) -> ResponseDecoding {
    let base = media_type_base(media_type);
    if is_json_media_type(media_type) {
        ResponseDecoding::Json
    } else if base == "text/plain" || is_xml_media_type(media_type) {
        ResponseDecoding::Text
    } else {
        ResponseDecoding::Bytes
    }
}

fn pick_body_content(body: &IrRequestBody) -> Option<(String, IrTypeExpr)> {
    pick_media_type(&body.content, |media_type| {
        media_type_base(media_type) == "application/json"
    })
    .or_else(|| pick_media_type(&body.content, is_json_media_type))
    .or_else(|| {
        pick_media_type(&body.content, |media_type| {
            media_type_base(media_type) == "multipart/form-data"
        })
    })
    .or_else(|| {
        pick_media_type(&body.content, |media_type| {
            media_type_base(media_type) == "application/x-www-form-urlencoded"
        })
    })
    .or_else(|| pick_media_type(&body.content, is_xml_media_type))
    .or_else(|| {
        pick_media_type(&body.content, |media_type| {
            media_type_base(media_type) == "text/plain"
        })
    })
    .or_else(|| {
        pick_media_type(&body.content, |media_type| {
            media_type_base(media_type) == "application/octet-stream"
        })
    })
    .or_else(|| pick_first_content(&body.content))
}

fn pick_response_content(r: &IrResponse) -> Option<(String, IrTypeExpr)> {
    pick_media_type(&r.content, |media_type| {
        media_type_base(media_type) == "application/json"
    })
    .or_else(|| pick_media_type(&r.content, is_json_media_type))
    .or_else(|| {
        pick_media_type(&r.content, |media_type| {
            media_type_base(media_type) == "application/octet-stream"
        })
    })
    .or_else(|| {
        pick_media_type(&r.content, |media_type| {
            media_type_base(media_type) == "text/plain"
        })
    })
    .or_else(|| pick_media_type(&r.content, is_xml_media_type))
    .or_else(|| pick_first_content(&r.content))
}

fn pick_media_type(
    content: &indexmap::IndexMap<String, IrTypeExpr>,
    predicate: impl Fn(&str) -> bool,
) -> Option<(String, IrTypeExpr)> {
    content
        .iter()
        .find(|(media_type, _)| predicate(media_type))
        .map(|(media_type, t)| (media_type.clone(), t.clone()))
}

fn pick_first_content(
    content: &indexmap::IndexMap<String, IrTypeExpr>,
) -> Option<(String, IrTypeExpr)> {
    content
        .iter()
        .next()
        .map(|(media_type, t)| (media_type.clone(), t.clone()))
}

fn media_type_base(media_type: &str) -> String {
    media_type
        .split(';')
        .next()
        .unwrap_or(media_type)
        .trim()
        .to_ascii_lowercase()
}

fn is_json_media_type(media_type: &str) -> bool {
    let base = media_type_base(media_type);
    base == "application/json" || base.ends_with("+json")
}

fn is_xml_media_type(media_type: &str) -> bool {
    let base = media_type_base(media_type);
    base == "application/xml" || base == "text/xml" || base.ends_with("+xml")
}

fn multipart_parts_for(
    body: &IrRequestBody,
    media_type: &str,
    ir: &IrSpec,
) -> Option<Vec<MultipartPart>> {
    multipart_parts_for_request_body(body, media_type, ir).map(|parts| {
        parts
            .into_iter()
            .map(|part| MultipartPart {
                field_name: python_field_name(&part.wire_name),
                wire_name: part.wire_name,
                type_expr: part.type_expr,
                is_binary: part.is_binary,
                required: part.required,
                content_type: part.content_type,
                value_encoding: part.value_encoding,
            })
            .collect()
    })
}

fn python_param_name(name: &str) -> String {
    let snake = name.to_snake_case();
    if snake.is_empty() {
        return "param".to_string();
    }
    match snake.as_str() {
        "and" | "as" | "assert" | "async" | "await" | "break" | "class" | "continue" | "def"
        | "del" | "elif" | "else" | "except" | "finally" | "for" | "from" | "global" | "if"
        | "import" | "in" | "is" | "lambda" | "nonlocal" | "not" | "or" | "pass" | "raise"
        | "return" | "try" | "while" | "with" | "yield" | "type" | "self" => {
            format!("{snake}_")
        }
        _ => snake,
    }
}

fn unique_name(desired: &str, used: &mut HashSet<String>) -> String {
    if used.insert(desired.to_string()) {
        return desired.to_string();
    }
    for i in 2..=u32::MAX {
        let candidate = format!("{desired}{i}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!("name collision space exhausted")
}

fn sanitize_operation_id(op_id: &str, method: &str, path: &str) -> String {
    if !op_id.is_empty() {
        return op_id.to_string();
    }
    let path_part: String = path
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    format!("{method}_{path_part}")
}

/// Returns true if the type expression is already nullable (wrapped in None),
/// so that the caller can avoid double-wrapping with TypeName::optional.
fn is_already_optional(expr: &IrTypeExpr) -> bool {
    matches!(expr, IrTypeExpr::Nullable(_))
}
