use std::collections::{BTreeMap, HashSet};

use crate::codegen::traits::file_writer::FileInfo;
use crate::generators::multipart::{MultipartValueEncoding, multipart_parts_for_request_body};
use crate::generators::request_inputs::{RequestInputPlan, request_input_for_operation};
use crate::ir::types::{
    IrOperation, IrParameter, IrRequestBody, IrResponse, IrSpec, IrTypeExpr, ParameterLocation,
};
use heck::{ToLowerCamelCase, ToPascalCase};
use sigil_stitch::lang::kotlin::Kotlin;
use sigil_stitch::prelude::*;

use super::util::{
    kt_field_name, kt_ident, kt_type_str, render_value_as_string, sanitize_operation_id,
    unique_name,
};

const RENDER_WIDTH: usize = 100;

pub fn generate_api_files(
    ir: &IrSpec,
    package_name: &str,
    header: &str,
    request_inputs: &RequestInputPlan,
) -> Result<Vec<FileInfo>, String> {
    let by_tag = group_by_tag(&ir.operations);
    let mut files = Vec::with_capacity(by_tag.len());
    for (tag, ops) in &by_tag {
        let class_name = format!("{}Api", tag.to_pascal_case());
        let filename = format!("{class_name}.kt");
        let body = emit_api_file(tag, ops, ir, package_name, request_inputs);
        let content = format!("{header}{body}");
        files.push(FileInfo::api(filename, content));
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

// ---------------------------------------------------------------------------
// File assembly
// ---------------------------------------------------------------------------

fn emit_api_file(
    tag: &str,
    ops: &[&IrOperation],
    ir: &IrSpec,
    package_name: &str,
    request_inputs: &RequestInputPlan,
) -> String {
    let class_name = format!("{}Api", tag.to_pascal_case());
    let plans: Vec<OpPlan> = ops
        .iter()
        .map(|op| plan_operation(op, ir, request_inputs))
        .collect();

    let filename = format!("{class_name}.kt");
    let mut fb = FileSpec::builder_with(&filename, Kotlin::new())
        .header(package_header(package_name))
        .add_import(ImportSpec::named(&format!("{package_name}.models"), "*"))
        .add_import(ImportSpec::named(
            &format!("{package_name}.runtime"),
            "ApiClient",
        ))
        .add_import(ImportSpec::named(
            &format!("{package_name}.runtime"),
            "ApiException",
        ))
        .add_import(ImportSpec::named("com.google.gson", "Gson"))
        .add_import(ImportSpec::named("com.google.gson.reflect", "TypeToken"))
        .add_import(ImportSpec::named("okhttp3", "Response"));
    let has_supported_multipart_body = plans.iter().any(|plan| {
        plan.body.as_ref().is_some_and(|body| {
            media_type_base(&body.media_type) == "multipart/form-data"
                && body.multipart_parts.is_some()
        })
    });
    let has_raw_request_body = plans.iter().any(|plan| plan.body.is_some());
    if has_supported_multipart_body {
        fb = fb.add_import(ImportSpec::named("okhttp3", "MultipartBody"));
    }
    if has_supported_multipart_body || has_raw_request_body {
        fb = fb
            .add_import(ImportSpec::named(
                "okhttp3.MediaType.Companion",
                "toMediaType",
            ))
            .add_import(ImportSpec::named(
                "okhttp3.RequestBody.Companion",
                "toRequestBody",
            ));
    }

    // API class
    let mut cls = TypeSpec::builder(&class_name, TypeKind::Class).visibility(Visibility::Public);
    cls = cls.doc(&format!(
        "{class_name} groups operations under the {tag} tag."
    ));
    cls = cls.add_primary_constructor_param(
        ParameterSpec::new("private val client: ApiClient", TypeName::primitive(""))
            .expect("client param"),
    );

    // Gson instance
    cls = cls.add_field(
        FieldSpec::builder("gson", TypeName::primitive("Gson"))
            .visibility(Visibility::Private)
            .is_readonly()
            .initializer(CodeBlock::of("Gson()", ()).expect("gson init"))
            .build()
            .expect("gson field"),
    );

    // Response data classes + methods
    for plan in &plans {
        fb = fb.add_type(build_response_class(plan));
    }

    // API class with methods
    for plan in &plans {
        cls = cls.add_method(build_operation_fun(plan));
    }

    fb = fb.add_type(cls.build().expect("API class builds"));

    let file = fb.build().expect("FileSpec builds for API file");
    file.render(RENDER_WIDTH)
        .expect("FileSpec renders for API file")
}

fn package_header(package_name: &str) -> CodeBlock {
    sigil_quote!(Kotlin {
        package $L(format!("{package_name}.apis"))
    })
    .expect("package header builds")
}

// ---------------------------------------------------------------------------
// Response data class
// ---------------------------------------------------------------------------

fn build_response_class(plan: &OpPlan<'_>) -> TypeSpec {
    let mut tb =
        TypeSpec::builder(&plan.response_type, TypeKind::Struct).visibility(Visibility::Public);
    tb = tb.doc(&format!(
        "{} carries the response from {}.",
        plan.response_type, plan.method_name
    ));

    tb = tb.add_primary_constructor_param(
        ParameterSpec::new("val statusCode: Int", TypeName::primitive("")).expect("param"),
    );

    tb = tb.add_primary_constructor_param(
        ParameterSpec::new("val raw: Response", TypeName::primitive("")).expect("param"),
    );

    let mut seen: HashSet<String> = HashSet::new();
    for tr in &plan.typed_responses {
        if !seen.insert(tr.field_name.clone()) {
            continue;
        }
        let nullable_type = format!("{}?", tr.kt_type);
        tb = tb.add_primary_constructor_param(
            ParameterSpec::new(
                &format!("val {}: {} = null", tr.field_name, nullable_type),
                TypeName::primitive(""),
            )
            .expect("param"),
        );
    }

    tb.build().expect("response class builds")
}

// ---------------------------------------------------------------------------
// Operation method
// ---------------------------------------------------------------------------

fn build_operation_fun(plan: &OpPlan<'_>) -> FunSpec {
    let mut fb = FunSpec::builder(&plan.method_name);
    fb = fb.visibility(Visibility::Public);

    if let Some(summary) = &plan.op.summary {
        fb = fb.doc(summary);
    } else {
        fb = fb.doc(&format!(
            "{} {} {}.",
            plan.method_name,
            plan.op.method.to_uppercase(),
            plan.op.path,
        ));
    }

    // Parameters
    for p in plan
        .path_params
        .iter()
        .chain(&plan.query_params)
        .chain(&plan.header_params)
    {
        fb = fb.add_param(
            ParameterSpec::new(
                &format!("{}: {}", p.var_name, p.kt_type),
                TypeName::primitive(""),
            )
            .expect("param"),
        );
    }
    if let Some(body) = &plan.body {
        fb = fb.add_param(
            ParameterSpec::new(
                &format!("{}: {}", body.var_name, body.kt_type),
                TypeName::primitive(""),
            )
            .expect("body param"),
        );
    }

    fb = fb.returns(TypeName::primitive(&plan.response_type));
    fb = fb.body(emit_method_body(plan));

    fb.build().expect("operation FunSpec builds")
}

// ---------------------------------------------------------------------------
// Method body
// ---------------------------------------------------------------------------

fn emit_method_body(plan: &OpPlan<'_>) -> CodeBlock {
    let mut cb = CodeBlock::builder();

    // Path
    let mut path_expr = format!("\"{}\"", plan.op.path);
    for p in &plan.path_params {
        let placeholder = format!("{{{}}}", p.param.name);
        let stringified = render_value_as_string(&p.var_name, &p.param.type_expr);
        path_expr = format!("{path_expr}.replace(\"{placeholder}\", {stringified})");
    }
    cb.add(&format!("val path = {path_expr}"), ());
    cb.add_line();

    // Query
    let has_query = !plan.query_params.is_empty();
    if has_query {
        cb.add("val query = mutableMapOf<String, String>()", ());
        cb.add_line();
        for p in &plan.query_params {
            let stringified = render_value_as_string(&p.var_name, &p.param.type_expr);
            cb.add_code(kotlin_query_param_put(
                p.param.required,
                &p.var_name,
                &p.param.name,
                &stringified,
            ));
        }
    }

    // Build request
    let method = plan.op.method.to_uppercase();
    if let Some(body) = &plan.body {
        if body.encoding == BodyEncoding::Multipart {
            if let Some(parts) = &body.multipart_parts {
                emit_multipart_body(&mut cb, body, parts);
                cb.add_code(kotlin_new_request_with_body(
                    &method,
                    has_query,
                    "multipartBody",
                ));
            } else {
                cb.add(
                    "val request: okhttp3.Request = throw IllegalArgumentException(\"unsupported multipart request body: schema must be object-shaped\")",
                    (),
                );
                cb.add_line();
            }
        } else {
            emit_request_body(&mut cb, body);
            cb.add_code(kotlin_new_request_with_body(
                &method,
                has_query,
                "requestBody",
            ));
        }
    } else {
        cb.add_code(kotlin_new_request(&method, has_query));
    }

    // Headers
    if !plan.header_params.is_empty() {
        cb.add("var finalRequest = request", ());
        cb.add_line();
        for p in &plan.header_params {
            let stringified = render_value_as_string(&p.var_name, &p.param.type_expr);
            cb.add_code(kotlin_header_param_set(
                p.param.required,
                &p.var_name,
                &p.param.name,
                &stringified,
            ));
        }
    }

    // Execute
    cb.add_code(kotlin_execute_request(plan.header_params.is_empty()));
    cb.add_line();

    // Error handling
    let error_block = sigil_quote!(Kotlin {
        if (!response.isSuccessful) {
            val errorBody = response.body?.string() ?: ""
            throw ApiException(response.code, response.message, errorBody)
        }
    })
    .expect("error block");
    cb.add_code(error_block);

    // Response parsing
    if !plan.typed_responses.is_empty() {
        cb.add(
            "val responseBytes = response.body?.bytes() ?: ByteArray(0)",
            (),
        );
        cb.add_line();
        cb.add(
            "val responseText = responseBytes.toString(Charsets.UTF_8)",
            (),
        );
        cb.add_line();
        let mut seen: HashSet<String> = HashSet::new();
        for tr in &plan.typed_responses {
            if !seen.insert(tr.field_name.clone()) {
                continue;
            }
            cb.add_line();
            cb.add_code(kotlin_response_decode_assignment(tr));
        }

        // Return with typed fields
        let fields: Vec<String> = std::iter::once("statusCode = response.code".to_string())
            .chain(std::iter::once("raw = response".to_string()))
            .chain(
                plan.typed_responses
                    .iter()
                    .map(|tr| format!("{} = {}", tr.field_name, tr.field_name)),
            )
            .collect();
        // deduplicate field assignments
        let mut dedup_fields: Vec<String> = Vec::new();
        let mut field_names_seen: HashSet<String> = HashSet::new();
        for f in fields {
            let key = f.split(" = ").next().unwrap_or("").to_string();
            if field_names_seen.insert(key) {
                dedup_fields.push(f);
            }
        }
        cb.add(
            &format!("return {}({})", plan.response_type, dedup_fields.join(", ")),
            (),
        );
    } else {
        cb.add(
            &format!(
                "return {}(statusCode = response.code, raw = response)",
                plan.response_type
            ),
            (),
        );
    }

    cb.build().expect("method body builds")
}

fn kotlin_new_request(method: &str, has_query: bool) -> CodeBlock {
    let with_query = format!("val request = client.newRequest(\"{method}\", path, query, null)");
    let without_query = format!("val request = client.newRequest(\"{method}\", path, null, null)");
    sigil_quote!(Kotlin {
        $if(has_query) {
            $L(with_query.as_str())
        } $else {
            $L(without_query.as_str())
        }
    })
    .expect("Kotlin request construction builds")
}

fn kotlin_new_request_with_body(method: &str, has_query: bool, body_expr: &str) -> CodeBlock {
    let with_query =
        format!("val request = client.newRequestWithBody(\"{method}\", path, query, {body_expr})");
    let without_query =
        format!("val request = client.newRequestWithBody(\"{method}\", path, null, {body_expr})");
    sigil_quote!(Kotlin {
        $if(has_query) {
            $L(with_query.as_str())
        } $else {
            $L(without_query.as_str())
        }
    })
    .expect("Kotlin request body construction builds")
}

fn kotlin_query_param_put(
    required: bool,
    var_name: &str,
    param_name: &str,
    value_expr: &str,
) -> CodeBlock {
    sigil_quote!(Kotlin {
        $if(required) {
            query[$S(param_name)] = $L(value_expr)
        } $else {
            if ($L(var_name) != null) {
                query[$S(param_name)] = $L(value_expr)
            }
        }
    })
    .expect("Kotlin query param put builds")
}

fn kotlin_header_param_set(
    required: bool,
    var_name: &str,
    param_name: &str,
    value_expr: &str,
) -> CodeBlock {
    sigil_quote!(Kotlin {
        $if(required) {
            finalRequest = finalRequest.newBuilder().header($S(param_name), $L(value_expr)).build()
        } $else {
            if ($L(var_name) != null) {
                finalRequest = finalRequest.newBuilder().header($S(param_name), $L(value_expr)).build()
            }
        }
    })
    .expect("Kotlin header param set builds")
}

fn emit_multipart_body(
    cb: &mut sigil_stitch::code_block::CodeBlockBuilder,
    body: &BodyBinding,
    parts: &[MultipartPart],
) {
    if !body.required {
        cb.add_code(
            sigil_quote!(Kotlin {
                var multipartBody = ByteArray(0).toRequestBody(null)
            })
            .expect("default multipart body builds"),
        );
        cb.begin_control_flow(&format!("if ({} != null)", body.var_name), ());
    }
    cb.add_code(
        sigil_quote!(Kotlin {
            val multipartBuilder = MultipartBody.Builder().setType(MultipartBody.FORM)
        })
        .expect("multipart builder builds"),
    );
    for part in parts {
        let access = format!("{}.{}", body.var_name, part.field_name);
        if part.required {
            emit_required_multipart_part(cb, part, &access);
        } else {
            cb.begin_control_flow(&format!("if ({access} != null)"), ());
            emit_required_multipart_part(cb, part, &access);
            cb.end_control_flow();
        }
    }
    cb.add_code(kotlin_multipart_body_finish(body.required));
    if !body.required {
        cb.end_control_flow();
    }
}

fn kotlin_execute_request(use_request: bool) -> CodeBlock {
    sigil_quote!(Kotlin {
        $if(use_request) {
            val response = client.execute(request)
        } $else {
            val response = client.execute(finalRequest)
        }
    })
    .expect("execute request builds")
}

fn kotlin_multipart_body_finish(body_required: bool) -> CodeBlock {
    sigil_quote!(Kotlin {
        $if(body_required) {
            val multipartBody = multipartBuilder.build()
        } $else {
            multipartBody = multipartBuilder.build()
        }
    })
    .expect("multipart body finish builds")
}

fn emit_request_body(cb: &mut sigil_stitch::code_block::CodeBlockBuilder, body: &BodyBinding) {
    if !body.required {
        cb.add_code(
            sigil_quote!(Kotlin {
                var requestBody = ByteArray(0).toRequestBody(null)
            })
            .expect("default request body builds"),
        );
        cb.begin_control_flow(&format!("if ({} != null)", body.var_name), ());
    }
    match body.encoding {
        BodyEncoding::Json => {
            let body_var = body.var_name.as_str();
            let media_type = body.media_type.as_str();
            cb.add_code(kotlin_json_request_body(
                body.required,
                body_var,
                media_type,
            ));
        }
        BodyEncoding::TextPlain | BodyEncoding::OctetStream => {
            let body_var = body.var_name.as_str();
            let media_type = body.media_type.as_str();
            cb.add_code(kotlin_raw_request_body(body.required, body_var, media_type));
        }
        BodyEncoding::FormUrlEncoded | BodyEncoding::Xml | BodyEncoding::Other => {
            let message = format!("unsupported request body media type: {}", body.media_type);
            cb.add_code(
                sigil_quote!(Kotlin {
                    throw IllegalArgumentException($S(message))
                })
                .expect("unsupported request body builds"),
            );
        }
        BodyEncoding::Multipart => unreachable!("multipart handled separately"),
    }
    if !body.required {
        cb.end_control_flow();
    }
}

fn kotlin_json_request_body(body_required: bool, body_var: &str, media_type: &str) -> CodeBlock {
    sigil_quote!(Kotlin {
        val jsonBody = gson.toJson($L(body_var))
        $if(body_required) {
            val requestBody = jsonBody.toRequestBody($S(media_type).toMediaType())
        } $else {
            requestBody = jsonBody.toRequestBody($S(media_type).toMediaType())
        }
    })
    .expect("json request body builds")
}

fn kotlin_raw_request_body(body_required: bool, body_var: &str, media_type: &str) -> CodeBlock {
    sigil_quote!(Kotlin {
        $if(body_required) {
            val requestBody = $L(body_var).toRequestBody($S(media_type).toMediaType())
        } $else {
            requestBody = $L(body_var).toRequestBody($S(media_type).toMediaType())
        }
    })
    .expect("raw request body builds")
}

fn response_decode_expr(tr: &TypedResponse) -> String {
    match tr.decoding {
        ResponseDecoding::Json => format!(
            "gson.fromJson<{}>(responseText.ifEmpty {{ \"null\" }}, object : TypeToken<{}>() {{}}.type)",
            tr.kt_type, tr.kt_type
        ),
        ResponseDecoding::Text => "responseText".to_string(),
        ResponseDecoding::Bytes => "responseBytes".to_string(),
    }
}

fn kotlin_response_decode_assignment(tr: &TypedResponse) -> CodeBlock {
    let exact_status = tr.status.parse::<u16>().ok();
    let has_exact_status = exact_status.is_some();
    let status_code = exact_status.unwrap_or_default().to_string();
    let guard = wildcard_status_guard(&tr.status);
    let field_name = tr.field_name.as_str();
    let deserialize_expr = response_decode_expr(tr);
    sigil_quote!(Kotlin {
        $if(has_exact_status) {
            val $L(field_name) = if (response.code == $L(status_code.as_str())) $L(deserialize_expr.as_str()) else null
        } $else {
            val $L(field_name) = if ($L(guard.as_str())) $L(deserialize_expr.as_str()) else null
        }
    })
    .expect("Kotlin response decode assignment builds")
}

fn emit_required_multipart_part(
    cb: &mut sigil_stitch::code_block::CodeBlockBuilder,
    part: &MultipartPart,
    access: &str,
) {
    cb.add_code(kotlin_multipart_part(part, access));
    cb.add_line();
}

fn kotlin_multipart_part(part: &MultipartPart, access: &str) -> CodeBlock {
    let wire_name = part.wire_name.as_str();
    let content_type = part.content_type.as_str();
    sigil_quote!(Kotlin {
        $if(part.is_binary) {
            multipartBuilder.addFormDataPart($S(wire_name), $L(access).filenameOrDefault($S(wire_name)), $L(access).data.toRequestBody($S(content_type).toMediaType()))
        } $else_if(part.value_encoding == MultipartValueEncoding::Json) {
            multipartBuilder.addFormDataPart($S(wire_name), null, gson.toJson($L(access)).toRequestBody($S(content_type).toMediaType()))
        } $else_if(part.value_encoding == MultipartValueEncoding::Unsupported) {
            throw IllegalArgumentException($S("unsupported multipart part content type"))
        } $else {
            multipartBuilder.addFormDataPart($S(wire_name), null, $L(access).toString().toRequestBody($S(content_type).toMediaType()))
        }
    })
    .expect("multipart part block builds")
}

// ---------------------------------------------------------------------------
// Planning
// ---------------------------------------------------------------------------

struct OpPlan<'a> {
    op: &'a IrOperation,
    method_name: String,
    response_type: String,
    path_params: Vec<ParamBinding<'a>>,
    query_params: Vec<ParamBinding<'a>>,
    header_params: Vec<ParamBinding<'a>>,
    body: Option<BodyBinding>,
    typed_responses: Vec<TypedResponse>,
}

struct ParamBinding<'a> {
    param: &'a IrParameter,
    var_name: String,
    kt_type: String,
}

struct BodyBinding {
    var_name: String,
    kt_type: String,
    media_type: String,
    required: bool,
    encoding: BodyEncoding,
    multipart_parts: Option<Vec<MultipartPart>>,
}

struct MultipartPart {
    wire_name: String,
    field_name: String,
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
    status: String,
    field_name: String,
    kt_type: String,
    decoding: ResponseDecoding,
}

fn plan_operation<'a>(
    op: &'a IrOperation,
    ir: &IrSpec,
    request_inputs: &RequestInputPlan,
) -> OpPlan<'a> {
    let op_id = sanitize_operation_id(&op.operation_id, &op.method, &op.path);
    let method_name = op_id.to_lower_camel_case();
    let response_type = format!("{}Response", op_id.to_pascal_case());

    let mut used_names: HashSet<String> = HashSet::new();

    let mut path_params = Vec::new();
    let mut query_params = Vec::new();
    let mut header_params = Vec::new();
    for p in &op.parameters {
        let var_name = unique_name(&kt_ident(&p.name), &mut used_names);
        let kt_type = if p.required {
            kt_type_str(&p.type_expr)
        } else {
            format!("{}?", kt_type_str(&p.type_expr))
        };
        let binding = ParamBinding {
            param: p,
            var_name,
            kt_type,
        };
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
        response_type,
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
    let mut kt_type = match encoding {
        BodyEncoding::TextPlain => "String".to_string(),
        BodyEncoding::OctetStream => "ByteArray".to_string(),
        BodyEncoding::Multipart => request_input_for_operation(request_inputs, op, &media_type)
            .map(|input| input.name.to_pascal_case())
            .unwrap_or_else(|| kt_type_str(&t)),
        _ => kt_type_str(&t),
    };
    if !b.required {
        kt_type = format!("{kt_type}?");
    }
    let var_name = unique_name("body", used_names);
    let multipart_parts = if media_type_base(&media_type) == "multipart/form-data" {
        multipart_parts_for(b, &media_type, ir)
    } else {
        None
    };
    Some(BodyBinding {
        var_name,
        kt_type,
        media_type,
        required: b.required,
        encoding,
        multipart_parts,
    })
}

fn plan_response(r: &IrResponse) -> Option<TypedResponse> {
    let (media_type, t) = pick_response_content(r)?;
    let decoding = response_decoding(&media_type);
    let kt_type = match decoding {
        ResponseDecoding::Json => kt_type_str(&t),
        ResponseDecoding::Text => "String".to_string(),
        ResponseDecoding::Bytes => "ByteArray".to_string(),
    };
    Some(TypedResponse {
        status: r.status.clone(),
        field_name: response_field_name(&r.status),
        kt_type,
        decoding,
    })
}

fn response_field_name(status: &str) -> String {
    if status == "default" {
        "default".to_string()
    } else if let Ok(code) = status.parse::<u16>() {
        format!("status{code}")
    } else {
        format!("status{}", status.to_lowercase())
    }
}

fn wildcard_status_guard(status: &str) -> String {
    let upper = status.to_uppercase();
    if upper == "4XX" {
        "response.code in 400..499".to_string()
    } else if upper == "5XX" {
        "response.code in 500..599".to_string()
    } else {
        // "default" or unknown wildcard: match everything (fallback response)
        "true".to_string()
    }
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
                field_name: kt_field_name(&part.wire_name),
                wire_name: part.wire_name,
                is_binary: part.is_binary,
                required: part.required,
                content_type: part.content_type,
                value_encoding: part.value_encoding,
            })
            .collect()
    })
}
