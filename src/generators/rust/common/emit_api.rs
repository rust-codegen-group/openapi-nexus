//! API emission for IR operations (Rust APIs).
//!
//! Groups operations by tag, emits one `apis/<tag>.rs` per tag group. Each file
//! declares a `{Tag}Api` struct holding a `&runtime::Client` and exposes one
//! method per operation.
//!
//! Backend-specific method bodies are injected via a closure, keeping this module
//! agnostic to the HTTP library (reqwest, ureq, aioduct, etc.).

use std::collections::{BTreeMap, HashSet};

use crate::codegen::traits::file_writer::FileInfo;
use crate::generators::multipart::multipart_parts_for_request_body;
pub use crate::generators::multipart::{MultipartPart, MultipartValueEncoding};
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
use heck::{ToPascalCase, ToSnakeCase};
use sigil_stitch::code_block::{CodeBlock, CodeBlockBuilder};
use sigil_stitch::lang::rust::Rust;
use sigil_stitch::prelude::sigil_quote;
use sigil_stitch::spec::annotation_spec::AnnotationSpec;
use sigil_stitch::spec::field_spec::FieldSpec;
use sigil_stitch::spec::file_spec::FileSpec;
use sigil_stitch::spec::fun_spec::FunSpec;
use sigil_stitch::spec::import_spec::ImportSpec;
use sigil_stitch::spec::modifiers::{DeclarationContext, TypeKind, Visibility};
use sigil_stitch::spec::parameter_spec::ParameterSpec;
use sigil_stitch::spec::type_spec::TypeSpec;
use sigil_stitch::type_name::TypeName;

use super::config::ExtraDeriveConfig;
use super::emit_models::rust_type_str_qualified;

// ---------------------------------------------------------------------------
// Backend configuration
// ---------------------------------------------------------------------------

/// Captures the differences between Rust HTTP backends.
pub struct RustBackendConfig {
    /// Whether methods are async (reqwest, aioduct) or sync (ureq).
    pub is_async: bool,
    /// Module containing the native header-map type retained on generated responses.
    pub response_headers_module: &'static str,
    /// Native header-map type retained on generated responses.
    pub response_headers_name: &'static str,
    /// Extra generic parameters on the Api struct, e.g., `"R: aioduct::RuntimePoll"`.
    /// `None` for reqwest and ureq.
    pub struct_generics: Option<String>,
    /// Extra generic args for the client field type, e.g., `"<R>"`.
    /// `None` for reqwest and ureq.
    pub client_type_args: Option<String>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Generate every API file from the IR.
pub fn generate_api_files(
    ir: &IrSpec,
    header: &str,
    config: &RustBackendConfig,
    response_extra_derives: Option<&ExtraDeriveConfig>,
    request_inputs: &RequestInputPlan,
    body_emitter: &dyn Fn(&OpPlan<'_>) -> CodeBlock,
) -> Vec<FileInfo> {
    let by_tag = group_by_tag(&ir.operations);
    let mut files = Vec::with_capacity(by_tag.len());
    let mut mod_entries = Vec::new();

    for (tag, ops) in &by_tag {
        let stem = tag.to_snake_case();
        let filename = format!("{stem}.rs");
        mod_entries.push(stem);
        let body = emit_api_file(
            tag,
            ops,
            ir,
            config,
            response_extra_derives,
            request_inputs,
            body_emitter,
        );
        let content = format!("{header}{body}");
        files.push(FileInfo::api(filename, content));
    }

    // mod.rs
    let mut mod_content = String::from(header);
    for entry in &mod_entries {
        mod_content.push_str(&format!("mod {entry};\npub use {entry}::*;\n"));
    }
    files.push(FileInfo::api("mod.rs".to_string(), mod_content));

    files
}

// ---------------------------------------------------------------------------
// Grouping
// ---------------------------------------------------------------------------

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
    config: &RustBackendConfig,
    response_extra_derives: Option<&ExtraDeriveConfig>,
    request_inputs: &RequestInputPlan,
    body_emitter: &dyn Fn(&OpPlan<'_>) -> CodeBlock,
) -> String {
    let struct_name = format!("{}Api", tag.to_pascal_case());
    let plans: Vec<OpPlan> = ops
        .iter()
        .map(|op| plan_operation(op, ir, request_inputs))
        .collect();

    let stem = tag.to_snake_case();
    let mut fsb = FileSpec::builder(&format!("{stem}.rs"));

    // Use imports
    fsb = fsb.add_import(ImportSpec::named("crate::runtime::client", "Client"));
    fsb = fsb.add_import(ImportSpec::named("crate::runtime::error", "ApiError"));
    fsb = fsb.add_import(ImportSpec::named("crate::runtime::error", "Error"));

    // Struct generics (e.g., `<'a, R: aioduct::RuntimePoll>`)
    let (struct_gen, impl_gen, type_args, client_field_args) = match &config.struct_generics {
        Some(g) => {
            let client_args = config.client_type_args.as_deref().unwrap_or("");
            let param_names = g
                .split(',')
                .map(|param| param.split(':').next().unwrap_or(param).trim())
                .collect::<Vec<_>>()
                .join(", ");
            (
                format!("<'a, {g}>"),
                format!("<'a, {g}>"),
                format!("<'a, {param_names}>"),
                client_args.to_string(),
            )
        }
        None => (
            "<'a>".to_string(),
            "<'a>".to_string(),
            "<'a>".to_string(),
            String::new(),
        ),
    };

    // Build struct + impl as a CodeBlock (lifetimes/generics don't fit TypeSpec)
    let mut body = CodeBlock::builder();

    // Struct declaration via sigil_quote
    let doc_struct = format!("/// API operations under the \"{tag}\" tag.");
    let generics = struct_gen.as_str();
    let client_type_suffix = client_field_args.as_str();
    let client_field = format!("client: &'a Client{client_type_suffix},");
    body.add_code(
        sigil_quote!(RustLang {
            $L(doc_struct)
            pub struct $N(struct_name.as_str())$L(generics) {
                $L(client_field)
            }
        })
        .expect("struct sigil_quote builds"),
    );
    body.add_line();

    // Impl block (kept open for method injection)
    let impl_header = format!("impl{impl_gen} {struct_name}{type_args}");
    body.add(&impl_header, ());
    body.begin_control_flow("", ());

    // Constructor via sigil_quote
    let doc_ctor = format!("/// Create a new `{struct_name}` bound to the given client.");
    body.add_code(
        sigil_quote!(RustLang {
            $L(doc_ctor)
            pub fn $L("new(client: &'a Client@{client_type_suffix}) -> Self") {
                Self {
                    client,
                }
            }
        })
        .expect("constructor sigil_quote builds"),
    );

    // Methods
    for plan in &plans {
        body.add_line();
        body.add_code(emit_operation(plan, config, body_emitter));
    }

    body.end_control_flow(); // close impl

    fsb = fsb.add_code(body.build().expect("body builds"));

    // Response structs -- add as TypeSpec members
    for plan in &plans {
        let response_headers_type =
            TypeName::qualified(config.response_headers_module, config.response_headers_name);
        fsb = fsb.add_type(emit_response_struct(
            plan,
            &response_headers_type,
            response_extra_derives,
        ));
        fsb = fsb.add_code(emit_error_enum(plan, &response_headers_type));
    }

    let file = fsb.build().expect("FileSpec builds");
    file.render(100).expect("FileSpec renders")
}

// ---------------------------------------------------------------------------
// Operation planning (public for backend use)
// ---------------------------------------------------------------------------

pub struct OpPlan<'a> {
    pub op: &'a IrOperation,
    pub method_name: String,
    pub response_type: String,
    pub error_type: String,
    pub path_params: Vec<ParamBinding<'a>>,
    pub query_params: Vec<ParamBinding<'a>>,
    pub header_params: Vec<ParamBinding<'a>>,
    pub body: Option<BodyBinding>,
    pub typed_responses: Vec<TypedResponse>,
    pub error_responses: Vec<ErrorResponse>,
    pub success_headers: Vec<ResponseHeaderPlan>,
    pub error_headers: Vec<ResponseHeaderPlan>,
}

pub struct ParamBinding<'a> {
    pub param: &'a IrParameter,
    pub var_name: String,
    pub rust_type: String,
    pub is_optional: bool,
}

pub struct BodyBinding {
    pub var_name: String,
    pub rust_type: String,
    pub media_type: String,
    pub required: bool,
    pub encoding: BodyEncoding,
    pub multipart_supported: bool,
    pub multipart_parts: Vec<MultipartPart>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyEncoding {
    Json,
    FormUrlEncoded,
    Multipart,
    Xml,
    TextPlain,
    OctetStream,
    Other(String),
}

pub struct TypedResponse {
    pub status: String,
    pub field_name: String,
    pub rust_type: String,
    pub decoding: ResponseDecoding,
}

pub struct ErrorResponse {
    pub status: String,
    pub variant_name: String,
    pub rust_type: String,
    pub decoding: Option<ResponseDecoding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponseDecoding {
    Json,
    Xml,
    TextPlain,
    OctetStream,
    Other(String),
}

pub fn plan_operation<'a>(
    op: &'a IrOperation,
    ir: &'a IrSpec,
    request_inputs: &RequestInputPlan,
) -> OpPlan<'a> {
    let op_id = sanitize_operation_id(&op.operation_id, &op.method, &op.path);
    let method_name = op_id.to_snake_case();
    let response_type = format!("{}Response", op_id.to_pascal_case());
    let error_type = format!("{}Error", op_id.to_pascal_case());

    let mut used_names: HashSet<String> = HashSet::new();
    used_names.insert("self".to_string());

    let mut path_params = Vec::new();
    let mut query_params = Vec::new();
    let mut header_params = Vec::new();
    for p in &op.parameters {
        let var_name = unique_name(&p.name.to_snake_case(), &mut used_names);
        let (rust_type, is_optional) = param_rust_type(p, ir);
        let binding = ParamBinding {
            param: p,
            var_name,
            rust_type,
            is_optional,
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
        .and_then(|b| plan_body(op, b, &mut used_names, ir, request_inputs));

    let mut typed_responses: Vec<TypedResponse> = op
        .responses
        .iter()
        .filter(|r| is_success_status(&r.status))
        .filter_map(|r| plan_response(r, ir))
        .collect();
    typed_responses.sort_by_key(|r| response_match_rank(&r.status));
    let error_responses = op
        .responses
        .iter()
        .filter(|r| !is_success_status(&r.status))
        .map(|r| plan_error_response(r, ir))
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

pub fn plan_body(
    op: &IrOperation,
    b: &IrRequestBody,
    used_names: &mut HashSet<String>,
    ir: &IrSpec,
    request_inputs: &RequestInputPlan,
) -> Option<BodyBinding> {
    let (media_type, t) = pick_body_content(b)?;
    let encoding = body_encoding(&media_type);
    let rust_type = match encoding {
        BodyEncoding::OctetStream => "Vec<u8>".to_string(),
        BodyEncoding::TextPlain => "String".to_string(),
        BodyEncoding::Multipart => request_input_for_operation(request_inputs, op, &media_type)
            .map(|input| format!("crate::models::{}", input.name.to_pascal_case()))
            .unwrap_or_else(|| rust_type_str_qualified(&t, ir)),
        _ => rust_type_str_qualified(&t, ir),
    };
    let multipart_parts = if encoding == BodyEncoding::Multipart {
        multipart_parts_for_request_body(b, &media_type, ir).unwrap_or_default()
    } else {
        Vec::new()
    };
    let multipart_supported = encoding != BodyEncoding::Multipart
        || multipart_parts_for_request_body(b, &media_type, ir).is_some();
    let var_name = unique_name("body", used_names);
    Some(BodyBinding {
        var_name,
        rust_type,
        media_type,
        required: b.required,
        encoding,
        multipart_supported,
        multipart_parts,
    })
}

pub fn plan_response(r: &IrResponse, ir: &IrSpec) -> Option<TypedResponse> {
    let (media_type, t) = pick_response_content(r)?;
    let decoding = response_decoding(&media_type);
    let rust_type = match decoding {
        ResponseDecoding::OctetStream => "Vec<u8>".to_string(),
        ResponseDecoding::TextPlain => "String".to_string(),
        _ => rust_type_str_qualified(&t, ir),
    };
    Some(TypedResponse {
        status: r.status.clone(),
        field_name: response_field_name(&r.status),
        rust_type,
        decoding,
    })
}

pub fn plan_error_response(r: &IrResponse, ir: &IrSpec) -> ErrorResponse {
    let (rust_type, decoding) = match pick_response_content(r) {
        Some((media_type, t)) => {
            let decoding = response_decoding(&media_type);
            let rust_type = match decoding {
                ResponseDecoding::OctetStream => "Vec<u8>".to_string(),
                ResponseDecoding::TextPlain => "String".to_string(),
                _ => rust_type_str_qualified(&t, ir),
            };
            (rust_type, Some(decoding))
        }
        None => ("()".to_string(), None),
    };
    ErrorResponse {
        status: r.status.clone(),
        variant_name: response_variant_name(&r.status),
        rust_type,
        decoding,
    }
}

pub fn is_success_status(status: &str) -> bool {
    status
        .parse::<u16>()
        .is_ok_and(|code| (200..300).contains(&code))
        || status.eq_ignore_ascii_case("2XX")
}

pub fn param_rust_type(p: &IrParameter, ir: &IrSpec) -> (String, bool) {
    let base = rust_type_str_qualified(&p.type_expr, ir);
    if p.required {
        (base, false)
    } else if matches!(p.type_expr, IrTypeExpr::Nullable(_)) {
        // Already wrapped in Option by rust_type_str_qualified → avoid double-wrapping
        (base, true)
    } else {
        (format!("Option<{base}>"), true)
    }
}

pub fn unique_name(desired: &str, used: &mut HashSet<String>) -> String {
    if used.insert(desired.to_string()) {
        return desired.to_string();
    }
    for i in 2..=u32::MAX {
        let candidate = format!("{desired}_{i}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!("name collision space exhausted")
}

// ---------------------------------------------------------------------------
// Per-operation emission
// ---------------------------------------------------------------------------

fn emit_operation(
    plan: &OpPlan<'_>,
    config: &RustBackendConfig,
    body_emitter: &dyn Fn(&OpPlan<'_>) -> CodeBlock,
) -> CodeBlock {
    let OpPlan {
        op,
        method_name,
        response_type,
        error_type,
        ..
    } = plan;

    let mut b = CodeBlock::builder();

    // Doc comment
    if let Some(summary) = &op.summary {
        for line in summary.lines() {
            if line.is_empty() {
                b.add("///\n", ());
            } else {
                b.add(&format!("/// {line}\n"), ());
            }
        }
    } else {
        b.add(
            &format!("/// {} {}\n", op.method.to_uppercase(), op.path),
            (),
        );
    }
    if let Some(desc) = &op.description {
        b.add("///\n", ());
        for line in desc.lines() {
            if line.is_empty() {
                b.add("///\n", ());
            } else {
                b.add(&format!("/// {line}\n"), ());
            }
        }
    }

    // Method signature
    let mut params = Vec::new();
    params.push("&self".to_string());
    for p in plan
        .path_params
        .iter()
        .chain(&plan.query_params)
        .chain(&plan.header_params)
    {
        let ty = if is_copy_type(&p.rust_type) {
            p.rust_type.clone()
        } else if p.rust_type == "String" {
            "&str".to_string()
        } else if let Some(inner) = p
            .rust_type
            .strip_prefix("Vec<")
            .and_then(|s| s.strip_suffix('>'))
        {
            format!("&[{inner}]")
        } else {
            format!("&{}", p.rust_type)
        };
        params.push(format!("{}: {ty}", p.var_name));
    }
    if let Some(body) = &plan.body {
        let ty = if body.required {
            format!("&{}", body.rust_type)
        } else {
            format!("Option<&{}>", body.rust_type)
        };
        params.push(format!("{}: {ty}", body.var_name));
    }

    let async_kw = if config.is_async { "async " } else { "" };
    b.add(
        &format!(
            "pub {async_kw}fn {method_name}(\n    {},\n) -> Result<{response_type}, {error_type}>",
            params.join(",\n    "),
        ),
        (),
    );
    b.begin_control_flow("", ());

    // Method body from backend
    b.add_code(body_emitter(plan));

    b.end_control_flow();
    b.build().unwrap()
}

pub fn emit_response_struct(
    plan: &OpPlan<'_>,
    response_headers_type: &TypeName,
    extra: Option<&ExtraDeriveConfig>,
) -> TypeSpec {
    let mut tb = TypeSpec::builder(&plan.response_type, TypeKind::Struct);
    tb = tb.visibility(Visibility::Public);
    tb = tb.doc(&format!("Response from `{}`.", plan.method_name));

    let mut ann = AnnotationSpec::new("derive").args(["Debug"]);
    if let Some(cfg) = extra {
        ann = ann.args(cfg.derives.iter().map(|s| s.as_str()));
    }
    tb = tb.annotate(ann);

    // status_code field
    {
        let fb = FieldSpec::builder("status_code", TypeName::primitive("u16"));
        let fb = fb.visibility(Visibility::Public);
        tb = tb.add_field(fb.build().expect("FieldSpec builds"));
    }

    // native response headers
    {
        let fb = FieldSpec::builder("headers", response_headers_type.clone());
        let fb = fb.visibility(Visibility::Public);
        tb = tb.add_field(fb.build().expect("FieldSpec builds"));
    }

    // typed response fields
    let mut seen: HashSet<String> = HashSet::new();
    for tr in &plan.typed_responses {
        if !seen.insert(tr.field_name.clone()) {
            continue;
        }
        let fb = FieldSpec::builder(
            &tr.field_name,
            TypeName::raw(&format!("Option<{}>", tr.rust_type)),
        );
        let fb = fb.visibility(Visibility::Public);
        tb = tb.add_field(fb.build().expect("FieldSpec builds"));
    }

    let method_names = rust_header_accessor_names(&plan.success_headers);
    for (header, method_name) in plan.success_headers.iter().zip(method_names) {
        tb = tb.add_method(build_rust_header_accessor(
            header,
            &method_name,
            "self.headers",
        ));
    }

    tb.build().expect("TypeSpec builds")
}

pub fn emit_error_enum(plan: &OpPlan<'_>, response_headers_type: &TypeName) -> CodeBlock {
    let mut cb = CodeBlock::builder();
    let mut variants = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for er in &plan.error_responses {
        let variant = unique_variant_name(&er.variant_name, &mut seen);
        variants.push((variant, er.rust_type.clone()));
    }
    let unexpected_variant = unique_variant_name("Unexpected", &mut seen);
    let transport_variant = unique_variant_name("Transport", &mut seen);

    cb.add(&format!("/// Error from `{}`.\n", plan.method_name), ());
    cb.add("#[derive(Debug)]\n", ());
    cb.add(&format!("pub enum {} {{\n", plan.error_type), ());
    for (variant, rust_type) in &variants {
        cb.add(&format!("    {variant}(ApiError<{rust_type}>),\n"), ());
    }
    cb.add(
        &format!("    {unexpected_variant}(ApiError<Vec<u8>>),\n"),
        (),
    );
    cb.add(&format!("    {transport_variant}(Error),\n"), ());
    cb.add("}\n\n", ());

    cb.add(
        &format!("impl From<Error> for {} {{\n", plan.error_type),
        (),
    );
    cb.add("    fn from(error: Error) -> Self {\n", ());
    cb.add(&format!("        Self::{transport_variant}(error)\n"), ());
    cb.add("    }\n", ());
    cb.add("}\n\n", ());

    let operation_id = if plan.op.operation_id.is_empty() {
        plan.method_name.as_str()
    } else {
        plan.op.operation_id.as_str()
    };
    cb.add_code(emit_api_call_error_conversion(
        &plan.error_type,
        operation_id,
        &variants,
        &unexpected_variant,
        &transport_variant,
    ));
    cb.add_line();

    cb.add_code(emit_rust_error_header_impl(
        plan,
        response_headers_type,
        &variants,
        &unexpected_variant,
        &transport_variant,
    ));

    cb.add(
        &format!("impl std::fmt::Display for {} {{\n", plan.error_type),
        (),
    );
    cb.add(
        "    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {\n",
        (),
    );
    cb.add("        match self {\n", ());
    for (variant, _) in &variants {
        cb.add(
            &format!("            Self::{variant}(err) => write!(f, \"HTTP error {{}}\", err.status_code()),\n"),
            (),
        );
    }
    cb.add(&format!(
        "            Self::{unexpected_variant}(err) => write!(f, \"unexpected HTTP error {{}}\", err.status_code()),\n"
    ), ());
    cb.add(
        &format!("            Self::{transport_variant}(err) => std::fmt::Display::fmt(err, f),\n"),
        (),
    );
    cb.add("        }\n", ());
    cb.add("    }\n", ());
    cb.add("}\n\n", ());

    cb.add(
        &format!("impl std::error::Error for {} {{\n", plan.error_type),
        (),
    );
    cb.add(
        "    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {\n",
        (),
    );
    cb.add("        match self {\n", ());
    cb.add(
        &format!("            Self::{transport_variant}(err) => Some(err),\n"),
        (),
    );
    cb.add("            _ => None,\n", ());
    cb.add("        }\n", ());
    cb.add("    }\n", ());
    cb.add("}\n", ());

    cb.build().expect("error enum builds")
}

fn emit_api_call_error_conversion(
    operation_error_type: &str,
    operation_id: &str,
    variants: &[(String, String)],
    unexpected_variant: &str,
    transport_variant: &str,
) -> CodeBlock {
    let operation_error_type = TypeName::primitive(operation_error_type);
    let api_call_error_type = TypeName::importable("crate::runtime::error", "ApiCallError");

    sigil_quote!(RustLang {
        impl From<$T(operation_error_type.clone())> for $T(api_call_error_type) {
            fn from(error: $T(operation_error_type.clone())) -> Self {
                match error {
                    $for((variant, _) in variants) {
                        $T(operation_error_type.clone())::$N(variant.as_str())(error) => Self::from_api_error($S(operation_id), error),
                    }
                    $T(operation_error_type.clone())::$N(unexpected_variant)(error) => Self::from_api_error($S(operation_id), error),
                    $T(operation_error_type)::$N(transport_variant)(error) => Self::from_runtime_error($S(operation_id), error),
                }
            }
        }
    })
    .expect("Rust API call error conversion builds")
}

fn emit_rust_error_header_impl(
    plan: &OpPlan<'_>,
    response_headers_type: &TypeName,
    variants: &[(String, String)],
    unexpected_variant: &str,
    transport_variant: &str,
) -> CodeBlock {
    if plan.error_headers.is_empty() {
        return sigil_quote!(RustLang {}).expect("empty Rust error header impl builds");
    }

    let response_headers_body = sigil_quote!(RustLang {
        match self {
            $for((variant, _) in variants) {
                Self::$N(variant.as_str())(err) => Some(err.headers()),
            }
            Self::$N(unexpected_variant)(err) => Some(err.headers()),
            Self::$N(transport_variant)(_) => None,
        }
    })
    .expect("Rust response headers body builds");
    let response_headers = FunSpec::builder("response_headers")
        .add_param(ParameterSpec::of("&self", TypeName::primitive("")))
        .returns(vec![TypeName::optional(TypeName::reference(
            response_headers_type.clone(),
        ))])
        .body(response_headers_body)
        .build()
        .expect("Rust response headers method builds");

    let lang = Rust::new();
    let response_headers = response_headers
        .emit(&lang, DeclarationContext::Member)
        .expect("Rust response headers method emits");
    let method_names = rust_header_accessor_names(&plan.error_headers);
    let header_accessors = plan
        .error_headers
        .iter()
        .zip(method_names)
        .map(|(header, method_name)| {
            build_rust_header_accessor(header, &method_name, "self.response_headers()?")
                .emit(&lang, DeclarationContext::Member)
                .expect("Rust error header accessor emits")
        })
        .collect::<Vec<_>>();
    sigil_quote!(RustLang {
        impl $N(plan.error_type.as_str()) {
            $L(response_headers)
            $for(accessor in &header_accessors) {
                $L((*accessor).clone())
            }
        }
    })
    .expect("Rust error header impl builds")
}

fn build_rust_header_accessor(
    header: &ResponseHeaderPlan,
    method_name: &str,
    headers_expr: &str,
) -> FunSpec {
    FunSpec::builder(method_name)
        .visibility(Visibility::Public)
        .add_param(ParameterSpec::of("&self", TypeName::primitive("")))
        .returns(vec![rust_header_return_type(header.value_kind)])
        .body(rust_header_accessor_body(header, headers_expr))
        .build()
        .expect("Rust header accessor builds")
}

fn rust_header_accessor_body(header: &ResponseHeaderPlan, headers_expr: &str) -> CodeBlock {
    let wire_name = header.wire_name.as_str();
    match header.value_kind {
        ResponseHeaderValueKind::String => sigil_quote!(RustLang {
            $L(headers_expr).get($S(wire_name))?.to_str().ok()
        }),
        ResponseHeaderValueKind::Integer | ResponseHeaderValueKind::Boolean => {
            sigil_quote!(RustLang {
                $L(headers_expr).get($S(wire_name))?.to_str().ok()?.parse().ok()
            })
        }
        ResponseHeaderValueKind::Number => sigil_quote!(RustLang {
            $L(headers_expr).get($S(wire_name))?.to_str().ok()?.parse().ok().filter(|value: &f64| value.is_finite())
        }),
    }
    .expect("Rust header accessor body builds")
}

fn rust_header_return_type(kind: ResponseHeaderValueKind) -> TypeName {
    let inner = match kind {
        ResponseHeaderValueKind::String => TypeName::reference(TypeName::primitive("str")),
        ResponseHeaderValueKind::Integer => TypeName::primitive("i64"),
        ResponseHeaderValueKind::Number => TypeName::primitive("f64"),
        ResponseHeaderValueKind::Boolean => TypeName::primitive("bool"),
    };
    TypeName::optional(inner)
}

fn rust_header_accessor_names(headers: &[ResponseHeaderPlan]) -> Vec<String> {
    unique_response_header_accessor_names(headers, |wire_name| {
        let mut base = wire_name.to_snake_case();
        if base.is_empty() || base.starts_with(|character: char| character.is_ascii_digit()) {
            base = format!("header_{base}");
        }
        format!("{base}_header")
    })
}

fn unique_variant_name(desired: &str, used: &mut HashSet<String>) -> String {
    if used.insert(desired.to_string()) {
        return desired.to_string();
    }
    for i in 2..=u32::MAX {
        let candidate = format!("{desired}{i}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!("variant collision space exhausted")
}

// ---------------------------------------------------------------------------
// Helpers (public for backend use)
// ---------------------------------------------------------------------------

pub fn sanitize_operation_id(id: &str, method: &str, path: &str) -> String {
    if !id.is_empty() {
        return id.to_string();
    }
    format!(
        "{}_{}",
        method,
        path.replace('/', "_").replace(['{', '}'], "")
    )
}

pub fn response_field_name(status: &str) -> String {
    match status {
        "200" => "data".to_string(),
        "201" => "created".to_string(),
        "204" => "no_content".to_string(),
        "default" => "error_body".to_string(),
        s if s.ends_with("XX") => {
            let prefix = &s[..s.len() - 2];
            format!("status_{prefix}xx")
        }
        s => format!("status_{s}"),
    }
}

/// Convert an OpenAPI status code string to a Rust match pattern.
pub fn status_match_pattern(status: &str) -> String {
    match status {
        "default" => "_".to_string(),
        s if s.ends_with("XX") => {
            let prefix: u16 = s[..s.len() - 2].parse().unwrap_or(0);
            let lo = prefix * 100;
            let hi = lo + 99;
            format!("{lo}..={hi}")
        }
        s => s.to_string(),
    }
}

pub fn pick_body_type(b: &IrRequestBody) -> Option<IrTypeExpr> {
    pick_body_content(b).map(|(_, t)| t)
}

pub fn pick_response_type(r: &IrResponse) -> Option<IrTypeExpr> {
    pick_response_content(r).map(|(_, t)| t)
}

fn pick_body_content(b: &IrRequestBody) -> Option<(String, IrTypeExpr)> {
    pick_media_type(&b.content, |media_type| {
        media_type_base(media_type) == "application/json"
    })
    .or_else(|| pick_media_type(&b.content, is_json_media_type))
    .or_else(|| {
        pick_media_type(&b.content, |media_type| {
            media_type_base(media_type) == "multipart/form-data"
        })
    })
    .or_else(|| {
        pick_media_type(&b.content, |media_type| {
            media_type_base(media_type) == "application/x-www-form-urlencoded"
        })
    })
    .or_else(|| pick_media_type(&b.content, is_xml_media_type))
    .or_else(|| {
        pick_media_type(&b.content, |media_type| {
            media_type_base(media_type) == "text/plain"
        })
    })
    .or_else(|| {
        pick_media_type(&b.content, |media_type| {
            media_type_base(media_type) == "application/octet-stream"
        })
    })
    .or_else(|| pick_first_content(&b.content))
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

fn body_encoding(media_type: &str) -> BodyEncoding {
    let base = media_type_base(media_type);
    match base.as_str() {
        "application/json" => BodyEncoding::Json,
        "application/x-www-form-urlencoded" => BodyEncoding::FormUrlEncoded,
        "multipart/form-data" => BodyEncoding::Multipart,
        "application/xml" | "text/xml" => BodyEncoding::Xml,
        "text/plain" => BodyEncoding::TextPlain,
        "application/octet-stream" => BodyEncoding::OctetStream,
        _ if is_json_media_type(media_type) => BodyEncoding::Json,
        _ if is_xml_media_type(media_type) => BodyEncoding::Xml,
        _ => BodyEncoding::Other(media_type.to_string()),
    }
}

fn response_decoding(media_type: &str) -> ResponseDecoding {
    let base = media_type_base(media_type);
    match base.as_str() {
        "application/json" => ResponseDecoding::Json,
        "application/xml" | "text/xml" => ResponseDecoding::Xml,
        "text/plain" => ResponseDecoding::TextPlain,
        "application/octet-stream" => ResponseDecoding::OctetStream,
        _ if is_json_media_type(media_type) => ResponseDecoding::Json,
        _ if is_xml_media_type(media_type) => ResponseDecoding::Xml,
        _ => ResponseDecoding::Other(media_type.to_string()),
    }
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

pub fn rust_field_name(wire_name: &str) -> String {
    escape_rust_keyword(&wire_name.to_snake_case())
}

fn escape_rust_keyword(name: &str) -> String {
    const KEYWORDS: &[&str] = &[
        "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
        "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move",
        "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait",
        "true", "type", "union", "unsafe", "use", "where", "while", "yield",
    ];
    if KEYWORDS.contains(&name) {
        format!("r#{name}")
    } else {
        name.to_string()
    }
}

pub fn rust_string_literal(value: &str) -> String {
    format!("{value:?}")
}

pub fn text_field_expr(base: &str, part: &MultipartPart) -> String {
    let field_name = rust_field_name(&part.wire_name);
    match part.value_encoding {
        MultipartValueEncoding::Text => format!("{base}.{field_name}.to_string()"),
        MultipartValueEncoding::Json => {
            format!("serde_json::to_string(&{base}.{field_name}).map_err(Error::Deserialize)?")
        }
        MultipartValueEncoding::Unsupported => {
            unreachable!("unsupported multipart parts are emitted before value expressions")
        }
    }
}

pub fn binary_field_expr(base: &str, part: &MultipartPart) -> String {
    format!("{base}.{}.data.clone()", rust_field_name(&part.wire_name))
}

pub fn optional_text_field_expr(value: &str, part: &MultipartPart) -> String {
    match part.value_encoding {
        MultipartValueEncoding::Text => format!("{value}.to_string()"),
        MultipartValueEncoding::Json => {
            format!("serde_json::to_string({value}).map_err(Error::Deserialize)?")
        }
        MultipartValueEncoding::Unsupported => {
            unreachable!("unsupported multipart parts are emitted before value expressions")
        }
    }
}

pub fn optional_binary_field_expr(value: &str) -> String {
    format!("{value}.data.clone()")
}

pub fn binary_filename_expr(base: &str, part: &MultipartPart) -> String {
    format!(
        "{base}.{}.filename_or_default({}).to_string()",
        rust_field_name(&part.wire_name),
        rust_string_literal(&part.default_filename)
    )
}

pub fn optional_binary_filename_expr(value: &str, part: &MultipartPart) -> String {
    format!(
        "{value}.filename_or_default({}).to_string()",
        rust_string_literal(&part.default_filename)
    )
}

pub fn response_value_expr(tr: &TypedResponse, bytes_var: &str) -> String {
    let owned_bytes_expr = bytes_var.strip_prefix('&').unwrap_or(bytes_var);
    match tr.decoding {
        ResponseDecoding::Json => {
            format!("serde_json::from_slice({bytes_var}).map_err(Error::Deserialize)")
        }
        ResponseDecoding::Xml => {
            format!(
                "serde_xml_rs::from_reader(std::io::Cursor::new({bytes_var})).map_err(Error::Xml)"
            )
        }
        ResponseDecoding::TextPlain => {
            format!("Ok::<String, Error>(String::from_utf8_lossy({bytes_var}).into_owned())")
        }
        ResponseDecoding::OctetStream => {
            format!("Ok::<Vec<u8>, Error>({owned_bytes_expr}.to_vec())")
        }
        ResponseDecoding::Other(_) => {
            format!("serde_json::from_slice({bytes_var}).map_err(Error::Deserialize)")
        }
    }
}

pub fn response_value_expr_from_str(tr: &TypedResponse, body_var: &str) -> String {
    match tr.decoding {
        ResponseDecoding::Json => {
            format!("serde_json::from_str({body_var}).map_err(Error::Deserialize)")
        }
        ResponseDecoding::Xml => {
            format!("serde_xml_rs::from_str({body_var}).map_err(Error::Xml)")
        }
        ResponseDecoding::TextPlain => format!("Ok::<String, Error>({body_var})"),
        ResponseDecoding::OctetStream => {
            format!("Ok::<Vec<u8>, Error>({body_var}.into_bytes())")
        }
        ResponseDecoding::Other(_) => {
            format!("serde_json::from_str({body_var}).map_err(Error::Deserialize)")
        }
    }
}

pub fn response_needs_bytes(typed_responses: &[TypedResponse]) -> bool {
    typed_responses
        .iter()
        .any(|tr| matches!(tr.decoding, ResponseDecoding::OctetStream))
}

pub fn render_to_string(var: &str, type_expr: &IrTypeExpr, _is_optional: bool) -> String {
    match type_expr {
        IrTypeExpr::Array(_) => {
            format!("{var}.iter().map(ToString::to_string).collect::<Vec<_>>().join(\",\")")
        }
        _ => format!("{var}.to_string()"),
    }
}

pub fn is_copy_type(ty: &str) -> bool {
    matches!(
        ty,
        "bool" | "i32" | "i64" | "f32" | "f64" | "u8" | "u16" | "u32" | "u64"
    ) || ty.starts_with("Option<")
        && is_copy_type(
            ty.strip_prefix("Option<")
                .unwrap()
                .strip_suffix('>')
                .unwrap_or(""),
        )
}

// ---------------------------------------------------------------------------
// Shared body-emission helpers (used by all Rust backends)
// ---------------------------------------------------------------------------

/// Emit `let mut result = FooResponse { status_code, field1: None, ... };`
pub fn emit_result_init(
    b: &mut CodeBlockBuilder,
    response_type: &str,
    typed_responses: &[TypedResponse],
) {
    let mut field_names = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for tr in typed_responses {
        if seen.insert(tr.field_name.clone()) {
            field_names.push(tr.field_name.as_str());
        }
    }
    b.add_code(
        sigil_quote!(RustLang {
            let mut result = $N(response_type)$L(" { status_code, headers: response_headers.clone()")$for(field_name in &field_names) { $L(", ")$N(*field_name)$L(": None") }$L(" }");
        })
        .expect("response result initializer builds"),
    );
}

pub fn emit_empty_result_init(b: &mut CodeBlockBuilder, response_type: &str) {
    b.add_code(
        sigil_quote!(RustLang {
            let result = $N(response_type)$L(" { status_code, headers: response_headers.clone() }");
        })
        .expect("empty response result initializer builds"),
    );
}

/// Clone the native response header map before the response body is consumed.
pub fn response_headers_init() -> CodeBlock {
    sigil_quote!(RustLang {
        let response_headers = resp.headers().clone();
    })
    .expect("response headers init builds")
}

/// Emit `match status_code { ... }` dispatching deserialized bodies into result fields.
pub fn emit_response_match(
    b: &mut CodeBlockBuilder,
    typed_responses: &[TypedResponse],
    value_expr: &dyn Fn(&TypedResponse) -> String,
) {
    b.begin_control_flow("match status_code", ());
    let mut seen: HashSet<String> = HashSet::new();
    for tr in typed_responses {
        if !seen.insert(format!("{}-{}", tr.status, tr.field_name)) {
            continue;
        }
        let status_pattern = status_match_pattern(&tr.status);
        let value_expr = value_expr(tr);
        b.begin_control_flow(&format!("{status_pattern} =>"), ());
        b.add(
            &format!("result.{} = Some({value_expr}?);\n", tr.field_name),
            (),
        );
        b.end_control_flow();
    }
    if !typed_responses.iter().any(|tr| tr.status == "default") {
        b.add("_ => {}\n", ());
    }
    b.end_control_flow();
}

pub fn emit_error_response_match(
    b: &mut CodeBlockBuilder,
    error_type: &str,
    error_responses: &[ErrorResponse],
    value_expr: &dyn Fn(&ErrorResponse) -> String,
) {
    b.begin_control_flow("if !(200..300).contains(&status_code)", ());
    b.begin_control_flow("match status_code", ());

    let mut seen: HashSet<String> = HashSet::new();
    for er in error_responses
        .iter()
        .filter(|er| er.status.parse::<u16>().is_ok())
    {
        let key = format!("{}-{}", er.status, er.variant_name);
        if !seen.insert(key) {
            continue;
        }
        let pattern = status_match_pattern(&er.status);
        let body_expr = value_expr(er);
        b.begin_control_flow(&format!("{pattern} =>"), ());
        b.add(&format!("let body = {body_expr};\n"), ());
        b.add(&format!(
            "return Err({error_type}::{}(ApiError::new(status_code, response_headers.clone(), body_bytes.to_vec(), body)));\n",
            er.variant_name
        ), ());
        b.end_control_flow();
    }

    for er in error_responses
        .iter()
        .filter(|er| er.status.ends_with("XX") && er.status.parse::<u16>().is_err())
    {
        let key = format!("{}-{}", er.status, er.variant_name);
        if !seen.insert(key) {
            continue;
        }
        let pattern = status_match_pattern(&er.status);
        let body_expr = value_expr(er);
        b.begin_control_flow(&format!("{pattern} =>"), ());
        b.add(&format!("let body = {body_expr};\n"), ());
        b.add(&format!(
            "return Err({error_type}::{}(ApiError::new(status_code, response_headers.clone(), body_bytes.to_vec(), body)));\n",
            er.variant_name
        ), ());
        b.end_control_flow();
    }

    if let Some(er) = error_responses
        .iter()
        .find(|er| er.status.eq_ignore_ascii_case("default"))
    {
        let body_expr = value_expr(er);
        b.begin_control_flow("_ =>", ());
        b.add(&format!("let body = {body_expr};\n"), ());
        b.add(&format!(
            "return Err({error_type}::{}(ApiError::new(status_code, response_headers.clone(), body_bytes.to_vec(), body)));\n",
            er.variant_name
        ), ());
        b.end_control_flow();
    } else {
        b.begin_control_flow("_ =>", ());
        b.add(
            "let body = Ok::<Vec<u8>, Error>(body_bytes.to_vec());\n",
            (),
        );
        b.add(&format!(
            "return Err({error_type}::Unexpected(ApiError::new(status_code, response_headers.clone(), body_bytes.to_vec(), body)));\n"
        ), ());
        b.end_control_flow();
    }

    b.end_control_flow();
    b.end_control_flow();
}

pub fn error_response_value_expr(er: &ErrorResponse, bytes_var: &str) -> String {
    let owned_bytes_expr = bytes_var.strip_prefix('&').unwrap_or(bytes_var);
    match er.decoding {
        Some(ResponseDecoding::Json) => {
            format!("serde_json::from_slice({bytes_var}).map_err(Error::Deserialize)")
        }
        Some(ResponseDecoding::Xml) => {
            format!(
                "serde_xml_rs::from_reader(std::io::Cursor::new({bytes_var})).map_err(Error::Xml)"
            )
        }
        Some(ResponseDecoding::TextPlain) => {
            format!("Ok::<String, Error>(String::from_utf8_lossy({bytes_var}).into_owned())")
        }
        Some(ResponseDecoding::OctetStream) => {
            format!("Ok::<Vec<u8>, Error>({owned_bytes_expr}.to_vec())")
        }
        Some(ResponseDecoding::Other(_)) => {
            format!("serde_json::from_slice({bytes_var}).map_err(Error::Deserialize)")
        }
        None => "Ok::<(), Error>(())".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use sigil_stitch::assert_rendered;

    use super::*;

    #[test]
    fn api_call_error_conversion_renders_structured_rust() {
        let variants = vec![
            ("BadRequest".to_string(), "BadRequestError".to_string()),
            ("Conflict".to_string(), "ConflictError".to_string()),
        ];
        let conversion = emit_api_call_error_conversion(
            "CreateResourceError",
            "createResource",
            &variants,
            "Unexpected",
            "Transport",
        );

        assert_rendered!(
            Rust::new(),
            width = 100,
            conversion,
            r#"use crate::runtime::error::ApiCallError;

impl From<CreateResourceError> for ApiCallError {
    fn from(error: CreateResourceError) -> Self {
        match error {
            CreateResourceError::BadRequest(error) => Self::from_api_error("createResource", error),
            CreateResourceError::Conflict(error) => Self::from_api_error("createResource", error),
            CreateResourceError::Unexpected(error) => Self::from_api_error("createResource", error),
            CreateResourceError::Transport(error) => Self::from_runtime_error("createResource", error),
        }
    }
}
"#,
        );
    }
}
