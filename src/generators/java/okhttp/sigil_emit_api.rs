use std::collections::{BTreeMap, HashSet};

use crate::codegen::traits::file_writer::FileInfo;
use crate::generators::multipart::{MultipartValueEncoding, multipart_parts_for_request_body};
use crate::ir::types::{
    IrOperation, IrParameter, IrRequestBody, IrResponse, IrSpec, IrTypeExpr, ParameterLocation,
};
use heck::{ToLowerCamelCase, ToPascalCase};
use sigil_stitch::lang::java::Java;
use sigil_stitch::prelude::*;

use super::util::{
    build_java_getter, java_boxed_type_str, java_field_name, java_ident, java_type_str,
    render_value_as_string, sanitize_operation_id, unique_name,
};

const RENDER_WIDTH: usize = 100;

pub fn generate_api_files(
    ir: &IrSpec,
    package_name: &str,
    header: &str,
) -> Result<Vec<FileInfo>, String> {
    let by_tag = group_by_tag(&ir.operations);
    let mut files = Vec::with_capacity(by_tag.len());
    for (tag, ops) in &by_tag {
        let class_name = format!("{}Api", tag.to_pascal_case());
        let filename = format!("{class_name}.java");
        let body = emit_api_file(tag, ops, ir, package_name);
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

fn emit_api_file(tag: &str, ops: &[&IrOperation], ir: &IrSpec, package_name: &str) -> String {
    let class_name = format!("{}Api", tag.to_pascal_case());
    let plans: Vec<OpPlan> = ops.iter().map(|op| plan_operation(op, ir)).collect();

    let filename = format!("{class_name}.java");
    let mut fb = FileSpec::builder_with(&filename, Java::new())
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
        .add_import(ImportSpec::named("java.io", "IOException"))
        .add_import(ImportSpec::named("java.nio.charset", "StandardCharsets"))
        .add_import(ImportSpec::named("java.util", "HashMap"))
        .add_import(ImportSpec::named("java.util", "List"))
        .add_import(ImportSpec::named("java.util", "Map"))
        .add_import(ImportSpec::named("java.util.stream", "Collectors"))
        .add_import(ImportSpec::named("okhttp3", "Request"))
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
    if has_raw_request_body {
        fb = fb.add_import(ImportSpec::named("okhttp3", "RequestBody"));
    }
    if has_supported_multipart_body || has_raw_request_body {
        fb = fb.add_import(ImportSpec::named("okhttp3", "MediaType"));
    }

    // Response classes
    for plan in &plans {
        fb = fb.add_type(build_response_class(plan));
    }

    // API class
    let mut cls = TypeSpec::builder(&class_name, TypeKind::Class).visibility(Visibility::Public);
    cls = cls.doc(&format!(
        "{class_name} groups operations under the {tag} tag."
    ));

    // Fields
    cls = cls.add_field(
        FieldSpec::builder("client", TypeName::primitive("ApiClient"))
            .visibility(Visibility::Private)
            .is_readonly()
            .build()
            .expect("client field"),
    );
    cls = cls.add_field(
        FieldSpec::builder("gson", TypeName::primitive("Gson"))
            .visibility(Visibility::Private)
            .is_readonly()
            .initializer(CodeBlock::of("new Gson()", ()).expect("gson init"))
            .build()
            .expect("gson field"),
    );

    // Constructor
    let mut ctor = FunSpec::builder(&class_name);
    ctor = ctor.visibility(Visibility::Public);
    ctor = ctor.add_param(
        ParameterSpec::new("ApiClient client", TypeName::primitive("")).expect("client param"),
    );
    let ctor_body = sigil_quote!(Java {
        this.client = client;
    })
    .expect("ctor body");
    ctor = ctor.body(ctor_body);
    cls = cls.add_method(ctor.build().expect("constructor"));

    // API methods
    for plan in &plans {
        cls = cls.add_method(build_operation_fun(plan));
    }

    fb = fb.add_type(cls.build().expect("API class builds"));

    let file = fb.build().expect("FileSpec builds for API file");
    file.render(RENDER_WIDTH)
        .expect("FileSpec renders for API file")
}

fn package_header(package_name: &str) -> CodeBlock {
    sigil_quote!(Java {
        package $L(format!("{package_name}.apis"));
    })
    .expect("package header builds")
}

// ---------------------------------------------------------------------------
// Response class
// ---------------------------------------------------------------------------

fn build_response_class(plan: &OpPlan<'_>) -> TypeSpec {
    let mut tb =
        TypeSpec::builder(&plan.response_type, TypeKind::Struct).visibility(Visibility::Public);
    tb = tb.doc(&format!(
        "{} carries the response from {}.",
        plan.response_type, plan.method_name
    ));

    // Fields
    tb = tb.add_field(
        FieldSpec::builder("statusCode", TypeName::primitive("int"))
            .visibility(Visibility::Private)
            .is_readonly()
            .build()
            .expect("field"),
    );
    tb = tb.add_field(
        FieldSpec::builder("raw", TypeName::primitive("Response"))
            .visibility(Visibility::Private)
            .is_readonly()
            .build()
            .expect("field"),
    );

    let mut seen: HashSet<String> = HashSet::new();
    for tr in &plan.typed_responses {
        if !seen.insert(tr.field_name.clone()) {
            continue;
        }
        tb = tb.add_field(
            FieldSpec::builder(&tr.field_name, TypeName::primitive(&tr.java_type))
                .visibility(Visibility::Private)
                .is_readonly()
                .build()
                .expect("field"),
        );
    }

    // Constructor
    let mut ctor = FunSpec::builder(&plan.response_type);
    ctor = ctor.visibility(Visibility::Public);
    ctor = ctor
        .add_param(ParameterSpec::new("int statusCode", TypeName::primitive("")).expect("param"));
    ctor =
        ctor.add_param(ParameterSpec::new("Response raw", TypeName::primitive("")).expect("param"));
    let mut ctor_seen: HashSet<String> = HashSet::new();
    for tr in &plan.typed_responses {
        if !ctor_seen.insert(tr.field_name.clone()) {
            continue;
        }
        ctor = ctor.add_param(
            ParameterSpec::new(
                &format!("{} {}", tr.java_type, tr.field_name),
                TypeName::primitive(""),
            )
            .expect("param"),
        );
    }
    let mut field_assignments: Vec<CodeBlock> = vec![
        sigil_quote!(Java { this.statusCode = statusCode; }).expect("assign"),
        sigil_quote!(Java { this.raw = raw; }).expect("assign"),
    ];
    let mut body_seen: HashSet<String> = HashSet::new();
    for tr in &plan.typed_responses {
        if !body_seen.insert(tr.field_name.clone()) {
            continue;
        }
        field_assignments.push(
            sigil_quote!(Java {
                this.$L(tr.field_name.as_str()) = $L(tr.field_name.as_str());
            })
            .expect("assign"),
        );
    }
    let ctor_body = sigil_quote!(Java {
        $C_each(field_assignments);
    })
    .expect("ctor body");
    ctor = ctor.body(ctor_body);
    tb = tb.add_method(ctor.build().expect("response ctor"));

    // Getters
    tb = tb.add_method(build_java_getter("getStatusCode", "int", "statusCode"));
    tb = tb.add_method(build_java_getter("getRaw", "Response", "raw"));

    let mut getter_seen: HashSet<String> = HashSet::new();
    for tr in &plan.typed_responses {
        if !getter_seen.insert(tr.field_name.clone()) {
            continue;
        }
        let getter_name = format!("get{}", tr.field_name.to_pascal_case());
        tb = tb.add_method(build_java_getter(
            &getter_name,
            &tr.java_type,
            &tr.field_name,
        ));
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
                &format!("{} {}", p.java_type, p.var_name),
                TypeName::primitive(""),
            )
            .expect("param"),
        );
    }
    if let Some(body) = &plan.body {
        fb = fb.add_param(
            ParameterSpec::new(
                &format!("{} {}", body.java_type, body.var_name),
                TypeName::primitive(""),
            )
            .expect("body param"),
        );
    }

    fb = fb.returns(TypeName::primitive(&plan.response_type));
    fb = fb.suffix("throws IOException");
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
    cb.add_statement(&format!("String path = {path_expr}"), ());

    // Query
    let has_query = !plan.query_params.is_empty();
    if has_query {
        cb.add_statement("Map<String, String> query = new HashMap<>()", ());
        for p in &plan.query_params {
            let stringified = render_value_as_string(&p.var_name, &p.param.type_expr);
            if p.param.required {
                cb.add_statement(
                    &format!("query.put(\"{}\", {})", p.param.name, stringified),
                    (),
                );
            } else {
                cb.begin_control_flow(&format!("if ({} != null)", p.var_name), ());
                cb.add_statement(
                    &format!("query.put(\"{}\", {})", p.param.name, stringified),
                    (),
                );
                cb.end_control_flow();
            }
        }
    }

    // Build request
    let query_arg = if has_query { "query" } else { "null" };
    let method = plan.op.method.to_uppercase();
    if let Some(body) = &plan.body {
        cb.add_statement("Request request", ());
        if body.encoding == BodyEncoding::Multipart {
            if let Some(parts) = &body.multipart_parts {
                emit_multipart_body(&mut cb, body, parts);
                cb.add_statement(
                    &format!(
                        "request = client.newRequestWithBody(\"{method}\", path, {query_arg}, multipartBody)"
                    ),
                    (),
                );
            } else {
                cb.add_statement(
                    "throw new IllegalArgumentException(\"unsupported multipart request body: schema must be object-shaped\")",
                    (),
                );
            }
        } else {
            emit_request_body(&mut cb, body);
            cb.add_statement(
                &format!(
                    "request = client.newRequestWithBody(\"{method}\", path, {query_arg}, requestBody)"
                ),
                (),
            );
        }
    } else {
        cb.add_statement(
            &format!("Request request = client.newRequest(\"{method}\", path, {query_arg}, null)"),
            (),
        );
    }

    // Headers
    for p in &plan.header_params {
        let stringified = render_value_as_string(&p.var_name, &p.param.type_expr);
        if p.param.required {
            cb.add_statement(
                &format!(
                    "request = request.newBuilder().header(\"{}\", {stringified}).build()",
                    p.param.name
                ),
                (),
            );
        } else {
            cb.begin_control_flow(&format!("if ({} != null)", p.var_name), ());
            cb.add_statement(
                &format!(
                    "request = request.newBuilder().header(\"{}\", {stringified}).build()",
                    p.param.name
                ),
                (),
            );
            cb.end_control_flow();
        }
    }

    // Execute
    cb.add_statement("Response response = client.execute(request)", ());
    cb.add_line();

    // Error handling
    let error_block = sigil_quote!(Java {
        if (!response.isSuccessful()) {
            String errorBody = response.body() != null ? response.body().string() : "";
            throw new ApiException(response.code(), response.message(), errorBody);
        }
    })
    .expect("error block");
    cb.add_code(error_block);

    // Response parsing
    if !plan.typed_responses.is_empty() {
        cb.add_statement(
            "byte[] responseBytes = response.body() != null ? response.body().bytes() : new byte[0]",
            (),
        );
        cb.add_statement(
            "String responseText = new String(responseBytes, StandardCharsets.UTF_8)",
            (),
        );
        let mut seen: HashSet<String> = HashSet::new();

        // Numeric status codes
        for tr in &plan.typed_responses {
            if !seen.insert(tr.field_name.clone()) {
                continue;
            }
            cb.add_statement(&format!("{} {} = null", tr.java_type, tr.field_name), ());
            if let Ok(code) = tr.status.parse::<u16>() {
                cb.begin_control_flow(&format!("if (response.code() == {code})"), ());
                cb.add_statement(
                    &format!("{} = {}", tr.field_name, response_decode_expr(tr)),
                    (),
                );
                cb.end_control_flow();
            } else {
                // Wildcard status ("4XX", "5XX", "default"): guard by range
                let guard = wildcard_status_guard_java(&tr.status);
                cb.begin_control_flow(&format!("if ({guard})"), ());
                cb.add_statement(
                    &format!("{} = {}", tr.field_name, response_decode_expr(tr)),
                    (),
                );
                cb.end_control_flow();
            }
        }

        // Return with typed fields
        let args: Vec<String> = std::iter::once("response.code()".to_string())
            .chain(std::iter::once("response".to_string()))
            .chain(plan.typed_responses.iter().map(|tr| tr.field_name.clone()))
            .collect();
        // deduplicate
        let mut dedup_args: Vec<String> = Vec::new();
        let mut args_seen: HashSet<String> = HashSet::new();
        for a in args {
            if args_seen.insert(a.clone()) {
                dedup_args.push(a);
            }
        }
        cb.add_statement(
            &format!(
                "return new {}({})",
                plan.response_type,
                dedup_args.join(", ")
            ),
            (),
        );
    } else {
        cb.add_statement(
            &format!(
                "return new {}(response.code(), response)",
                plan.response_type
            ),
            (),
        );
    }

    cb.build().expect("method body builds")
}

fn emit_multipart_body(
    cb: &mut sigil_stitch::code_block::CodeBlockBuilder,
    body: &BodyBinding,
    parts: &[MultipartPart],
) {
    if !body.required {
        cb.add_statement(
            "RequestBody multipartBody = RequestBody.create(new byte[0], null)",
            (),
        );
        cb.begin_control_flow(&format!("if ({} != null)", body.var_name), ());
    }
    cb.add_statement("MultipartBody.Builder multipartBuilder = new MultipartBody.Builder().setType(MultipartBody.FORM)", ());
    for part in parts {
        let access = format!(
            "{}.get{}()",
            body.var_name,
            part.field_name.to_pascal_case()
        );
        if part.required {
            emit_required_multipart_part(cb, part, &access);
        } else {
            cb.begin_control_flow(&format!("if ({access} != null)"), ());
            emit_required_multipart_part(cb, part, &access);
            cb.end_control_flow();
        }
    }
    if body.required {
        cb.add_statement("RequestBody multipartBody = multipartBuilder.build()", ());
    } else {
        cb.add_statement("multipartBody = multipartBuilder.build()", ());
        cb.end_control_flow();
    }
}

fn emit_request_body(cb: &mut sigil_stitch::code_block::CodeBlockBuilder, body: &BodyBinding) {
    if !body.required {
        cb.add_statement(
            "RequestBody requestBody = RequestBody.create(new byte[0], null)",
            (),
        );
        cb.begin_control_flow(&format!("if ({} != null)", body.var_name), ());
    }
    match body.encoding {
        BodyEncoding::Json => {
            cb.add_statement(
                &format!("String jsonBody = gson.toJson({})", body.var_name),
                (),
            );
            let prefix = if body.required {
                "RequestBody requestBody ="
            } else {
                "requestBody ="
            };
            cb.add_statement(
                &format!(
                    "{prefix} RequestBody.create(jsonBody, MediaType.get(\"{}\"))",
                    body.media_type
                ),
                (),
            );
        }
        BodyEncoding::TextPlain | BodyEncoding::OctetStream => {
            let prefix = if body.required {
                "RequestBody requestBody ="
            } else {
                "requestBody ="
            };
            cb.add_statement(
                &format!(
                    "{prefix} RequestBody.create({}, MediaType.get(\"{}\"))",
                    body.var_name, body.media_type
                ),
                (),
            );
        }
        BodyEncoding::FormUrlEncoded | BodyEncoding::Xml | BodyEncoding::Other => {
            cb.add_statement(
                &format!(
                    "throw new IllegalArgumentException(\"unsupported request body media type: {}\")",
                    body.media_type
                ),
                (),
            );
        }
        BodyEncoding::Multipart => unreachable!("multipart handled separately"),
    }
    if !body.required {
        cb.end_control_flow();
    }
}

fn response_decode_expr(tr: &TypedResponse) -> String {
    match tr.decoding {
        ResponseDecoding::Json => {
            let type_token = format!("new TypeToken<{}>() {{}}.getType()", tr.java_type);
            format!("gson.fromJson(responseText.isEmpty() ? \"null\" : responseText, {type_token})")
        }
        ResponseDecoding::Text => "responseText".to_string(),
        ResponseDecoding::Bytes => "responseBytes".to_string(),
    }
}

fn emit_required_multipart_part(
    cb: &mut sigil_stitch::code_block::CodeBlockBuilder,
    part: &MultipartPart,
    access: &str,
) {
    let wire_name = part.wire_name.as_str();
    let content_type = part.content_type.as_str();
    if part.is_binary {
        cb.add_code(
            sigil_quote!(Java {
                multipartBuilder.addFormDataPart($S(wire_name), $S(wire_name), RequestBody.create($L(access), MediaType.get($S(content_type))));
            })
            .expect("binary multipart part block builds"),
        );
    } else if part.value_encoding == MultipartValueEncoding::Json {
        cb.add_code(
            sigil_quote!(Java {
                multipartBuilder.addFormDataPart($S(wire_name), null, RequestBody.create(gson.toJson($L(access)), MediaType.get($S(content_type))));
            })
            .expect("json multipart part block builds"),
        );
    } else if part.value_encoding == MultipartValueEncoding::Unsupported {
        cb.add_code(
            sigil_quote!(Java {
                throw new IllegalArgumentException($S("unsupported multipart part content type"));
            })
            .expect("unsupported multipart part block builds"),
        );
    } else {
        cb.add_code(
            sigil_quote!(Java {
                multipartBuilder.addFormDataPart($S(wire_name), null, RequestBody.create(String.valueOf($L(access)), MediaType.get($S(content_type))));
            })
            .expect("text multipart part block builds"),
        );
    }
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
    java_type: String,
}

struct BodyBinding {
    var_name: String,
    java_type: String,
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
    java_type: String,
    decoding: ResponseDecoding,
}

fn plan_operation<'a>(op: &'a IrOperation, ir: &IrSpec) -> OpPlan<'a> {
    let op_id = sanitize_operation_id(&op.operation_id, &op.method, &op.path);
    let method_name = op_id.to_lower_camel_case();
    let response_type = format!("{}Response", op_id.to_pascal_case());

    let mut used_names: HashSet<String> = HashSet::new();

    let mut path_params = Vec::new();
    let mut query_params = Vec::new();
    let mut header_params = Vec::new();
    for p in &op.parameters {
        let var_name = unique_name(&java_ident(&p.name), &mut used_names);
        let java_type = if p.required {
            java_type_str(&p.type_expr)
        } else {
            java_boxed_type_str(&p.type_expr)
        };
        let binding = ParamBinding {
            param: p,
            var_name,
            java_type,
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
        .and_then(|b| plan_body(b, ir, &mut used_names));

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
    b: &IrRequestBody,
    ir: &IrSpec,
    used_names: &mut HashSet<String>,
) -> Option<BodyBinding> {
    let (media_type, t) = pick_body_content(b)?;
    let encoding = body_encoding(&media_type);
    let java_type = match encoding {
        BodyEncoding::TextPlain => "String".to_string(),
        BodyEncoding::OctetStream => "byte[]".to_string(),
        _ => java_type_str(&t),
    };
    let var_name = unique_name("body", used_names);
    let multipart_parts = if media_type_base(&media_type) == "multipart/form-data" {
        multipart_parts_for(b, &media_type, ir)
    } else {
        None
    };
    Some(BodyBinding {
        var_name,
        java_type,
        media_type,
        required: b.required,
        encoding,
        multipart_parts,
    })
}

fn plan_response(r: &IrResponse) -> Option<TypedResponse> {
    let (media_type, t) = pick_response_content(r)?;
    let decoding = response_decoding(&media_type);
    let java_type = match decoding {
        ResponseDecoding::Json => java_type_str(&t),
        ResponseDecoding::Text => "String".to_string(),
        ResponseDecoding::Bytes => "byte[]".to_string(),
    };
    Some(TypedResponse {
        status: r.status.clone(),
        field_name: response_field_name(&r.status),
        java_type,
        decoding,
    })
}

fn response_field_name(status: &str) -> String {
    if status == "default" {
        "default_".to_string()
    } else if let Ok(code) = status.parse::<u16>() {
        format!("status{code}")
    } else {
        format!("status{}", status.to_lowercase())
    }
}

fn wildcard_status_guard_java(status: &str) -> String {
    let upper = status.to_uppercase();
    if upper == "4XX" {
        "response.code() >= 400 && response.code() < 500".to_string()
    } else if upper == "5XX" {
        "response.code() >= 500 && response.code() < 600".to_string()
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
                field_name: java_field_name(&part.wire_name),
                wire_name: part.wire_name,
                is_binary: part.is_binary,
                required: part.required,
                content_type: part.content_type,
                value_encoding: part.value_encoding,
            })
            .collect()
    })
}
