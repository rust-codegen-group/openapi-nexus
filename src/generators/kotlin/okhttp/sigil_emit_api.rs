use std::collections::{BTreeMap, HashSet};

use crate::codegen::traits::file_writer::FileInfo;
use crate::generators::multipart::{MultipartValueEncoding, multipart_parts_for_request_body};
use crate::generators::request_inputs::{RequestInputPlan, request_input_for_operation};
use crate::generators::response_headers::{
    ResponseHeaderPlan, ResponseHeaderValueKind, collect_response_headers,
    unique_response_header_accessor_names,
};
use crate::generators::response_names::{
    response_entry_name as response_variant_name, response_match_rank,
};
use crate::generators::{ArtifactKind, GeneratorError};
use crate::ir::types::{
    IrOperation, IrParameter, IrRequestBody, IrResponse, IrSpec, IrTypeExpr, ParameterLocation,
};
use heck::{ToLowerCamelCase, ToPascalCase};
use sigil_stitch::error::SigilStitchError;
use sigil_stitch::lang::kotlin::Kotlin;
use sigil_stitch::prelude::*;

use super::util::{
    kt_decl_name, kt_param_name, kt_ref, kt_type_str, render_value_as_string,
    sanitize_operation_id, unique_name,
};

const RENDER_WIDTH: usize = 100;

pub fn generate_api_files(
    ir: &IrSpec,
    package_name: &str,
    header: &str,
    request_inputs: &RequestInputPlan,
) -> Result<Vec<FileInfo>, GeneratorError> {
    let by_tag = group_by_tag(&ir.operations);
    let mut files = Vec::with_capacity(by_tag.len());
    let has_models = has_kotlin_models(ir, request_inputs);
    for (tag, ops) in &by_tag {
        let class_name = format!("{}Api", tag.to_pascal_case());
        let filename = format!("{class_name}.kt");
        let body = emit_api_file(tag, ops, ir, package_name, request_inputs, has_models).map_err(
            |source| GeneratorError::emission(ArtifactKind::Api, format!("{tag} API"), source),
        )?;
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

fn has_kotlin_models(ir: &IrSpec, request_inputs: &RequestInputPlan) -> bool {
    !ir.schemas.is_empty() || !request_inputs.models().is_empty()
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
    has_models: bool,
) -> Result<String, SigilStitchError> {
    let class_name = format!("{}Api", tag.to_pascal_case());
    let plans: Vec<OpPlan> = ops
        .iter()
        .map(|op| plan_operation(op, ir, request_inputs))
        .collect();

    let filename = format!("{class_name}.kt");
    let has_textual_error_response = plans.iter().any(|plan| {
        plan.error_responses.iter().any(|response| {
            matches!(
                response.decoding,
                ResponseDecoding::Json | ResponseDecoding::Text
            )
        })
    });
    let mut fb = FileSpec::builder_with(&filename, Kotlin::new())
        .header(package_header(package_name))
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
    if has_models {
        fb = fb.add_import(ImportSpec::named(&format!("{package_name}.models"), "*"));
    }
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
    if has_textual_error_response {
        fb = fb.add_import(ImportSpec::named(
            "okhttp3.MediaType.Companion",
            "toMediaTypeOrNull",
        ));
    }

    // API class
    let mut cls = TypeSpec::builder(&class_name, TypeKind::Class).visibility(Visibility::Public);
    cls = cls.doc(&format!(
        "{class_name} groups operations under the {tag} tag."
    ));
    cls = cls.add_primary_constructor_param(
        ParameterSpec::builder("client", TypeName::primitive("ApiClient"))
            .is_property()
            .storage_visibility(Visibility::Private)
            .build()
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
        fb = fb.add_code(build_error_types_block(plan));
    }

    // API class with methods
    for plan in &plans {
        cls = cls.add_method(build_operation_fun(plan)?);
    }

    fb = fb.add_type(cls.build().expect("API class builds"));

    let file = fb.build().expect("FileSpec builds for API file");
    file.render(RENDER_WIDTH)
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
        ParameterSpec::builder("statusCode", TypeName::primitive("Int"))
            .is_property()
            .build()
            .expect("param"),
    );

    tb = tb.add_primary_constructor_param(
        ParameterSpec::builder("raw", TypeName::primitive("Response"))
            .is_property()
            .build()
            .expect("param"),
    );

    let mut seen: HashSet<String> = HashSet::new();
    for tr in &plan.typed_responses {
        if !seen.insert(tr.field_name.clone()) {
            continue;
        }
        let nullable_type = format!("{}?", tr.kt_type);
        tb = tb.add_primary_constructor_param(
            ParameterSpec::builder(&tr.field_name, TypeName::primitive(&nullable_type))
                .is_property()
                .default_value(CodeBlock::of("null", ()).expect("nullable default builds"))
                .build()
                .expect("param"),
        );
    }
    let header_method_names =
        unique_response_header_accessor_names(&plan.success_headers, kotlin_header_method_name);
    for (header, method_name) in plan.success_headers.iter().zip(header_method_names) {
        tb = tb.add_method(build_kotlin_header_accessor(
            header,
            &method_name,
            "raw.headers",
        ));
    }

    tb.build().expect("response class builds")
}

fn build_kotlin_header_accessor(
    header: &ResponseHeaderPlan,
    method_name: &str,
    headers_expr: &str,
) -> FunSpec {
    FunSpec::builder(method_name)
        .visibility(Visibility::Public)
        .returns(vec![TypeName::optional(TypeName::primitive(
            kotlin_header_type(header.value_kind),
        ))])
        .body(kotlin_header_accessor_body(header, headers_expr))
        .build()
        .expect("Kotlin header accessor builds")
}

fn build_error_types_block(plan: &OpPlan<'_>) -> CodeBlock {
    let mut cb = CodeBlock::builder();
    let detail_interface = format!("{}Detail", plan.error_type);
    cb.add(
        &format!(
            "sealed interface {detail_interface} {{\n    val statusCode: Int\n    val headers: okhttp3.Headers\n    fun rawBody(): ByteArray\n}}\n\n"
        ),
        (),
    );
    cb.add(
        &format!(
            "class {}(\n    val detail: {},\n    statusCode: Int,\n    status: String,\n    body: String,\n    val headers: okhttp3.Headers,\n    rawBodyBytes: ByteArray,\n) : ApiException(statusCode, status, body) {{\n    private val rawBodyBytes: ByteArray = rawBodyBytes.copyOf()\n    fun rawBody(): ByteArray = this.rawBodyBytes.copyOf()\n",
            plan.error_type, detail_interface,
        ),
        (),
    );
    cb.add("%>", ());
    let method_names =
        unique_response_header_accessor_names(&plan.error_headers, kotlin_header_method_name);
    for (header, method_name) in plan.error_headers.iter().zip(method_names) {
        cb.add_code(
            build_kotlin_header_accessor(header, &method_name, "headers")
                .emit(&Kotlin::new(), DeclarationContext::Member)
                .expect("Kotlin error header accessor emits"),
        );
    }
    cb.add("%<}\n\n", ());
    let mut seen = HashSet::new();
    for response in &plan.error_responses {
        if !seen.insert(response.field_name.clone()) {
            continue;
        }
        let class_name = format!(
            "{}{}",
            plan.method_name.to_pascal_case(),
            response.field_name
        );
        cb.add_code(kotlin_error_detail_class(
            &class_name,
            &detail_interface,
            response,
        ));
    }
    let unexpected = format!("{}Unexpected", plan.method_name.to_pascal_case());
    cb.add(
        &format!(
            "class {unexpected}(\n    override val statusCode: Int,\n    override val headers: okhttp3.Headers,\n    rawBodyBytes: ByteArray,\n) : {detail_interface} {{\n    private val rawBodyBytes: ByteArray = rawBodyBytes.copyOf()\n    override fun rawBody(): ByteArray = this.rawBodyBytes.copyOf()\n    fun body(): ByteArray = rawBody()\n}}\n"
        ),
        (),
    );
    cb.build().expect("Kotlin error types block builds")
}

fn kotlin_header_method_name(wire_name: &str) -> String {
    let mut base = wire_name.to_lower_camel_case();
    if base.is_empty() || base.starts_with(|character: char| character.is_ascii_digit()) {
        base = format!("header{base}");
    }
    format!("{base}Header")
}

fn kotlin_header_type(kind: ResponseHeaderValueKind) -> &'static str {
    match kind {
        ResponseHeaderValueKind::String => "String",
        ResponseHeaderValueKind::Integer => "Long",
        ResponseHeaderValueKind::Number => "Double",
        ResponseHeaderValueKind::Boolean => "Boolean",
    }
}

fn kotlin_header_accessor_body(header: &ResponseHeaderPlan, headers_expr: &str) -> CodeBlock {
    let wire_name = header.wire_name.as_str();
    let integer_pattern = r"[+-]?[0-9]+";
    let number_pattern = r"[+-]?(?:[0-9]+(?:[.][0-9]*)?|[.][0-9]+)(?:[eE][+-]?[0-9]+)?";
    match header.value_kind {
        ResponseHeaderValueKind::String => sigil_quote!(Kotlin {
            return $L(headers_expr)[$S(wire_name)]
        }),
        ResponseHeaderValueKind::Integer => sigil_quote!(Kotlin {
            return $L(headers_expr)[$S(wire_name)]?.takeIf { it.matches(Regex($S(integer_pattern))) }?.toLongOrNull()
        }),
        ResponseHeaderValueKind::Number => sigil_quote!(Kotlin {
            return $L(headers_expr)[$S(wire_name)]?.takeIf { it.matches(Regex($S(number_pattern))) }?.toDoubleOrNull()?.takeIf { it.isFinite() }
        }),
        ResponseHeaderValueKind::Boolean => sigil_quote!(Kotlin {
            return $L(headers_expr)[$S(wire_name)]?.toBooleanStrictOrNull()
        }),
    }
    .expect("Kotlin header accessor body builds")
}

fn kotlin_error_detail_class(
    class_name: &str,
    detail_interface: &str,
    response: &TypedResponse,
) -> CodeBlock {
    let body_method = kotlin_error_body_method(response);
    let text_helper = kotlin_error_text_helper(response);
    let block = format!(
        "class {class_name}(\n    override val statusCode: Int,\n    override val headers: okhttp3.Headers,\n    rawBodyBytes: ByteArray,\n) : {detail_interface} {{\n    private val rawBodyBytes: ByteArray = rawBodyBytes.copyOf()\n    override fun rawBody(): ByteArray = this.rawBodyBytes.copyOf()\n{text_helper}{body_method}\n}}\n\n"
    );
    CodeBlock::of(&block, ()).expect("Kotlin error detail class builds")
}

fn kotlin_error_text_helper(response: &TypedResponse) -> &'static str {
    match response.decoding {
        ResponseDecoding::Json | ResponseDecoding::Text => {
            "    private fun textBody(): String {\n        val charset = headers[\"Content-Type\"]?.toMediaTypeOrNull()?.charset(Charsets.UTF_8) ?: Charsets.UTF_8\n        return rawBodyBytes.toString(charset)\n    }\n"
        }
        ResponseDecoding::Bytes => "",
    }
}

fn kotlin_error_body_method(response: &TypedResponse) -> String {
    match response.decoding {
        ResponseDecoding::Json => format!(
            "    fun body(): {} = Gson().fromJson(textBody().ifEmpty {{ \"null\" }}, object : TypeToken<{}>() {{}}.type)",
            response.kt_type, response.kt_type
        ),
        ResponseDecoding::Text => "    fun body(): String = textBody()".to_string(),
        ResponseDecoding::Bytes => "    fun body(): ByteArray = rawBody()".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Operation method
// ---------------------------------------------------------------------------

fn build_operation_fun(plan: &OpPlan<'_>) -> Result<FunSpec, SigilStitchError> {
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
        fb = fb
            .add_param(ParameterSpec::new(&p.var_name, TypeName::raw(&p.kt_type)).expect("param"));
    }
    if let Some(body) = &plan.body {
        fb = fb.add_param(
            ParameterSpec::new(&body.var_name, TypeName::raw(&body.kt_type)).expect("body param"),
        );
    }

    fb = fb.returns(vec![TypeName::primitive(&plan.response_type)]);
    fb = fb.body(emit_method_body(plan)?);

    fb.build()
}

// ---------------------------------------------------------------------------
// Method body
// ---------------------------------------------------------------------------

fn emit_method_body(plan: &OpPlan<'_>) -> Result<CodeBlock, SigilStitchError> {
    let mut cb = CodeBlock::builder();

    // Path
    let path_replacements: Vec<(String, String)> = plan
        .path_params
        .iter()
        .map(|p| {
            let var_ref = kt_ref(&p.var_name);
            (
                format!("{{{}}}", p.param.name),
                render_value_as_string(&var_ref, &p.param.type_expr),
            )
        })
        .collect();
    cb.add_code(sigil_quote!(Kotlin {
        val path = $S(plan.op.path)$for((placeholder, stringified) in &path_replacements) {
            .replace($S(placeholder.as_str()), $L(stringified.as_str()))
        }
    })?);

    // Query
    let has_query = !plan.query_params.is_empty();
    if has_query {
        cb.add_code(sigil_quote!(Kotlin {
            val query = mutableMapOf<String, String>()
        })?);
        for p in &plan.query_params {
            let var_ref = kt_ref(&p.var_name);
            let stringified = render_value_as_string(&var_ref, &p.param.type_expr);
            cb.add_code(kotlin_query_param_put(
                p.param.required,
                &var_ref,
                &p.param.name,
                &stringified,
            ));
        }
    }

    // Build request
    let method = plan.op.method.to_uppercase();
    if let Some(body) = &plan.body {
        if let Some(message) = unsupported_request_body_message(body) {
            cb.add_code(sigil_quote!(Kotlin {
                throw IllegalArgumentException($S(message))
            })?);
            return cb.build();
        }
        if body.encoding == BodyEncoding::Multipart {
            if let Some(parts) = &body.multipart_parts {
                emit_multipart_body(&mut cb, body, parts);
                cb.add_code(kotlin_new_request_with_body(
                    &method,
                    has_query,
                    "multipartBody",
                )?);
            } else {
                cb.add_code(
                    sigil_quote!(Kotlin {
                        val request: okhttp3.Request = throw IllegalArgumentException("unsupported multipart request body: schema must be object-shaped")
                    })
                    ?,
                );
            }
        } else {
            emit_request_body(&mut cb, body);
            cb.add_code(kotlin_new_request_with_body(
                &method,
                has_query,
                "requestBody",
            )?);
        }
    } else {
        cb.add_code(kotlin_new_request(&method, has_query)?);
    }

    // Headers
    if !plan.header_params.is_empty() {
        cb.add_code(sigil_quote!(Kotlin { var finalRequest = request })?);
        for p in &plan.header_params {
            let var_ref = kt_ref(&p.var_name);
            let stringified = render_value_as_string(&var_ref, &p.param.type_expr);
            cb.add_code(kotlin_header_param_set(
                p.param.required,
                &var_ref,
                &p.param.name,
                &stringified,
            ));
        }
    }

    // Execute
    cb.add_code(kotlin_execute_request(plan.header_params.is_empty()));
    cb.add_line();
    cb.add_code(kotlin_error_throw(plan));

    // Response parsing
    if !plan.typed_responses.is_empty() {
        cb.add_line();
        cb.add_code(sigil_quote!(Kotlin {
            val responseBytes = response.body?.bytes() ?: ByteArray(0)
        })?);
        cb.add_code(sigil_quote!(Kotlin {
            val responseText = responseBytes.toString(Charsets.UTF_8)
        })?);
        let mut seen: HashSet<String> = HashSet::new();
        let mut emitted_response_variable = false;
        for tr in &plan.typed_responses {
            if !seen.insert(tr.field_name.clone()) {
                continue;
            }
            if emitted_response_variable {
                cb.add_line();
            }
            let response_type = TypeName::raw(&tr.kt_type);
            cb.add_code(sigil_quote!(Kotlin {
                var $N(tr.field_name.as_str()): $T(response_type)? = null
            })?);
            emitted_response_variable = true;
        }
        cb.add_line();
        cb.add_code(kotlin_response_decode_assignments(&plan.typed_responses)?);

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
        cb.add_code(
            sigil_quote!(Kotlin {
                return $N(plan.response_type.as_str())($for(field in &dedup_fields; separator = ", ") { $L(field.as_str()) })
            })
            ?,
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

    cb.build()
}

fn kotlin_new_request(method: &str, has_query: bool) -> Result<CodeBlock, SigilStitchError> {
    sigil_quote!(Kotlin {
        $if(has_query) {
            val request = client.newRequest($S(method), path, query, null)
        } $else {
            val request = client.newRequest($S(method), path, null, null)
        }
    })
}

fn unsupported_request_body_message(body: &BodyBinding) -> Option<String> {
    if body.encoding == BodyEncoding::Multipart && body.multipart_parts.is_none() {
        return Some(
            "unsupported multipart request body: schema must be object-shaped".to_string(),
        );
    }
    match body.encoding {
        BodyEncoding::FormUrlEncoded | BodyEncoding::Xml | BodyEncoding::Other => Some(format!(
            "unsupported request body media type: {}",
            body.media_type
        )),
        _ => None,
    }
}

fn kotlin_new_request_with_body(
    method: &str,
    has_query: bool,
    body_expr: &str,
) -> Result<CodeBlock, SigilStitchError> {
    sigil_quote!(Kotlin {
        $if(has_query) {
            val request = client.newRequestWithBody($S(method), path, query, $L(body_expr))
        } $else {
            val request = client.newRequestWithBody($S(method), path, null, $L(body_expr))
        }
    })
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
        cb.begin_control_flow(&format!("if ({} != null)", kt_ref(&body.var_name)), ());
    }
    cb.add_code(
        sigil_quote!(Kotlin {
            val multipartBuilder = MultipartBody.Builder().setType(MultipartBody.FORM)
        })
        .expect("multipart builder builds"),
    );
    for part in parts {
        let access = format!("{}.{}", kt_ref(&body.var_name), kt_ref(&part.field_name));
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
        cb.begin_control_flow(&format!("if ({} != null)", kt_ref(&body.var_name)), ());
    }
    match body.encoding {
        BodyEncoding::Json => {
            let body_ref = kt_ref(&body.var_name);
            let media_type = body.media_type.as_str();
            cb.add_code(kotlin_json_request_body(
                body.required,
                &body_ref,
                media_type,
            ));
        }
        BodyEncoding::TextPlain | BodyEncoding::OctetStream => {
            let body_ref = kt_ref(&body.var_name);
            let media_type = body.media_type.as_str();
            cb.add_code(kotlin_raw_request_body(
                body.required,
                &body_ref,
                media_type,
            ));
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

fn kotlin_response_decode_assignments(
    typed_responses: &[TypedResponse],
) -> Result<CodeBlock, SigilStitchError> {
    let mut cb = CodeBlock::builder();
    let mut seen: HashSet<String> = HashSet::new();
    let mut emitted_any = false;
    for tr in typed_responses {
        if !seen.insert(tr.field_name.clone()) {
            continue;
        }
        let guard = response_status_guard(&tr.status);
        let field_name = tr.field_name.as_str();
        let deserialize_expr = response_decode_expr(tr);
        let branch = if emitted_any {
            sigil_quote!(Kotlin {
                else if ($L(guard.as_str())) {
                    $N(field_name) = $L(deserialize_expr.as_str())
                }
            })
        } else {
            sigil_quote!(Kotlin {
                if ($L(guard.as_str())) {
                    $N(field_name) = $L(deserialize_expr.as_str())
                }
            })
        }?;
        cb.add_code(branch);
        emitted_any = true;
    }
    cb.build()
}

fn kotlin_error_throw(plan: &OpPlan<'_>) -> CodeBlock {
    let mut cb = CodeBlock::builder();
    cb.begin_control_flow("if (!response.isSuccessful)", ());
    cb.add("val responseBody = response.body", ());
    cb.add_line();
    cb.add(
        "val responseCharset = responseBody?.contentType()?.charset(Charsets.UTF_8) ?: Charsets.UTF_8",
        (),
    );
    cb.add_line();
    cb.add(
        "val responseBytes = responseBody?.bytes() ?: ByteArray(0)",
        (),
    );
    cb.add_line();
    cb.add(
        "val responseText = responseBytes.toString(responseCharset)",
        (),
    );
    cb.add_line();
    cb.add(
        &format!(
            "val detail: {}Detail = when (response.code) ",
            plan.error_type
        ),
        (),
    );
    cb.begin_control_flow("", ());
    let mut seen = HashSet::new();
    for response in plan
        .error_responses
        .iter()
        .filter(|response| response.status.parse::<u16>().is_ok())
    {
        if !seen.insert(response.field_name.clone()) {
            continue;
        }
        cb.add(&format!("{} -> ", response.status), ());
        cb.add_code(kotlin_error_detail_expr(plan, response));
    }
    cb.add("else -> ", ());
    cb.begin_control_flow("", ());
    let mut emitted_wildcard = false;
    for response in plan
        .error_responses
        .iter()
        .filter(|response| response.status.ends_with("XX"))
    {
        let keyword = if emitted_wildcard { "else if" } else { "if" };
        cb.add(
            &format!("{keyword} ({}) ", wildcard_status_guard(&response.status)),
            (),
        );
        cb.begin_control_flow("", ());
        cb.add_code(kotlin_error_detail_expr(plan, response));
        cb.end_control_flow();
        emitted_wildcard = true;
    }
    if let Some(default) = plan
        .error_responses
        .iter()
        .find(|response| response.status.eq_ignore_ascii_case("default"))
    {
        if emitted_wildcard {
            cb.add("else ", ());
            cb.begin_control_flow("", ());
        }
        cb.add_code(kotlin_error_detail_expr(plan, default));
        if emitted_wildcard {
            cb.end_control_flow();
        }
    } else {
        let unexpected = format!("{}Unexpected", plan.method_name.to_pascal_case());
        if emitted_wildcard {
            cb.add("else ", ());
            cb.begin_control_flow("", ());
        }
        cb.add(&format!(
            "{unexpected}(statusCode = response.code, headers = response.headers, rawBodyBytes = responseBytes)\n"
        ), ());
        if emitted_wildcard {
            cb.end_control_flow();
        }
    }
    cb.end_control_flow();
    cb.end_control_flow();
    cb.add(&format!(
        "throw {}(detail = detail, statusCode = response.code, status = response.message, body = responseText, headers = response.headers, rawBodyBytes = responseBytes)",
        plan.error_type
    ), ());
    cb.add_line();
    cb.end_control_flow();
    cb.build().expect("Kotlin error throw builds")
}

fn kotlin_error_detail_expr(plan: &OpPlan<'_>, response: &TypedResponse) -> CodeBlock {
    let class_name = format!(
        "{}{}",
        plan.method_name.to_pascal_case(),
        response.field_name
    );
    let expr = format!(
        "{class_name}(statusCode = response.code, headers = response.headers, rawBodyBytes = responseBytes)\n"
    );
    CodeBlock::of(&expr, ()).expect("Kotlin error detail expr builds")
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
    error_type: String,
    path_params: Vec<ParamBinding<'a>>,
    query_params: Vec<ParamBinding<'a>>,
    header_params: Vec<ParamBinding<'a>>,
    body: Option<BodyBinding>,
    typed_responses: Vec<TypedResponse>,
    error_responses: Vec<TypedResponse>,
    success_headers: Vec<ResponseHeaderPlan>,
    error_headers: Vec<ResponseHeaderPlan>,
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
    let error_type = format!("{}Exception", op_id.to_pascal_case());

    let mut used_names: HashSet<String> = HashSet::new();

    let mut path_params = Vec::new();
    let mut query_params = Vec::new();
    let mut header_params = Vec::new();
    for p in &op.parameters {
        let var_name = unique_name(&kt_param_name(&p.name), &mut used_names);
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

    let mut typed_responses: Vec<TypedResponse> = op
        .responses
        .iter()
        .filter(|r| is_success_status(&r.status))
        .filter_map(plan_response)
        .collect();
    typed_responses.sort_by_key(|r| response_match_rank(&r.status));
    let error_responses = op
        .responses
        .iter()
        .filter(|r| !is_success_status(&r.status))
        .map(plan_error_response)
        .collect();
    let success_headers = collect_response_headers(
        op.responses
            .iter()
            .filter(|response| is_success_status(&response.status)),
        ir,
    );
    let error_headers = collect_response_headers(
        op.responses
            .iter()
            .filter(|response| !is_success_status(&response.status)),
        ir,
    );

    OpPlan {
        op,
        method_name,
        response_type,
        error_type,
        path_params,
        query_params,
        header_params,
        body,
        typed_responses,
        error_responses,
        success_headers,
        error_headers,
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

fn plan_error_response(r: &IrResponse) -> TypedResponse {
    match pick_response_content(r) {
        Some((media_type, t)) => {
            let decoding = response_decoding(&media_type);
            let kt_type = match decoding {
                ResponseDecoding::Json => kt_type_str(&t),
                ResponseDecoding::Text => "String".to_string(),
                ResponseDecoding::Bytes => "ByteArray".to_string(),
            };
            TypedResponse {
                status: r.status.clone(),
                field_name: response_variant_name(&r.status),
                kt_type,
                decoding,
            }
        }
        None => TypedResponse {
            status: r.status.clone(),
            field_name: response_variant_name(&r.status),
            kt_type: "ByteArray".to_string(),
            decoding: ResponseDecoding::Bytes,
        },
    }
}

fn is_success_status(status: &str) -> bool {
    status
        .parse::<u16>()
        .is_ok_and(|code| (200..300).contains(&code))
        || status.eq_ignore_ascii_case("2XX")
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
    } else if upper == "2XX" {
        "response.code in 200..299".to_string()
    } else {
        // "default" or unknown wildcard: match everything (fallback response)
        "true".to_string()
    }
}

fn response_status_guard(status: &str) -> String {
    status
        .parse::<u16>()
        .map(|code| format!("response.code == {code}"))
        .unwrap_or_else(|_| wildcard_status_guard(status))
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
                field_name: kt_decl_name(&part.wire_name),
                wire_name: part.wire_name,
                is_binary: part.is_binary,
                required: part.required,
                content_type: part.content_type,
                value_encoding: part.value_encoding,
            })
            .collect()
    })
}
