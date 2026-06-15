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
use crate::ir::types::{
    IrObject, IrOperation, IrParameter, IrPrimitive, IrRequestBody, IrResponse, IrSchemaKind,
    IrSpec, IrTypeExpr, ParameterLocation,
};
use heck::{ToPascalCase, ToSnakeCase};
use sigil_stitch::code_block::{CodeBlock, CodeBlockBuilder};
use sigil_stitch::prelude::sigil_quote;
use sigil_stitch::spec::annotation_spec::AnnotationSpec;
use sigil_stitch::spec::field_spec::FieldSpec;
use sigil_stitch::spec::file_spec::FileSpec;
use sigil_stitch::spec::import_spec::ImportSpec;
use sigil_stitch::spec::modifiers::{TypeKind, Visibility};
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
    /// Extra generic parameters on the Api struct, e.g., `"R: aioduct::Runtime"`.
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
    body_emitter: &dyn Fn(&OpPlan<'_>) -> CodeBlock,
) -> Result<Vec<FileInfo>, String> {
    let by_tag = group_by_tag(&ir.operations);
    let mut files = Vec::with_capacity(by_tag.len());
    let mut mod_entries = Vec::new();

    for (tag, ops) in &by_tag {
        let stem = tag.to_snake_case();
        let filename = format!("{stem}.rs");
        mod_entries.push(stem);
        let body = emit_api_file(tag, ops, ir, config, response_extra_derives, body_emitter);
        let content = format!("{header}{body}");
        files.push(FileInfo::api(filename, content));
    }

    // mod.rs
    let mut mod_content = String::from(header);
    for entry in &mod_entries {
        mod_content.push_str(&format!("mod {entry};\npub use {entry}::*;\n"));
    }
    files.push(FileInfo::api("mod.rs".to_string(), mod_content));

    Ok(files)
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
    body_emitter: &dyn Fn(&OpPlan<'_>) -> CodeBlock,
) -> String {
    let struct_name = format!("{}Api", tag.to_pascal_case());
    let plans: Vec<OpPlan> = ops.iter().map(|op| plan_operation(op, ir)).collect();

    let stem = tag.to_snake_case();
    let mut fsb = FileSpec::builder(&format!("{stem}.rs"));

    // Use imports
    fsb = fsb.add_import(ImportSpec::named("crate::runtime::client", "Client"));
    fsb = fsb.add_import(ImportSpec::named("crate::runtime::error", "Error"));

    // Struct generics (e.g., `<'a, R: aioduct::Runtime>`)
    let (struct_gen, impl_gen, type_args, client_field_args) = match &config.struct_generics {
        Some(g) => {
            let client_args = config.client_type_args.as_deref().unwrap_or("");
            let param_name = g.split(':').next().unwrap_or(g).trim();
            (
                format!("<'a, {g}>"),
                format!("<'a, {g}>"),
                format!("<'a, {param_name}>"),
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
        fsb = fsb.add_type(emit_response_struct(plan, response_extra_derives));
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
    pub path_params: Vec<ParamBinding<'a>>,
    pub query_params: Vec<ParamBinding<'a>>,
    pub header_params: Vec<ParamBinding<'a>>,
    pub body: Option<BodyBinding>,
    pub typed_responses: Vec<TypedResponse>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultipartPart {
    pub wire_name: String,
    pub field_name: String,
    pub is_binary: bool,
    pub required: bool,
    pub value_encoding: MultipartValueEncoding,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MultipartValueEncoding {
    Text,
    Json,
}

pub struct TypedResponse {
    pub status: String,
    pub field_name: String,
    pub rust_type: String,
    pub decoding: ResponseDecoding,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponseDecoding {
    Json,
    Xml,
    TextPlain,
    OctetStream,
    Other(String),
}

pub fn plan_operation<'a>(op: &'a IrOperation, ir: &'a IrSpec) -> OpPlan<'a> {
    let op_id = sanitize_operation_id(&op.operation_id, &op.method, &op.path);
    let method_name = op_id.to_snake_case();
    let response_type = format!("{}Response", op_id.to_pascal_case());

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
        .and_then(|b| plan_body(b, &mut used_names, ir));

    let typed_responses = op
        .responses
        .iter()
        .filter_map(|r| plan_response(r, ir))
        .collect();

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

pub fn plan_body(
    b: &IrRequestBody,
    used_names: &mut HashSet<String>,
    ir: &IrSpec,
) -> Option<BodyBinding> {
    let (media_type, t) = pick_body_content(b)?;
    let encoding = body_encoding(&media_type);
    let rust_type = match encoding {
        BodyEncoding::OctetStream => "Vec<u8>".to_string(),
        BodyEncoding::TextPlain => "String".to_string(),
        _ => rust_type_str_qualified(&t, ir),
    };
    let multipart_parts = if encoding == BodyEncoding::Multipart {
        multipart_parts_for(&t, ir).unwrap_or_default()
    } else {
        Vec::new()
    };
    let multipart_supported =
        encoding != BodyEncoding::Multipart || multipart_parts_for(&t, ir).is_some();
    let var_name = unique_name("body", used_names);
    Some(BodyBinding {
        var_name,
        rust_type,
        media_type,
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
        params.push(format!("{}: &{}", body.var_name, body.rust_type));
    }

    let async_kw = if config.is_async { "async " } else { "" };
    b.add(
        &format!(
            "pub {async_kw}fn {method_name}(\n    {},\n) -> Result<{response_type}, Error>",
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

pub fn emit_response_struct(plan: &OpPlan<'_>, extra: Option<&ExtraDeriveConfig>) -> TypeSpec {
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

    tb.build().expect("TypeSpec builds")
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

fn multipart_parts_for(t: &IrTypeExpr, ir: &IrSpec) -> Option<Vec<MultipartPart>> {
    // TODO: Honor OpenAPI multipart encoding metadata once it is represented in the IR.
    resolve_object(t, ir).map(|obj| multipart_parts_from_object(obj, ir))
}

fn multipart_parts_from_object(obj: &IrObject, ir: &IrSpec) -> Vec<MultipartPart> {
    obj.properties
        .iter()
        .map(|(wire_name, prop)| MultipartPart {
            wire_name: wire_name.clone(),
            field_name: rust_field_name(wire_name),
            is_binary: is_binary_type(&prop.type_expr, ir),
            required: prop.required && !prop.nullable,
            value_encoding: multipart_value_encoding(&prop.type_expr, ir),
        })
        .collect()
}

fn resolve_object<'a>(expr: &IrTypeExpr, ir: &'a IrSpec) -> Option<&'a IrObject> {
    match expr {
        IrTypeExpr::Named(name) => match ir.schemas.get(name).map(|schema| &schema.kind) {
            Some(IrSchemaKind::Object(obj)) => Some(obj),
            Some(IrSchemaKind::Alias(inner)) => resolve_object(inner, ir),
            _ => None,
        },
        IrTypeExpr::Nullable(inner) => resolve_object(inner, ir),
        _ => None,
    }
}

fn is_binary_type(expr: &IrTypeExpr, ir: &IrSpec) -> bool {
    match expr {
        IrTypeExpr::Primitive(IrPrimitive::Binary) => true,
        IrTypeExpr::Nullable(inner) => is_binary_type(inner, ir),
        IrTypeExpr::Named(name) => ir.schemas.get(name).is_some_and(|schema| {
            matches!(&schema.kind, IrSchemaKind::Alias(inner) if is_binary_type(inner, ir))
        }),
        _ => false,
    }
}

fn multipart_value_encoding(expr: &IrTypeExpr, ir: &IrSpec) -> MultipartValueEncoding {
    if is_multipart_text_type(expr, ir) {
        MultipartValueEncoding::Text
    } else {
        MultipartValueEncoding::Json
    }
}

fn is_multipart_text_type(expr: &IrTypeExpr, ir: &IrSpec) -> bool {
    match expr {
        IrTypeExpr::Primitive(_) | IrTypeExpr::StringLiteral(_) | IrTypeExpr::StringEnum(_) => true,
        IrTypeExpr::Nullable(inner) => is_multipart_text_type(inner, ir),
        IrTypeExpr::Named(name) => ir.schemas.get(name).is_some_and(|schema| {
            matches!(&schema.kind, IrSchemaKind::Alias(inner) if is_multipart_text_type(inner, ir))
        }),
        _ => false,
    }
}

fn rust_field_name(wire_name: &str) -> String {
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
    match part.value_encoding {
        MultipartValueEncoding::Text => format!("{base}.{}.to_string()", part.field_name),
        MultipartValueEncoding::Json => {
            format!("serde_json::to_string(&{base}.{})?", part.field_name)
        }
    }
}

pub fn binary_field_expr(base: &str, part: &MultipartPart) -> String {
    format!("{base}.{}.clone()", part.field_name)
}

pub fn optional_text_field_expr(value: &str, part: &MultipartPart) -> String {
    match part.value_encoding {
        MultipartValueEncoding::Text => format!("{value}.to_string()"),
        MultipartValueEncoding::Json => format!("serde_json::to_string({value})?"),
    }
}

pub fn optional_binary_field_expr(value: &str) -> String {
    format!("{value}.clone()")
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
    let mut fields = vec!["status_code".to_string()];
    let mut seen: HashSet<String> = HashSet::new();
    for tr in typed_responses {
        if seen.insert(tr.field_name.clone()) {
            fields.push(format!("{}: None", tr.field_name));
        }
    }
    b.add(
        &format!(
            "let mut result = {response_type} {{ {} }};\n",
            fields.join(", ")
        ),
        (),
    );
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
