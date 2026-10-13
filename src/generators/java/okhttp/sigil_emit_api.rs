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
    request_inputs: &RequestInputPlan,
) -> Result<Vec<FileInfo>, String> {
    let by_tag = group_by_tag(&ir.operations);
    let mut files = Vec::with_capacity(by_tag.len());
    let mut support_files_seen = HashSet::new();
    let has_models = has_java_models(ir, request_inputs);
    for (tag, ops) in &by_tag {
        let class_name = format!("{}Api", tag.to_pascal_case());
        let filename = format!("{class_name}.java");
        let body = emit_api_file(tag, ops, ir, package_name, request_inputs, has_models);
        let content = format!("{header}{body}");
        files.push(FileInfo::api(filename, content));
        for op in ops {
            let plan = plan_operation(op, ir, request_inputs);
            for file in operation_support_files(&plan, package_name, header, has_models) {
                if support_files_seen.insert(file.filename.clone()) {
                    files.push(file);
                }
            }
        }
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

fn has_java_models(ir: &IrSpec, request_inputs: &RequestInputPlan) -> bool {
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
) -> String {
    let class_name = format!("{}Api", tag.to_pascal_case());
    let plans: Vec<OpPlan> = ops
        .iter()
        .map(|op| plan_operation(op, ir, request_inputs))
        .collect();

    let filename = format!("{class_name}.java");
    let mut fb = FileSpec::builder_with(&filename, Java::new())
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
        .add_import(ImportSpec::named("java.io", "IOException"))
        .add_import(ImportSpec::named("java.nio.charset", "StandardCharsets"))
        .add_import(ImportSpec::named("java.util", "HashMap"))
        .add_import(ImportSpec::named("java.util", "List"))
        .add_import(ImportSpec::named("java.util", "Map"))
        .add_import(ImportSpec::named("java.util.stream", "Collectors"))
        .add_import(ImportSpec::named("okhttp3", "Request"))
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
    if has_raw_request_body {
        fb = fb.add_import(ImportSpec::named("okhttp3", "RequestBody"));
    }
    if has_supported_multipart_body || has_raw_request_body {
        fb = fb.add_import(ImportSpec::named("okhttp3", "MediaType"));
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
        ParameterSpec::new("client", TypeName::primitive("ApiClient")).expect("client param"),
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

fn operation_support_files(
    plan: &OpPlan<'_>,
    package_name: &str,
    header: &str,
    has_models: bool,
) -> Vec<FileInfo> {
    let mut files = Vec::new();
    let mut response_imports = vec![
        ImportSpec::named("java.util", "List"),
        ImportSpec::named("java.util", "Map"),
        ImportSpec::named("okhttp3", "Response"),
    ];
    if has_models {
        response_imports.push(ImportSpec::named(&format!("{package_name}.models"), "*"));
    }
    files.push(java_type_file(
        &plan.response_type,
        package_name,
        header,
        response_imports,
        build_response_class(plan),
    ));

    let detail_interface = format!("{}Detail", plan.error_type);
    let mut detail_impl_names = Vec::new();
    let mut detail_seen = HashSet::new();
    for response in &plan.error_responses {
        if !detail_seen.insert(response.field_name.clone()) {
            continue;
        }
        detail_impl_names.push(format!(
            "{}{}",
            plan.method_name.to_pascal_case(),
            response.field_name
        ));
    }
    let unexpected = format!("{}Unexpected", plan.method_name.to_pascal_case());
    detail_impl_names.push(unexpected.clone());
    files.push(java_code_file(
        &detail_interface,
        package_name,
        header,
        Vec::new(),
        java_error_detail_interface(&detail_interface, &detail_impl_names),
    ));

    files.push(java_code_file(
        &plan.error_type,
        package_name,
        header,
        vec![
            ImportSpec::named(&format!("{package_name}.runtime"), "ApiException"),
            ImportSpec::named("okhttp3", "Headers"),
        ],
        java_error_exception_class(&plan.error_type, &detail_interface, &plan.error_headers),
    ));

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
        files.push(java_code_file(
            &class_name,
            package_name,
            header,
            error_detail_imports(package_name, has_models),
            java_error_detail_class(&class_name, &detail_interface, response),
        ));
    }

    files.push(java_code_file(
        &unexpected,
        package_name,
        header,
        Vec::new(),
        java_unexpected_detail_class(&unexpected, &detail_interface),
    ));
    files
}

fn java_type_file(
    class_name: &str,
    package_name: &str,
    header: &str,
    imports: Vec<ImportSpec>,
    type_spec: TypeSpec,
) -> FileInfo {
    let filename = format!("{class_name}.java");
    let mut fb =
        FileSpec::builder_with(&filename, Java::new()).header(package_header(package_name));
    for import in imports {
        fb = fb.add_import(import);
    }
    fb = fb.add_type(type_spec);
    let file = fb.build().expect("Java support type file builds");
    FileInfo::api(
        filename,
        format!(
            "{header}{}",
            file.render(RENDER_WIDTH)
                .expect("Java support type file renders")
        ),
    )
}

fn java_code_file(
    class_name: &str,
    package_name: &str,
    header: &str,
    imports: Vec<ImportSpec>,
    code: CodeBlock,
) -> FileInfo {
    let filename = format!("{class_name}.java");
    let mut fb =
        FileSpec::builder_with(&filename, Java::new()).header(package_header(package_name));
    for import in imports {
        fb = fb.add_import(import);
    }
    fb = fb.add_code(code);
    let file = fb.build().expect("Java support code file builds");
    FileInfo::api(
        filename,
        format!(
            "{header}{}",
            file.render(RENDER_WIDTH)
                .expect("Java support code file renders")
        ),
    )
}

fn error_detail_imports(package_name: &str, has_models: bool) -> Vec<ImportSpec> {
    let mut imports = vec![
        ImportSpec::named("com.google.gson", "Gson"),
        ImportSpec::named("com.google.gson.reflect", "TypeToken"),
        ImportSpec::named("java.nio.charset", "StandardCharsets"),
        ImportSpec::named("java.util", "List"),
        ImportSpec::named("java.util", "Map"),
    ];
    if has_models {
        imports.push(ImportSpec::named(&format!("{package_name}.models"), "*"));
    }
    imports
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
        .add_param(ParameterSpec::new("statusCode", TypeName::primitive("int")).expect("param"));
    ctor =
        ctor.add_param(ParameterSpec::new("raw", TypeName::primitive("Response")).expect("param"));
    let mut ctor_seen: HashSet<String> = HashSet::new();
    for tr in &plan.typed_responses {
        if !ctor_seen.insert(tr.field_name.clone()) {
            continue;
        }
        ctor = ctor.add_param(
            ParameterSpec::new(&tr.field_name, TypeName::primitive(&tr.java_type)).expect("param"),
        );
    }
    let mut assignment_fields = Vec::new();
    let mut body_seen: HashSet<String> = HashSet::new();
    for tr in &plan.typed_responses {
        if !body_seen.insert(tr.field_name.clone()) {
            continue;
        }
        assignment_fields.push(tr.field_name.clone());
    }
    let ctor_body = sigil_quote!(Java {
        this.statusCode = statusCode;
        this.raw = raw;
        $for(field_name in &assignment_fields) {
            this.$L(field_name.as_str()) = $L(field_name.as_str());
        }
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
    let header_method_names =
        unique_response_header_accessor_names(&plan.success_headers, |wire_name| {
            format!("get{}Header", wire_name.to_pascal_case())
        });
    for (header, method_name) in plan.success_headers.iter().zip(header_method_names) {
        tb = tb.add_method(build_java_header_getter(
            header,
            &method_name,
            "this.raw.header",
        ));
    }

    tb.build().expect("response class builds")
}

fn build_java_header_getter(
    header: &ResponseHeaderPlan,
    method_name: &str,
    lookup: &str,
) -> FunSpec {
    let return_type = java_header_type(header.value_kind);
    FunSpec::builder(method_name)
        .visibility(Visibility::Public)
        .returns(vec![TypeName::primitive(return_type)])
        .body(java_header_accessor_body(
            header.value_kind,
            lookup,
            &header.wire_name,
        ))
        .build()
        .expect("Java header getter builds")
}

fn java_error_detail_interface(detail_interface: &str, permits: &[String]) -> CodeBlock {
    let permits_clause = permits.join(", ");
    CodeBlock::of(
        &format!(
            "public sealed interface {detail_interface} permits {permits_clause} {{\n    int statusCode();\n    okhttp3.Headers headers();\n    byte[] rawBody();\n}}\n"
        ),
        (),
    )
    .expect("Java error detail interface builds")
}

fn java_error_exception_class(
    error_type: &str,
    detail_interface: &str,
    headers: &[ResponseHeaderPlan],
) -> CodeBlock {
    let mut cb = CodeBlock::builder();
    cb.add(
        &format!(
            "public final class {error_type} extends ApiException {{\n    private final {detail_interface} detail;\n    private final Headers headers;\n    private final byte[] rawBody;\n\n    public {error_type}(int statusCode, String status, String body, Headers headers, byte[] rawBody, {detail_interface} detail) {{\n        super(statusCode, status, body);\n        this.headers = headers;\n        this.rawBody = rawBody.clone();\n        this.detail = detail;\n    }}\n\n    public {detail_interface} detail() {{\n        return this.detail;\n    }}\n\n    public Headers headers() {{\n        return this.headers;\n    }}\n\n    public byte[] rawBody() {{\n        return this.rawBody.clone();\n    }}\n"
        ),
        (),
    );
    cb.add("%>", ());
    let method_names = unique_response_header_accessor_names(headers, |wire_name| {
        format!("get{}Header", wire_name.to_pascal_case())
    });
    for (header, method_name) in headers.iter().zip(method_names) {
        cb.add_line();
        cb.add_code(
            build_java_header_getter(header, &method_name, "this.headers.get")
                .emit(&Java::new(), DeclarationContext::Member)
                .expect("Java error header getter emits"),
        );
    }
    cb.add("%<}\n", ());
    cb.build().expect("Java error exception class builds")
}

fn java_header_type(kind: ResponseHeaderValueKind) -> &'static str {
    match kind {
        ResponseHeaderValueKind::String => "String",
        ResponseHeaderValueKind::Integer => "Long",
        ResponseHeaderValueKind::Number => "Double",
        ResponseHeaderValueKind::Boolean => "Boolean",
    }
}

fn java_header_accessor_body(
    kind: ResponseHeaderValueKind,
    lookup: &str,
    wire_name: &str,
) -> CodeBlock {
    let integer_pattern = r"[+-]?[0-9]+";
    let number_pattern = r"[+-]?(?:[0-9]+(?:[.][0-9]*)?|[.][0-9]+)(?:[eE][+-]?[0-9]+)?";
    match kind {
        ResponseHeaderValueKind::String => sigil_quote!(Java {
            return $L(lookup)($S(wire_name));
        }),
        ResponseHeaderValueKind::Integer => sigil_quote!(Java {
            String value = $L(lookup)($S(wire_name));
            if (value == null || !value.matches($S(integer_pattern))) {
                return null;
            }
            try {
                return Long.valueOf(value);
            } catch (NumberFormatException ignored) {
                return null;
            }
        }),
        ResponseHeaderValueKind::Number => sigil_quote!(Java {
            String value = $L(lookup)($S(wire_name));
            if (value == null || !value.matches($S(number_pattern))) {
                return null;
            }
            try {
                Double parsed = Double.valueOf(value);
                return Double.isFinite(parsed) ? parsed : null;
            } catch (NumberFormatException ignored) {
                return null;
            }
        }),
        ResponseHeaderValueKind::Boolean => sigil_quote!(Java {
            String value = $L(lookup)($S(wire_name));
            if ($S("true").equals(value)) {
                return true;
            }
            if ($S("false").equals(value)) {
                return false;
            }
            return null;
        }),
    }
    .expect("Java header getter body builds")
}

fn java_unexpected_detail_class(unexpected: &str, detail_interface: &str) -> CodeBlock {
    CodeBlock::of(
        &format!(
            "public final class {unexpected} implements {detail_interface} {{\n    private final int statusCode;\n    private final okhttp3.Headers headers;\n    private final byte[] rawBody;\n\n    public {unexpected}(int statusCode, okhttp3.Headers headers, byte[] rawBody) {{\n        this.statusCode = statusCode;\n        this.headers = headers;\n        this.rawBody = rawBody.clone();\n    }}\n\n    public int statusCode() {{ return this.statusCode; }}\n    public okhttp3.Headers headers() {{ return this.headers; }}\n    public byte[] rawBody() {{ return this.rawBody.clone(); }}\n    public byte[] body() {{ return this.rawBody.clone(); }}\n}}\n"
        ),
        (),
    )
    .expect("Java unexpected detail class builds")
}

fn java_error_detail_class(
    class_name: &str,
    detail_interface: &str,
    response: &TypedResponse,
) -> CodeBlock {
    let body_method = java_error_body_method(response);
    let text_helper = java_error_text_helper(response);
    let block = format!(
        "public final class {class_name} implements {detail_interface} {{\n    private final int statusCode;\n    private final okhttp3.Headers headers;\n    private final byte[] rawBody;\n\n    public {class_name}(int statusCode, okhttp3.Headers headers, byte[] rawBody) {{\n        this.statusCode = statusCode;\n        this.headers = headers;\n        this.rawBody = rawBody.clone();\n    }}\n\n    public int statusCode() {{ return this.statusCode; }}\n    public okhttp3.Headers headers() {{ return this.headers; }}\n    public byte[] rawBody() {{ return this.rawBody.clone(); }}\n{text_helper}{body_method}\n}}\n"
    );
    CodeBlock::of(&block, ()).expect("Java error detail class builds")
}

fn java_error_text_helper(response: &TypedResponse) -> &'static str {
    match response.decoding {
        ResponseDecoding::Json | ResponseDecoding::Text => {
            "\n    private String textBody() {\n        String contentTypeHeader = this.headers.get(\"Content-Type\");\n        okhttp3.MediaType contentType = contentTypeHeader != null ? okhttp3.MediaType.parse(contentTypeHeader) : null;\n        java.nio.charset.Charset charset = contentType != null ? contentType.charset(StandardCharsets.UTF_8) : StandardCharsets.UTF_8;\n        return new String(this.rawBody, charset);\n    }\n"
        }
        ResponseDecoding::Bytes => "",
    }
}

fn java_error_body_method(response: &TypedResponse) -> String {
    match response.decoding {
        ResponseDecoding::Json => format!(
            "    public {} body() {{\n        String text = textBody();\n        return new Gson().fromJson(text.isEmpty() ? \"null\" : text, new TypeToken<{}>() {{}}.getType());\n    }}",
            response.java_type, response.java_type
        ),
        ResponseDecoding::Text => {
            "    public String body() {\n        return textBody();\n    }".to_string()
        }
        ResponseDecoding::Bytes => {
            "    public byte[] body() {\n        return this.rawBody.clone();\n    }".to_string()
        }
    }
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
            ParameterSpec::new(&p.var_name, TypeName::raw(&p.java_type)).expect("param"),
        );
    }
    if let Some(body) = &plan.body {
        fb = fb.add_param(
            ParameterSpec::new(&body.var_name, TypeName::raw(&body.java_type)).expect("body param"),
        );
    }

    fb = fb.returns(vec![TypeName::primitive(&plan.response_type)]);
    fb = fb.throws(vec![TypeName::importable("java.io", "IOException")]);
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
            cb.add_code(java_query_param_put(
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
        if let Some(message) = unsupported_request_body_message(body) {
            cb.add_code(
                sigil_quote!(Java {
                    throw new IllegalArgumentException($S(message));
                })
                .expect("unsupported request body builds"),
            );
            return cb.build().expect("method body builds");
        }
        cb.add_statement("Request request", ());
        if body.encoding == BodyEncoding::Multipart {
            if let Some(parts) = &body.multipart_parts {
                emit_multipart_body(&mut cb, body, parts);
                cb.add_code(java_new_request_with_body(
                    &method,
                    has_query,
                    "multipartBody",
                ));
            } else {
                cb.add_statement(
                    "throw new IllegalArgumentException(\"unsupported multipart request body: schema must be object-shaped\")",
                    (),
                );
            }
        } else {
            emit_request_body(&mut cb, body);
            cb.add_code(java_new_request_with_body(
                &method,
                has_query,
                "requestBody",
            ));
        }
    } else {
        cb.add_code(java_new_request(&method, has_query));
    }

    // Headers
    for p in &plan.header_params {
        let stringified = render_value_as_string(&p.var_name, &p.param.type_expr);
        cb.add_code(java_header_param_set(
            p.param.required,
            &p.var_name,
            &p.param.name,
            &stringified,
        ));
    }

    // Execute
    cb.add_statement("Response response = client.execute(request)", ());
    cb.add_line();
    cb.add_code(java_error_throw(plan));

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
        }
        cb.add_code(java_response_decode_assignments(&plan.typed_responses));

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
        cb.add_code(
            sigil_quote!(Java {
                return new $N(plan.response_type.as_str())($for(arg in &dedup_args; separator = ", ") { $L(arg.as_str()) });
            })
            .expect("typed response constructor return"),
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

fn java_new_request(method: &str, has_query: bool) -> CodeBlock {
    let with_query =
        format!("Request request = client.newRequest(\"{method}\", path, query, null);");
    let without_query =
        format!("Request request = client.newRequest(\"{method}\", path, null, null);");
    sigil_quote!(Java {
        $if(has_query) {
            $L(with_query.as_str())
        } $else {
            $L(without_query.as_str())
        }
    })
    .expect("Java request construction builds")
}

fn java_new_request_with_body(method: &str, has_query: bool, body_expr: &str) -> CodeBlock {
    let with_query =
        format!("request = client.newRequestWithBody(\"{method}\", path, query, {body_expr});");
    let without_query =
        format!("request = client.newRequestWithBody(\"{method}\", path, null, {body_expr});");
    sigil_quote!(Java {
        $if(has_query) {
            $L(with_query.as_str())
        } $else {
            $L(without_query.as_str())
        }
    })
    .expect("Java request body construction builds")
}

fn java_query_param_put(
    required: bool,
    var_name: &str,
    param_name: &str,
    value_expr: &str,
) -> CodeBlock {
    sigil_quote!(Java {
        $if(required) {
            query.put($S(param_name), $L(value_expr));
        } $else {
            if ($L(var_name) != null) {
                query.put($S(param_name), $L(value_expr));
            }
        }
    })
    .expect("Java query param put builds")
}

fn java_header_param_set(
    required: bool,
    var_name: &str,
    param_name: &str,
    value_expr: &str,
) -> CodeBlock {
    sigil_quote!(Java {
        $if(required) {
            request = request.newBuilder().header($S(param_name), $L(value_expr)).build();
        } $else {
            if ($L(var_name) != null) {
                request = request.newBuilder().header($S(param_name), $L(value_expr)).build();
            }
        }
    })
    .expect("Java header param set builds")
}

fn emit_multipart_body(
    cb: &mut sigil_stitch::code_block::CodeBlockBuilder,
    body: &BodyBinding,
    parts: &[MultipartPart],
) {
    if !body.required {
        cb.add_code(
            sigil_quote!(Java {
                RequestBody multipartBody = RequestBody.create(new byte[0], null);
            })
            .expect("default multipart body builds"),
        );
        cb.begin_control_flow(&format!("if ({} != null)", body.var_name), ());
    }
    cb.add_code(
        sigil_quote!(Java {
            MultipartBody.Builder multipartBuilder = new MultipartBody.Builder().setType(MultipartBody.FORM);
        })
        .expect("multipart builder builds"),
    );
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
    cb.add_code(java_multipart_body_finish(body.required));
    if !body.required {
        cb.end_control_flow();
    }
}

fn java_multipart_body_finish(body_required: bool) -> CodeBlock {
    sigil_quote!(Java {
        $if(body_required) {
            RequestBody multipartBody = multipartBuilder.build();
        } $else {
            multipartBody = multipartBuilder.build();
        }
    })
    .expect("multipart body finish builds")
}

fn emit_request_body(cb: &mut sigil_stitch::code_block::CodeBlockBuilder, body: &BodyBinding) {
    if !body.required {
        cb.add_code(
            sigil_quote!(Java {
                RequestBody requestBody = RequestBody.create(new byte[0], null);
            })
            .expect("default request body builds"),
        );
        cb.begin_control_flow(&format!("if ({} != null)", body.var_name), ());
    }
    match body.encoding {
        BodyEncoding::Json => {
            let body_var = body.var_name.as_str();
            let media_type = body.media_type.as_str();
            cb.add_code(java_json_request_body(body.required, body_var, media_type));
        }
        BodyEncoding::TextPlain | BodyEncoding::OctetStream => {
            let body_var = body.var_name.as_str();
            let media_type = body.media_type.as_str();
            cb.add_code(java_raw_request_body(body.required, body_var, media_type));
        }
        BodyEncoding::FormUrlEncoded | BodyEncoding::Xml | BodyEncoding::Other => {
            let message = format!("unsupported request body media type: {}", body.media_type);
            cb.add_code(
                sigil_quote!(Java {
                    throw new IllegalArgumentException($S(message));
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

fn java_json_request_body(body_required: bool, body_var: &str, media_type: &str) -> CodeBlock {
    sigil_quote!(Java {
        String jsonBody = gson.toJson($L(body_var));
        $if(body_required) {
            RequestBody requestBody = RequestBody.create(jsonBody, MediaType.get($S(media_type)));
        } $else {
            requestBody = RequestBody.create(jsonBody, MediaType.get($S(media_type)));
        }
    })
    .expect("json request body builds")
}

fn java_raw_request_body(body_required: bool, body_var: &str, media_type: &str) -> CodeBlock {
    sigil_quote!(Java {
        $if(body_required) {
            RequestBody requestBody = RequestBody.create($L(body_var), MediaType.get($S(media_type)));
        } $else {
            requestBody = RequestBody.create($L(body_var), MediaType.get($S(media_type)));
        }
    })
    .expect("raw request body builds")
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

fn java_response_decode_assignments(typed_responses: &[TypedResponse]) -> CodeBlock {
    let mut cb = CodeBlock::builder();
    let mut seen: HashSet<String> = HashSet::new();
    let mut emitted_any = false;
    for tr in typed_responses {
        if !seen.insert(tr.field_name.clone()) {
            continue;
        }
        let keyword = if emitted_any { "else if" } else { "if" };
        let guard = response_status_guard_java(&tr.status);
        let assignment = format!("{} = {}", tr.field_name, response_decode_expr(tr));
        cb.begin_control_flow(&format!("{keyword} ({guard})"), ());
        cb.add(&format!("{assignment};\n"), ());
        cb.end_control_flow();
        emitted_any = true;
    }
    cb.build().expect("Java response decode assignments build")
}

fn java_error_throw(plan: &OpPlan<'_>) -> CodeBlock {
    let mut cb = CodeBlock::builder();
    cb.begin_control_flow("if (!response.isSuccessful())", ());
    cb.add_statement("okhttp3.ResponseBody responseBody = response.body()", ());
    cb.add_statement(
        "java.nio.charset.Charset responseCharset = responseBody != null && responseBody.contentType() != null ? responseBody.contentType().charset(StandardCharsets.UTF_8) : StandardCharsets.UTF_8",
        (),
    );
    cb.add_statement(
        "byte[] responseBytes = responseBody != null ? responseBody.bytes() : new byte[0]",
        (),
    );
    cb.add_statement(
        "String responseText = new String(responseBytes, responseCharset)",
        (),
    );
    cb.add(&format!("{}Detail detail;\n", plan.error_type), ());
    cb.begin_control_flow("switch (response.code())", ());
    let mut seen = HashSet::new();
    for response in plan
        .error_responses
        .iter()
        .filter(|response| response.status.parse::<u16>().is_ok())
    {
        if !seen.insert(response.field_name.clone()) {
            continue;
        }
        cb.add(&format!("case {}:\n", response.status), ());
        cb.add("%>", ());
        cb.add_code(java_error_detail_assign(plan, response));
        cb.add("break;\n", ());
        cb.add("%<", ());
    }
    cb.add("default:\n", ());
    cb.add("%>", ());
    for response in plan
        .error_responses
        .iter()
        .filter(|response| response.status.ends_with("XX"))
    {
        cb.begin_control_flow(
            &format!("if ({})", wildcard_status_guard_java(&response.status)),
            (),
        );
        cb.add_code(java_error_detail_assign(plan, response));
        cb.add("break;\n", ());
        cb.end_control_flow();
    }
    if let Some(default) = plan
        .error_responses
        .iter()
        .find(|response| response.status.eq_ignore_ascii_case("default"))
    {
        cb.add_code(java_error_detail_assign(plan, default));
    } else {
        let unexpected = format!("{}Unexpected", plan.method_name.to_pascal_case());
        cb.add(
            &format!(
                "detail = new {unexpected}(response.code(), response.headers(), responseBytes);\n"
            ),
            (),
        );
    }
    cb.add("%<", ());
    cb.end_control_flow();
    cb.add(
        &format!(
            "throw new {}(response.code(), response.message(), responseText, response.headers(), responseBytes, detail);\n",
            plan.error_type
        ),
        (),
    );
    cb.end_control_flow();
    cb.build().expect("Java error throw builds")
}

fn java_error_detail_assign(plan: &OpPlan<'_>, response: &TypedResponse) -> CodeBlock {
    let class_name = format!(
        "{}{}",
        plan.method_name.to_pascal_case(),
        response.field_name
    );
    let stmt =
        format!("detail = new {class_name}(response.code(), response.headers(), responseBytes);");
    CodeBlock::of(&stmt, ()).expect("Java error detail assign builds")
}

fn emit_required_multipart_part(
    cb: &mut sigil_stitch::code_block::CodeBlockBuilder,
    part: &MultipartPart,
    access: &str,
) {
    cb.add_code(java_multipart_part(part, access));
}

fn java_multipart_part(part: &MultipartPart, access: &str) -> CodeBlock {
    let wire_name = part.wire_name.as_str();
    let content_type = part.content_type.as_str();
    sigil_quote!(Java {
        $if(part.is_binary) {
            multipartBuilder.addFormDataPart($S(wire_name), $L(access).filenameOrDefault($S(wire_name)), RequestBody.create($L(access).getData(), MediaType.get($S(content_type))));
        } $else_if(part.value_encoding == MultipartValueEncoding::Json) {
            multipartBuilder.addFormDataPart($S(wire_name), null, RequestBody.create(gson.toJson($L(access)), MediaType.get($S(content_type))));
        } $else_if(part.value_encoding == MultipartValueEncoding::Unsupported) {
            throw new IllegalArgumentException($S("unsupported multipart part content type"));
        } $else {
            multipartBuilder.addFormDataPart($S(wire_name), null, RequestBody.create(String.valueOf($L(access)), MediaType.get($S(content_type))));
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
    let java_type = match encoding {
        BodyEncoding::TextPlain => "String".to_string(),
        BodyEncoding::OctetStream => "byte[]".to_string(),
        BodyEncoding::Multipart => request_input_for_operation(request_inputs, op, &media_type)
            .map(|input| input.name.to_pascal_case())
            .unwrap_or_else(|| java_type_str(&t)),
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

fn plan_error_response(r: &IrResponse) -> TypedResponse {
    match pick_response_content(r) {
        Some((media_type, t)) => {
            let decoding = response_decoding(&media_type);
            let java_type = match decoding {
                ResponseDecoding::Json => java_type_str(&t),
                ResponseDecoding::Text => "String".to_string(),
                ResponseDecoding::Bytes => "byte[]".to_string(),
            };
            TypedResponse {
                status: r.status.clone(),
                field_name: response_variant_name(&r.status),
                java_type,
                decoding,
            }
        }
        None => TypedResponse {
            status: r.status.clone(),
            field_name: response_variant_name(&r.status),
            java_type: "byte[]".to_string(),
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
    } else if upper == "2XX" {
        "response.code() >= 200 && response.code() < 300".to_string()
    } else {
        // "default" or unknown wildcard: match everything (fallback response)
        "true".to_string()
    }
}

fn response_status_guard_java(status: &str) -> String {
    status
        .parse::<u16>()
        .map(|code| format!("response.code() == {code}"))
        .unwrap_or_else(|_| wildcard_status_guard_java(status))
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
