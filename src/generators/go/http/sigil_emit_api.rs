//! API emission for IR operations (Go APIs).
//!
//! Groups operations by tag, emits one `apis/<tag>.go` in package `apis`. Each
//! file declares a `{Tag}API` struct carrying a `*runtime.Client` and exposes
//! one method per operation. Per-operation `{OperationID}Response` carries
//! `StatusCode`, `Raw *http.Response`, and typed payload fields for successful
//! responses.
//!
//! Non-2xx responses are surfaced as `*runtime.APIError` so callers can switch
//! on error via `errors.As`.
//!
//! File-level structure (package declaration, imports, struct/func declarations)
//! is assembled via sigil-stitch's `FileSpec`, `TypeSpec`, `FunSpec`, and
//! `CodeBlock` builders, giving us automatic import tracking through
//! `TypeName::importable`. Method bodies are emitted as `CodeBlock`s because
//! the imperative Go code (path templating, query building, JSON
//! marshal/unmarshal, status switch) does not fit the structural builders.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::codegen::traits::file_writer::FileInfo;
use crate::generators::multipart::{MultipartValueEncoding, multipart_parts_for_request_body};
use crate::generators::request_inputs::{RequestInputPlan, request_input_for_operation};
use crate::ir::types::{
    IrOperation, IrParameter, IrPrimitive, IrRequestBody, IrResponse, IrSpec, IrTypeExpr,
    ParameterLocation,
};
use heck::{ToLowerCamelCase, ToPascalCase, ToSnakeCase};
use sigil_stitch::code_block::CodeBlock;
use sigil_stitch::prelude::sigil_quote;
use sigil_stitch::spec::field_spec::FieldSpec;
use sigil_stitch::spec::file_spec::FileSpec;
use sigil_stitch::spec::fun_spec::FunSpec;
use sigil_stitch::spec::import_spec::ImportSpec;
use sigil_stitch::spec::modifiers::TypeKind;
use sigil_stitch::spec::parameter_spec::ParameterSpec;
use sigil_stitch::spec::type_spec::TypeSpec;
use sigil_stitch::type_name::TypeName;

const APIS_PACKAGE: &str = "apis";
const RENDER_WIDTH: usize = 100;

/// Generate every API file from the IR.
pub fn generate_api_files(
    ir: &IrSpec,
    module_path: &str,
    header: &str,
    request_inputs: &RequestInputPlan,
) -> Result<Vec<FileInfo>, String> {
    let by_tag = group_by_tag(&ir.operations);
    let mut files = Vec::with_capacity(by_tag.len());
    for (tag, ops) in &by_tag {
        let stem = tag.to_snake_case();
        // Avoid the `_test.go` suffix (Go treats those as test files).
        let filename = if stem.ends_with("_test") {
            format!("{stem}_api.go")
        } else {
            format!("{stem}.go")
        };
        let body = emit_api_file(tag, ops, ir, module_path, request_inputs);
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
    module_path: &str,
    request_inputs: &RequestInputPlan,
) -> String {
    let struct_name = format!("{}API", tag.to_pascal_case());

    // Pre-plan each operation so we can build specs from the plans.
    let plans: Vec<OpPlan> = ops
        .iter()
        .map(|op| plan_operation(op, ir, request_inputs))
        .collect();

    let filename = format!("{}.go", tag.to_snake_case());
    let mut fb = FileSpec::builder(&filename)
        // Package header
        .header(package_header())
        // API struct type
        .add_type(build_api_struct(&struct_name, module_path))
        // Constructor function
        .add_function(build_constructor(&struct_name, module_path));

    // Body-level imports: packages used inside CodeBlock method bodies that
    // sigil can't infer from structural TypeName references.
    for import in collect_body_imports(&plans, module_path) {
        fb = fb.add_import(import);
    }

    // For each operation: response struct + method
    for plan in &plans {
        fb = fb
            .add_type(build_response_struct(plan, module_path))
            .add_function(build_operation_fun(&struct_name, plan));
    }

    let file = fb.build().expect("FileSpec builds for API file");
    file.render(RENDER_WIDTH)
        .expect("FileSpec renders for API file")
}

// ---------------------------------------------------------------------------
// Body-level import collection
// ---------------------------------------------------------------------------

fn collect_body_imports(plans: &[OpPlan<'_>], module_path: &str) -> Vec<ImportSpec> {
    let mut pkgs: BTreeSet<String> = BTreeSet::new();

    for plan in plans {
        let has_path_params = !plan.path_params.is_empty();
        let has_query_params = !plan.query_params.is_empty();
        let has_body = plan.body.is_some();
        let has_typed_responses = !plan.typed_responses.is_empty();

        if has_path_params {
            pkgs.insert("strings".to_string());
        }
        if has_query_params {
            pkgs.insert("net/url".to_string());
        }
        if has_body {
            pkgs.insert("io".to_string());
            if let Some(body) = &plan.body {
                match body.encoding {
                    BodyEncoding::Multipart => {
                        pkgs.insert("fmt".to_string());
                        if let Some(parts) = &body.multipart_parts {
                            pkgs.insert("bytes".to_string());
                            if parts.iter().any(|part| part.is_binary) {
                                pkgs.insert("mime".to_string());
                            }
                            pkgs.insert("mime/multipart".to_string());
                            pkgs.insert("net/textproto".to_string());
                            for part in parts {
                                collect_stringify_imports(&part.type_expr, &mut pkgs);
                            }
                            if parts
                                .iter()
                                .any(|part| part.value_encoding == MultipartValueEncoding::Json)
                            {
                                pkgs.insert("encoding/json".to_string());
                            }
                        }
                    }
                    BodyEncoding::Json => {
                        pkgs.insert("bytes".to_string());
                        pkgs.insert("encoding/json".to_string());
                        pkgs.insert("fmt".to_string());
                    }
                    BodyEncoding::TextPlain => {
                        pkgs.insert("strings".to_string());
                    }
                    BodyEncoding::OctetStream => {
                        pkgs.insert("bytes".to_string());
                    }
                    BodyEncoding::FormUrlEncoded | BodyEncoding::Xml | BodyEncoding::Other => {
                        pkgs.insert("fmt".to_string());
                    }
                }
            }
        }
        // io.ReadAll used in error handling (4xx branch)
        pkgs.insert("io".to_string());
        if has_typed_responses {
            pkgs.insert("fmt".to_string());
            if plan
                .typed_responses
                .iter()
                .any(|tr| tr.decoding == ResponseDecoding::Json)
            {
                pkgs.insert("encoding/json".to_string());
            }
        }

        // Check if any param stringification needs strconv or fmt
        for p in plan
            .path_params
            .iter()
            .chain(&plan.query_params)
            .chain(&plan.header_params)
        {
            collect_stringify_imports(&p.param.type_expr, &mut pkgs);
        }

        // Check if body or any typed response references models
        if let Some(b) = &plan.body
            && b.go_type.contains("models.")
        {
            pkgs.insert(format!("{module_path}/models"));
        }
        for tr in &plan.typed_responses {
            if tr.go_type.contains("models.") {
                pkgs.insert(format!("{module_path}/models"));
            }
        }
    }

    pkgs.into_iter()
        .map(|pkg| {
            let name = pkg.rsplit('/').next().unwrap_or(&pkg);
            ImportSpec::named(&pkg, name)
        })
        .collect()
}

fn collect_stringify_imports(t: &IrTypeExpr, pkgs: &mut BTreeSet<String>) {
    match t {
        IrTypeExpr::Primitive(
            IrPrimitive::String
            | IrPrimitive::Date
            | IrPrimitive::DateTime
            | IrPrimitive::Uuid
            | IrPrimitive::StringWithFormat(_)
            | IrPrimitive::Binary,
        )
        | IrTypeExpr::StringLiteral(_)
        | IrTypeExpr::StringEnum(_)
        | IrTypeExpr::Named(_) => {}
        IrTypeExpr::Primitive(IrPrimitive::Boolean)
        | IrTypeExpr::Primitive(IrPrimitive::Integer)
        | IrTypeExpr::Primitive(IrPrimitive::IntegerWithFormat(_))
        | IrTypeExpr::Primitive(IrPrimitive::Number)
        | IrTypeExpr::Primitive(IrPrimitive::NumberWithFormat(_)) => {
            pkgs.insert("strconv".to_string());
        }
        IrTypeExpr::Nullable(inner) => collect_stringify_imports(inner, pkgs),
        IrTypeExpr::Array(inner) => {
            pkgs.insert("strings".to_string());
            if !is_stringish_primitive(inner) {
                pkgs.insert("fmt".to_string());
            }
            collect_stringify_imports(inner, pkgs);
        }
        _ => {
            pkgs.insert("fmt".to_string());
        }
    }
}

fn is_stringish_primitive(t: &IrTypeExpr) -> bool {
    matches!(
        t,
        IrTypeExpr::Primitive(
            IrPrimitive::String
                | IrPrimitive::Date
                | IrPrimitive::DateTime
                | IrPrimitive::Uuid
                | IrPrimitive::StringWithFormat(_)
        ) | IrTypeExpr::StringLiteral(_)
            | IrTypeExpr::StringEnum(_)
    )
}

// ---------------------------------------------------------------------------
// Structural builders
// ---------------------------------------------------------------------------

/// Build a `package apis` header block.
fn package_header() -> CodeBlock {
    sigil_quote!(GoLang {
        package $L(APIS_PACKAGE)
    })
    .expect("package header builds")
}

/// Build the API struct: `type {Name}API struct { client *runtime.Client }`
fn build_api_struct(struct_name: &str, module_path: &str) -> TypeSpec {
    let runtime_client_ty = TypeName::importable(&format!("{module_path}/runtime"), "Client");

    TypeSpec::builder(struct_name, TypeKind::Struct)
        .doc(&format!(
            "{struct_name} groups operations under the corresponding tag."
        ))
        .add_field(
            FieldSpec::builder("client", TypeName::pointer(runtime_client_ty))
                .build()
                .expect("client field builds"),
        )
        .build()
        .expect("API struct TypeSpec builds")
}

/// Build the constructor: `func New{Name}(client *runtime.Client) *{Name} { ... }`
fn build_constructor(struct_name: &str, module_path: &str) -> FunSpec {
    let runtime_client_ty = TypeName::importable(&format!("{module_path}/runtime"), "Client");
    let func_name = format!("New{struct_name}");

    let body = sigil_quote!(GoLang {
        return &$N(struct_name){client: client};
    })
    .expect("constructor body builds");

    FunSpec::builder(&func_name)
        .doc(&format!(
            "New{struct_name} constructs a {struct_name} bound to client."
        ))
        .add_param(
            ParameterSpec::new("client", TypeName::pointer(runtime_client_ty))
                .expect("param builds"),
        )
        .returns(TypeName::pointer(TypeName::primitive(struct_name)))
        .body(body)
        .build()
        .expect("constructor FunSpec builds")
}

/// Build the response struct for an operation.
fn build_response_struct(plan: &OpPlan<'_>, module_path: &str) -> TypeSpec {
    let _ = module_path;

    // Raw *http.Response
    let http_response_ty = TypeName::importable("net/http", "Response");

    let mut tb = TypeSpec::builder(&plan.response_type, TypeKind::Struct)
        .doc(&format!(
            "{} carries the response from the corresponding operation.",
            plan.response_type
        ))
        // StatusCode int
        .add_field(
            FieldSpec::builder("StatusCode", TypeName::primitive("int"))
                .build()
                .expect("StatusCode field builds"),
        )
        // Raw *http.Response
        .add_field(
            FieldSpec::builder("Raw", TypeName::pointer(http_response_ty))
                .build()
                .expect("Raw field builds"),
        );

    // Typed response payload fields
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for tr in &plan.typed_responses {
        if !seen.insert(tr.field_name.clone()) {
            continue;
        }
        let type_name = pointerize_type_name(&tr.go_type);
        tb = tb.add_field(
            FieldSpec::builder(&tr.field_name, type_name)
                .build()
                .expect("response payload field builds"),
        );
    }

    tb.build().expect("response struct TypeSpec builds")
}

/// Build a FunSpec for an operation method.
fn build_operation_fun(struct_name: &str, plan: &OpPlan<'_>) -> FunSpec {
    let OpPlan {
        op,
        method_name,
        response_type,
        ..
    } = plan;

    let mut fb = FunSpec::builder(method_name);

    // Doc comment
    if let Some(summary) = &op.summary {
        fb = fb.doc(&format!("{method_name} \u{2014} {summary}"));
    } else {
        fb = fb.doc(&format!(
            "{method_name} calls {} {}.",
            op.method.to_uppercase(),
            op.path,
        ));
    }
    if let Some(desc) = &op.description {
        fb = fb.doc("");
        for line in desc.lines() {
            fb = fb.doc(line);
        }
    }

    // Receiver: (a *{StructName})
    fb = fb.receiver(
        ParameterSpec::new("a", TypeName::pointer(TypeName::primitive(struct_name)))
            .expect("receiver builds"),
    );

    // Parameters: ctx context.Context, then path/query/header params, then body
    let context_ty = TypeName::importable("context", "Context");
    fb = fb.add_param(ParameterSpec::new("ctx", context_ty).expect("ctx param builds"));

    for p in plan
        .path_params
        .iter()
        .chain(&plan.query_params)
        .chain(&plan.header_params)
    {
        let type_name = TypeName::raw(&p.go_type);
        fb = fb.add_param(ParameterSpec::new(&p.var_name, type_name).expect("param builds"));
    }
    if let Some(body) = &plan.body {
        let type_name = TypeName::raw(&body.go_type);
        fb =
            fb.add_param(ParameterSpec::new(&body.var_name, type_name).expect("body param builds"));
    }

    // Return type: (*{Response}, error)
    fb = fb.returns(TypeName::raw(&format!("(*{response_type}, error)")));

    // Body
    fb = fb.body(emit_method_body(plan));

    fb.build().expect("operation FunSpec builds")
}

// ---------------------------------------------------------------------------
// Planning: resolve parameter names / types so emission is deterministic
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
    go_type: String,
    is_pointer: bool,
}

struct BodyBinding {
    var_name: String,
    go_type: String,
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
    status: String,
    field_name: String,
    go_type: String,
    decoding: ResponseDecoding,
}

fn plan_operation<'a>(
    op: &'a IrOperation,
    ir: &IrSpec,
    request_inputs: &RequestInputPlan,
) -> OpPlan<'a> {
    let op_id = sanitize_operation_id(&op.operation_id, &op.method, &op.path);
    let method_name = op_id.to_pascal_case();
    let response_type = format!("{method_name}Response");

    let mut used_names: HashSet<String> = HashSet::new();
    used_names.insert("ctx".to_string());
    used_names.insert("a".to_string());

    let mut path_params = Vec::new();
    let mut query_params = Vec::new();
    let mut header_params = Vec::new();
    for p in &op.parameters {
        let var_name = unique_name(&go_ident(&p.name), &mut used_names);
        let (go_type, is_pointer) = param_go_type(p);
        let binding = ParamBinding {
            param: p,
            var_name,
            go_type,
            is_pointer,
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
    let base_ty = match encoding {
        BodyEncoding::TextPlain => "string".to_string(),
        BodyEncoding::OctetStream => "[]byte".to_string(),
        BodyEncoding::Multipart => request_input_for_operation(request_inputs, op, &media_type)
            .map(|input| format!("models.{}", input.name.to_pascal_case()))
            .unwrap_or_else(|| go_type_str(&t)),
        _ => go_type_str(&t),
    };
    let go_type =
        if base_ty.starts_with('[') || base_ty.starts_with("map[") || base_ty.starts_with('*') {
            base_ty
        } else {
            format!("*{base_ty}")
        };
    let var_name = unique_name("body", used_names);
    let multipart_parts = if media_type_base(&media_type) == "multipart/form-data" {
        multipart_parts_for(b, &media_type, ir)
    } else {
        None
    };
    Some(BodyBinding {
        var_name,
        go_type,
        media_type,
        encoding,
        multipart_parts,
    })
}

fn plan_response(r: &IrResponse) -> Option<TypedResponse> {
    let (media_type, t) = pick_response_content(r)?;
    let decoding = response_decoding(&media_type);
    let go_type = match decoding {
        ResponseDecoding::Json => go_type_str(&t),
        ResponseDecoding::Text => "string".to_string(),
        ResponseDecoding::Bytes => "[]byte".to_string(),
    };
    Some(TypedResponse {
        status: r.status.clone(),
        field_name: response_field_name(&r.status),
        go_type,
        decoding,
    })
}

fn param_go_type(p: &IrParameter) -> (String, bool) {
    let base = go_type_str(&p.type_expr);
    if p.required || base.starts_with('*') || base.starts_with('[') || base.starts_with("map[") {
        let pointer = base.starts_with('*');
        (base, pointer)
    } else {
        (format!("*{base}"), true)
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

// ---------------------------------------------------------------------------
// Method body emission (CodeBlock)
// ---------------------------------------------------------------------------

fn emit_method_body(plan: &OpPlan<'_>) -> CodeBlock {
    let OpPlan {
        op,
        response_type,
        path_params,
        query_params,
        header_params,
        body,
        ..
    } = plan;

    let mut cb = CodeBlock::builder();

    // Path.
    cb.add_code(path_init(&op.path));
    for p in path_params {
        let placeholder = format!("{{{}}}", p.param.name);
        let value_expr = deref_if_pointer(&p.var_name, p.is_pointer);
        let stringified = render_value_as_string(&value_expr, &p.param.type_expr);
        cb.add_code(path_replace(&placeholder, &stringified));
    }

    // Query.
    let has_query = !query_params.is_empty();
    if has_query {
        cb.add_code(query_init());
        for p in query_params {
            let value_expr = deref_if_pointer(&p.var_name, p.is_pointer);
            let stringified = render_value_as_string(&value_expr, &p.param.type_expr);
            cb.add_code(query_set_guarded(
                p.param.required || !p.is_pointer,
                &p.var_name,
                &p.param.name,
                &stringified,
            ));
        }
    }

    // Body.
    if let Some(body) = body {
        cb.add_code(body_reader_decl());
        match body.encoding {
            BodyEncoding::Multipart => {
                cb.add_code(multipart_content_type_decl());
                if let Some(parts) = &body.multipart_parts {
                    emit_multipart_body(&mut cb, body, parts);
                } else {
                    cb.add_code(return_unsupported_multipart_request_body());
                }
            }
            BodyEncoding::Json => {
                cb.begin_control_flow(&format!("if {} != nil", body.var_name), ());
                cb.add_code(json_marshal_body(&body.var_name));
                cb.begin_control_flow("if err != nil", ());
                cb.add_code(return_marshal_body_error());
                cb.end_control_flow();
                cb.add_code(body_reader_bytes_buffer());
                cb.end_control_flow();
            }
            BodyEncoding::TextPlain => {
                cb.begin_control_flow(&format!("if {} != nil", body.var_name), ());
                cb.add_code(body_reader_string_pointer(&body.var_name));
                cb.end_control_flow();
            }
            BodyEncoding::OctetStream => {
                cb.begin_control_flow(&format!("if {} != nil", body.var_name), ());
                cb.add_code(body_reader_bytes(&body.var_name));
                cb.end_control_flow();
            }
            BodyEncoding::FormUrlEncoded | BodyEncoding::Xml | BodyEncoding::Other => {
                cb.add_code(return_unsupported_request_body_media_type(&body.media_type));
            }
        }
    }

    // Build request.
    cb.add_code(new_request_stmt(
        &op.method.to_uppercase(),
        has_query,
        body.is_some(),
    ));
    cb.begin_control_flow("if err != nil", ());
    cb.add_code(return_nil_err());
    cb.end_control_flow();

    // Headers.
    for p in header_params {
        let value_expr = deref_if_pointer(&p.var_name, p.is_pointer);
        let stringified = render_value_as_string(&value_expr, &p.param.type_expr);
        cb.add_code(set_header_guarded(
            p.param.required || !p.is_pointer,
            &p.var_name,
            &go_string_literal(&p.param.name),
            &stringified,
        ));
    }

    if let Some(body) = body {
        if body.encoding == BodyEncoding::Multipart {
            cb.begin_control_flow("if multipartContentType != \"\"", ());
            cb.add_code(set_header(
                &go_string_literal("Content-Type"),
                "multipartContentType",
            ));
            cb.end_control_flow();
        } else {
            cb.add_code(set_header(
                &go_string_literal("Content-Type"),
                &go_string_literal(&body.media_type),
            ));
        }
    }
    cb.add_code(set_header(
        &go_string_literal("Accept"),
        &go_string_literal("application/json"),
    ));

    // Dispatch.
    cb.add_code(do_request());
    cb.begin_control_flow("if err != nil", ());
    cb.add_code(return_nil_err());
    cb.end_control_flow();
    cb.add_code(defer_body_close());
    cb.add_line();

    cb.add_code(response_init(response_type));

    if !plan.typed_responses.is_empty() {
        let mut numeric_responses: Vec<&TypedResponse> = Vec::new();
        let mut wildcard_responses: Vec<&TypedResponse> = Vec::new();
        for tr in &plan.typed_responses {
            if tr.status.parse::<u16>().is_ok() {
                numeric_responses.push(tr);
            } else {
                wildcard_responses.push(tr);
            }
        }

        cb.begin_control_flow("switch httpResp.StatusCode", ());
        for tr in &numeric_responses {
            let code = tr.status.parse::<u16>().unwrap();
            cb.add(&format!("case {code}:"), ());
            cb.add_line();
            cb.add(
                "%L",
                emit_decode_into(&tr.field_name, &tr.go_type, tr.decoding),
            );
        }
        cb.add("default:", ());
        cb.add_line();
        cb.add("%>", ());
        for tr in &wildcard_responses {
            let (low, high) = wildcard_range(&tr.status);
            cb.begin_control_flow(
                &format!(
                    "if httpResp.StatusCode >= {} && httpResp.StatusCode < {}",
                    low, high
                ),
                (),
            );
            cb.add(
                "%L",
                emit_decode_into(&tr.field_name, &tr.go_type, tr.decoding),
            );
            cb.add_code(return_status_api_error());
            cb.end_control_flow();
        }
        cb.begin_control_flow("if httpResp.StatusCode >= 400", ());
        cb.add_code(error_body_read());
        cb.add_code(return_body_api_error());
        cb.end_control_flow();
        cb.add("%<", ());
        cb.end_control_flow();
    } else {
        cb.begin_control_flow("if httpResp.StatusCode >= 400", ());
        cb.add_code(error_body_read());
        cb.add_code(return_body_api_error());
        cb.end_control_flow();
    }

    cb.add_code(return_resp_nil());
    cb.build().expect("method body builds")
}

fn path_init(path: &str) -> CodeBlock {
    let stmt = format!("path := {}", go_string_literal(path));
    sigil_quote!(GoLang {
        $L(stmt.as_str())
    })
    .expect("path init builds")
}

fn path_replace(placeholder: &str, value_expr: &str) -> CodeBlock {
    let stmt = format!(
        "path = strings.Replace(path, {}, {value_expr}, 1)",
        go_string_literal(placeholder)
    );
    sigil_quote!(GoLang {
        $L(stmt.as_str())
    })
    .expect("path replace builds")
}

fn query_init() -> CodeBlock {
    let stmt = "query := url.Values{}";
    sigil_quote!(GoLang {
        $L(stmt)
    })
    .expect("query init builds")
}

fn query_set_guarded(always_set: bool, var_name: &str, name: &str, value_expr: &str) -> CodeBlock {
    let stmt = format!("query.Set({}, {value_expr})", go_string_literal(name));
    let guard = format!("{var_name} != nil");
    sigil_quote!(GoLang {
        $if(always_set) {
            $L(stmt.as_str())
        } $else {
            if $L(guard.as_str()) {
                $L(stmt.as_str())
            }
        }
    })
    .expect("guarded query set builds")
}

fn body_reader_decl() -> CodeBlock {
    sigil_quote!(GoLang {
        var bodyReader io.Reader
    })
    .expect("body reader declaration builds")
}

fn multipart_content_type_decl() -> CodeBlock {
    sigil_quote!(GoLang {
        var multipartContentType string
    })
    .expect("multipart content type declaration builds")
}

fn return_unsupported_multipart_request_body() -> CodeBlock {
    sigil_quote!(GoLang {
        return nil, fmt.Errorf("unsupported multipart request body: schema must be object-shaped")
    })
    .expect("unsupported multipart request body builds")
}

fn json_marshal_body(body_var: &str) -> CodeBlock {
    let stmt = format!("buf, err := json.Marshal({body_var})");
    sigil_quote!(GoLang {
        $L(stmt.as_str())
    })
    .expect("JSON marshal body builds")
}

fn return_marshal_body_error() -> CodeBlock {
    sigil_quote!(GoLang {
        return nil, fmt.Errorf("marshal body: %w", err)
    })
    .expect("marshal body error builds")
}

fn body_reader_bytes_buffer() -> CodeBlock {
    sigil_quote!(GoLang {
        bodyReader = bytes.NewReader(buf)
    })
    .expect("body reader bytes buffer builds")
}

fn body_reader_string_pointer(body_var: &str) -> CodeBlock {
    let stmt = format!("bodyReader = strings.NewReader(*{body_var})");
    sigil_quote!(GoLang {
        $L(stmt.as_str())
    })
    .expect("body reader string pointer builds")
}

fn body_reader_bytes(body_var: &str) -> CodeBlock {
    let stmt = format!("bodyReader = bytes.NewReader({body_var})");
    sigil_quote!(GoLang {
        $L(stmt.as_str())
    })
    .expect("body reader bytes builds")
}

fn return_unsupported_request_body_media_type(media_type: &str) -> CodeBlock {
    let message = format!("unsupported request body media type: {media_type}");
    sigil_quote!(GoLang {
        return nil, fmt.Errorf($S(message.as_str()))
    })
    .expect("unsupported request body media type builds")
}

fn new_request_stmt(method: &str, has_query: bool, has_body: bool) -> CodeBlock {
    let query_body_stmt =
        format!("req, err := a.client.NewRequest(ctx, {method:?}, path, query, bodyReader)");
    let query_no_body_stmt =
        format!("req, err := a.client.NewRequest(ctx, {method:?}, path, query, nil)");
    let no_query_body_stmt =
        format!("req, err := a.client.NewRequest(ctx, {method:?}, path, nil, bodyReader)");
    let no_query_no_body_stmt =
        format!("req, err := a.client.NewRequest(ctx, {method:?}, path, nil, nil)");
    sigil_quote!(GoLang {
        $if(has_query && has_body) {
            $L(query_body_stmt.as_str())
        } $else_if(has_query) {
            $L(query_no_body_stmt.as_str())
        } $else_if(has_body) {
            $L(no_query_body_stmt.as_str())
        } $else {
            $L(no_query_no_body_stmt.as_str())
        }
    })
    .expect("new request statement builds")
}

fn return_nil_err() -> CodeBlock {
    sigil_quote!(GoLang {
        return nil, err
    })
    .expect("return nil err builds")
}

fn set_header(name_expr: &str, value_expr: &str) -> CodeBlock {
    let stmt = format!("req.Header.Set({name_expr}, {value_expr})");
    sigil_quote!(GoLang {
        $L(stmt.as_str())
    })
    .expect("set header builds")
}

fn set_header_guarded(
    always_set: bool,
    var_name: &str,
    name_expr: &str,
    value_expr: &str,
) -> CodeBlock {
    let stmt = format!("req.Header.Set({name_expr}, {value_expr})");
    let guard = format!("{var_name} != nil");
    sigil_quote!(GoLang {
        $if(always_set) {
            $L(stmt.as_str())
        } $else {
            if $L(guard.as_str()) {
                $L(stmt.as_str())
            }
        }
    })
    .expect("guarded header set builds")
}

fn do_request() -> CodeBlock {
    sigil_quote!(GoLang {
        httpResp, err := a.client.Do(req)
    })
    .expect("do request builds")
}

fn defer_body_close() -> CodeBlock {
    sigil_quote!(GoLang {
        defer httpResp.Body.Close()
    })
    .expect("defer body close builds")
}

fn response_init(response_type: &str) -> CodeBlock {
    let stmt =
        format!("resp := &{response_type}{{StatusCode: httpResp.StatusCode, Raw: httpResp}}");
    sigil_quote!(GoLang {
        $L(stmt.as_str())
    })
    .expect("response init builds")
}

fn return_status_api_error() -> CodeBlock {
    let stmt =
        "return resp, &runtime.APIError{StatusCode: httpResp.StatusCode, Status: httpResp.Status}";
    sigil_quote!(GoLang {
        $L(stmt)
    })
    .expect("return status API error builds")
}

fn error_body_read() -> CodeBlock {
    sigil_quote!(GoLang {
        body, _ := io.ReadAll(httpResp.Body)
    })
    .expect("error body read builds")
}

fn return_body_api_error() -> CodeBlock {
    let stmt = "return nil, &runtime.APIError{StatusCode: httpResp.StatusCode, Status: httpResp.Status, Body: body}";
    sigil_quote!(GoLang {
        $L(stmt)
    })
    .expect("return body API error builds")
}

fn return_resp_nil() -> CodeBlock {
    sigil_quote!(GoLang {
        return resp, nil
    })
    .expect("return response builds")
}

fn emit_multipart_body(
    cb: &mut sigil_stitch::code_block::CodeBlockBuilder,
    body: &BodyBinding,
    parts: &[MultipartPart],
) {
    cb.begin_control_flow(&format!("if {} != nil", body.var_name), ());
    cb.add_code(multipart_buffer_init());
    cb.add_code(multipart_writer_init());
    for part in parts {
        let value_expr = format!("{}.{}", body.var_name, part.field_name);
        if part.required {
            emit_required_multipart_part(cb, part, &value_expr);
        } else {
            cb.begin_control_flow(&format!("if {value_expr} != nil"), ());
            cb.add_code(optional_multipart_value(&value_expr));
            emit_required_multipart_part(cb, part, "value");
            cb.end_control_flow();
        }
    }
    cb.begin_control_flow("if err := writer.Close(); err != nil", ());
    cb.add_code(return_close_multipart_writer_error());
    cb.end_control_flow();
    cb.add_code(multipart_body_reader_assign());
    cb.add_code(multipart_content_type_assign());
    cb.end_control_flow();
}

fn multipart_buffer_init() -> CodeBlock {
    let stmt = "buf := &bytes.Buffer{}";
    sigil_quote!(GoLang {
        $L(stmt)
    })
    .expect("multipart buffer init builds")
}

fn multipart_writer_init() -> CodeBlock {
    sigil_quote!(GoLang {
        writer := multipart.NewWriter(buf)
    })
    .expect("multipart writer init builds")
}

fn optional_multipart_value(value_expr: &str) -> CodeBlock {
    let stmt = format!("value := *{value_expr}");
    sigil_quote!(GoLang {
        $L(stmt.as_str())
    })
    .expect("optional multipart value builds")
}

fn return_close_multipart_writer_error() -> CodeBlock {
    let stmt = "return nil, fmt.Errorf(\"close multipart writer: %w\", err)";
    sigil_quote!(GoLang {
        $L(stmt)
    })
    .expect("close multipart writer error builds")
}

fn multipart_body_reader_assign() -> CodeBlock {
    sigil_quote!(GoLang {
        bodyReader = buf
    })
    .expect("multipart body reader assign builds")
}

fn multipart_content_type_assign() -> CodeBlock {
    sigil_quote!(GoLang {
        multipartContentType = writer.FormDataContentType()
    })
    .expect("multipart content type assign builds")
}

fn emit_required_multipart_part(
    cb: &mut sigil_stitch::code_block::CodeBlockBuilder,
    part: &MultipartPart,
    value_expr: &str,
) {
    cb.add("{", ());
    cb.add_line();
    cb.add_code(multipart_part_header_init());
    if part.is_binary {
        cb.add_code(multipart_binary_disposition(
            value_expr,
            &go_string_literal(&part.wire_name),
        ));
        cb.add_code(multipart_part_header_set(
            &go_string_literal("Content-Disposition"),
            "disposition",
        ));
    } else {
        let disposition = format!("form-data; name={}", go_string_literal(&part.wire_name));
        cb.add_code(multipart_part_header_set(
            &go_string_literal("Content-Disposition"),
            &go_string_literal(&disposition),
        ));
    }
    cb.add_code(multipart_part_header_set(
        &go_string_literal("Content-Type"),
        &go_string_literal(&part.content_type),
    ));
    cb.add_code(multipart_part_writer_create());
    cb.begin_control_flow("if err != nil", ());
    cb.add_code(return_create_multipart_part_error());
    cb.end_control_flow();
    if part.is_binary {
        cb.begin_control_flow(
            &format!("if _, err := partWriter.Write({value_expr}.Data); err != nil"),
            (),
        );
        cb.add_code(return_write_multipart_file_error());
        cb.end_control_flow();
    } else if part.value_encoding == MultipartValueEncoding::Json {
        cb.add_code(multipart_json_value(value_expr));
        cb.begin_control_flow("if err != nil", ());
        cb.add_code(return_marshal_multipart_field_error());
        cb.end_control_flow();
        cb.begin_control_flow("if _, err := partWriter.Write(partValue); err != nil", ());
        cb.add_code(return_write_multipart_field_error());
        cb.end_control_flow();
    } else if part.value_encoding == MultipartValueEncoding::Unsupported {
        cb.add_code(return_unsupported_multipart_part_error());
    } else {
        cb.begin_control_flow(
            &format!(
                "if _, err := io.WriteString(partWriter, {}); err != nil",
                render_value_as_string(value_expr, &part.type_expr)
            ),
            (),
        );
        cb.add_code(return_write_multipart_field_error());
        cb.end_control_flow();
    }
    cb.add("}", ());
    cb.add_line();
}

fn multipart_part_header_init() -> CodeBlock {
    let stmt = "partHeader := textproto.MIMEHeader{}";
    sigil_quote!(GoLang {
        $L(stmt)
    })
    .expect("multipart part header init builds")
}

fn multipart_binary_disposition(value_expr: &str, wire_name: &str) -> CodeBlock {
    let stmt = format!(
        "disposition := mime.FormatMediaType(\"form-data\", map[string]string{{\"name\": {wire_name}, \"filename\": {value_expr}.FilenameOrDefault({wire_name})}})"
    );
    sigil_quote!(GoLang {
        $L(stmt.as_str())
    })
    .expect("multipart binary disposition builds")
}

fn multipart_part_header_set(name_expr: &str, value_expr: &str) -> CodeBlock {
    let stmt = format!("partHeader.Set({name_expr}, {value_expr})");
    sigil_quote!(GoLang {
        $L(stmt.as_str())
    })
    .expect("multipart part header set builds")
}

fn multipart_part_writer_create() -> CodeBlock {
    sigil_quote!(GoLang {
        partWriter, err := writer.CreatePart(partHeader)
    })
    .expect("multipart part writer create builds")
}

fn return_create_multipart_part_error() -> CodeBlock {
    sigil_quote!(GoLang {
        return nil, fmt.Errorf("create multipart part: %w", err)
    })
    .expect("create multipart part error builds")
}

fn return_write_multipart_file_error() -> CodeBlock {
    sigil_quote!(GoLang {
        return nil, fmt.Errorf("write multipart file: %w", err)
    })
    .expect("write multipart file error builds")
}

fn multipart_json_value(value_expr: &str) -> CodeBlock {
    let stmt = format!("partValue, err := json.Marshal({value_expr})");
    sigil_quote!(GoLang {
        $L(stmt.as_str())
    })
    .expect("multipart JSON value builds")
}

fn return_marshal_multipart_field_error() -> CodeBlock {
    sigil_quote!(GoLang {
        return nil, fmt.Errorf("marshal multipart field: %w", err)
    })
    .expect("marshal multipart field error builds")
}

fn return_write_multipart_field_error() -> CodeBlock {
    sigil_quote!(GoLang {
        return nil, fmt.Errorf("write multipart field: %w", err)
    })
    .expect("write multipart field error builds")
}

fn return_unsupported_multipart_part_error() -> CodeBlock {
    sigil_quote!(GoLang {
        return nil, fmt.Errorf("unsupported multipart part content type")
    })
    .expect("unsupported multipart part error builds")
}

fn emit_decode_into(field: &str, go_ty: &str, decoding: ResponseDecoding) -> CodeBlock {
    let (elem_ty, assignment) = if go_ty.starts_with('[') || go_ty.starts_with("map[") {
        (go_ty.to_string(), format!("resp.{field} = payload"))
    } else {
        (
            go_ty.trim_start_matches('*').to_string(),
            format!("resp.{field} = &payload"),
        )
    };
    match decoding {
        ResponseDecoding::Json => sigil_quote!(GoLang {
            $>
            var $L("payload @{elem_ty}")
            if err := json.NewDecoder(httpResp.Body).Decode(&payload); err != nil {
                return nil, fmt.Errorf("decode response: %w", err)
            }
            $L(assignment)
            $<
        })
        .expect("decode JSON body builds"),
        ResponseDecoding::Text => sigil_quote!(GoLang {
            $>
            bodyBytes, err := io.ReadAll(httpResp.Body)
            if err != nil {
                return nil, fmt.Errorf("read response: %w", err)
            }
            payload := string(bodyBytes)
            $L(assignment)
            $<
        })
        .expect("decode text body builds"),
        ResponseDecoding::Bytes => sigil_quote!(GoLang {
            $>
            payload, err := io.ReadAll(httpResp.Body)
            if err != nil {
                return nil, fmt.Errorf("read response: %w", err)
            }
            $L(assignment)
            $<
        })
        .expect("decode bytes body builds"),
    }
}

fn deref_if_pointer(var: &str, is_pointer: bool) -> String {
    if is_pointer {
        format!("*{var}")
    } else {
        var.to_string()
    }
}

// ---------------------------------------------------------------------------
// TypeName builder for response struct fields
// ---------------------------------------------------------------------------

fn pointerize_type_name(go_ty: &str) -> TypeName {
    if go_ty.starts_with('[') || go_ty.starts_with('*') || go_ty.starts_with("map[") {
        TypeName::raw(go_ty)
    } else {
        TypeName::pointer(TypeName::raw(go_ty))
    }
}

// ---------------------------------------------------------------------------
// Response payload fields
// ---------------------------------------------------------------------------

fn response_field_name(status: &str) -> String {
    if status == "default" {
        "Default".to_string()
    } else if let Ok(code) = status.parse::<u16>() {
        format!("Status{code}")
    } else {
        format!("Status{}", status.to_uppercase())
    }
}

fn wildcard_range(status: &str) -> (u16, u16) {
    match status.to_uppercase().as_str() {
        "1XX" => (100, 200),
        "2XX" => (200, 300),
        "3XX" => (300, 400),
        "4XX" => (400, 500),
        "5XX" => (500, 600),
        _ => (0, 1000),
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
                field_name: go_field_name(&part.wire_name),
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

fn go_string_literal(value: &str) -> String {
    format!("{value:?}")
}

// ---------------------------------------------------------------------------
// Param -> Go identifier, value -> Go string expression
// ---------------------------------------------------------------------------

fn go_ident(name: &str) -> String {
    let camel = name.to_lower_camel_case();
    if camel.is_empty() {
        return "arg".to_string();
    }
    match camel.as_str() {
        "type" | "func" | "var" | "const" | "map" | "range" | "select" | "case" | "default"
        | "chan" | "go" | "if" | "else" | "for" | "return" | "break" | "continue" | "switch"
        | "interface" | "struct" | "package" | "import" | "fallthrough" | "goto" | "defer" => {
            format!("{camel}_")
        }
        _ => camel,
    }
}

fn go_field_name(name: &str) -> String {
    name.to_pascal_case()
}

fn render_value_as_string(value_expr: &str, t: &IrTypeExpr) -> String {
    match t {
        IrTypeExpr::Primitive(
            IrPrimitive::String
            | IrPrimitive::Date
            | IrPrimitive::DateTime
            | IrPrimitive::Uuid
            | IrPrimitive::StringWithFormat(_),
        )
        | IrTypeExpr::StringLiteral(_)
        | IrTypeExpr::StringEnum(_) => value_expr.to_string(),
        IrTypeExpr::Primitive(IrPrimitive::Boolean) => {
            format!("strconv.FormatBool({value_expr})")
        }
        IrTypeExpr::Primitive(IrPrimitive::Integer)
        | IrTypeExpr::Primitive(IrPrimitive::IntegerWithFormat(_)) => {
            format!("strconv.FormatInt(int64({value_expr}), 10)")
        }
        IrTypeExpr::Primitive(IrPrimitive::Number)
        | IrTypeExpr::Primitive(IrPrimitive::NumberWithFormat(_)) => {
            format!("strconv.FormatFloat(float64({value_expr}), 'f', -1, 64)")
        }
        IrTypeExpr::Nullable(inner) => render_value_as_string(value_expr, inner),
        IrTypeExpr::Named(_) => {
            format!("string({value_expr})")
        }
        IrTypeExpr::Array(inner) => {
            if is_stringish_primitive(inner) {
                format!("strings.Join({value_expr}, \",\")")
            } else {
                let (item_expr, item_type) =
                    if let IrTypeExpr::Nullable(real_inner) = inner.as_ref() {
                        ("*v", real_inner.as_ref())
                    } else {
                        ("v", inner.as_ref())
                    };
                let item_fmt = render_value_as_string(item_expr, item_type);
                format!(
                    "strings.Join(func() []string {{ parts := make([]string, len({value_expr})); for i, v := range {value_expr} {{ parts[i] = {item_fmt} }}; return parts }}(), \",\")"
                )
            }
        }
        _ => format!("fmt.Sprintf(\"%%v\", {value_expr})"),
    }
}

// ---------------------------------------------------------------------------
// Type-expr -> Go type
// ---------------------------------------------------------------------------

fn go_type_str(expr: &IrTypeExpr) -> String {
    match expr {
        IrTypeExpr::Named(name) => format!("models.{}", name.to_pascal_case()),
        IrTypeExpr::Primitive(p) => go_primitive(p).to_string(),
        IrTypeExpr::Array(inner) => format!("[]{}", go_type_str(inner)),
        IrTypeExpr::Map(inner) => format!("map[string]{}", go_type_str(inner)),
        IrTypeExpr::Nullable(inner) => format!("*{}", go_type_str(inner)),
        IrTypeExpr::StringLiteral(_) | IrTypeExpr::StringEnum(_) => "string".to_string(),
        IrTypeExpr::Union(_) => "any".to_string(),
        IrTypeExpr::Any => "any".to_string(),
    }
}

fn go_primitive(p: &IrPrimitive) -> &'static str {
    match p {
        IrPrimitive::String
        | IrPrimitive::Date
        | IrPrimitive::DateTime
        | IrPrimitive::Uuid
        | IrPrimitive::StringWithFormat(_) => "string",
        IrPrimitive::Binary => "[]byte",
        IrPrimitive::Integer => "int",
        IrPrimitive::IntegerWithFormat(format) => match format.as_str() {
            "int32" => "int32",
            "int64" => "int64",
            _ => "int",
        },
        IrPrimitive::Number => "float64",
        IrPrimitive::NumberWithFormat(format) => match format.as_str() {
            "float" => "float32",
            _ => "float64",
        },
        IrPrimitive::Boolean => "bool",
    }
}

// ---------------------------------------------------------------------------
// Misc
// ---------------------------------------------------------------------------

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
