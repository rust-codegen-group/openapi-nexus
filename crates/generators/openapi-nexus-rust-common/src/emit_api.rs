//! API emission for IR operations (Rust APIs).
//!
//! Groups operations by tag, emits one `apis/<tag>.rs` per tag group. Each file
//! declares a `{Tag}Api` struct holding a `&runtime::Client` and exposes one
//! method per operation.
//!
//! Backend-specific method bodies are injected via a closure, keeping this module
//! agnostic to the HTTP library (reqwest, ureq, aioduct, etc.).

use std::collections::{BTreeMap, HashSet};

use heck::{ToPascalCase, ToSnakeCase};
use openapi_nexus_core::traits::file_writer::FileInfo;
use openapi_nexus_ir::types::{
    IrOperation, IrParameter, IrRequestBody, IrResponse, IrSpec, IrTypeExpr, ParameterLocation,
};

use crate::emit_models::rust_type_str_qualified;

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
    body_emitter: &dyn Fn(&OpPlan<'_>) -> String,
) -> Result<Vec<FileInfo>, String> {
    let by_tag = group_by_tag(&ir.operations);
    let mut files = Vec::with_capacity(by_tag.len());
    let mut mod_entries = Vec::new();

    for (tag, ops) in &by_tag {
        let stem = tag.to_snake_case();
        let filename = format!("{stem}.rs");
        mod_entries.push(stem);
        let body = emit_api_file(tag, ops, config, body_emitter);
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
    config: &RustBackendConfig,
    body_emitter: &dyn Fn(&OpPlan<'_>) -> String,
) -> String {
    let struct_name = format!("{}Api", tag.to_pascal_case());
    let plans: Vec<OpPlan> = ops.iter().map(|op| plan_operation(op)).collect();

    let mut out = String::new();
    out.push_str("use crate::runtime::client::Client;\n");
    out.push_str("use crate::runtime::error::Error;\n");
    out.push('\n');

    // Struct generics (e.g., `<'a, R: aioduct::Runtime>`)
    // struct_gen: used in struct definition (has bounds), e.g., `<'a, R: aioduct::Runtime>`
    // impl_gen: used in impl header (has bounds), e.g., `<'a, R: aioduct::Runtime>`
    // type_args: used after struct name in impl (no bounds), e.g., `<'a, R>`
    let (struct_gen, impl_gen, type_args, client_field_args) = match &config.struct_generics {
        Some(g) => {
            let client_args = config.client_type_args.as_deref().unwrap_or("");
            // Extract just the type parameter name (before the colon) for type args
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

    out.push_str(&format!("/// API operations under the \"{tag}\" tag.\n"));
    out.push_str(&format!("pub struct {struct_name}{struct_gen} {{\n"));
    out.push_str(&format!("    client: &'a Client{client_field_args},\n"));
    out.push_str("}\n\n");

    out.push_str(&format!("impl{impl_gen} {struct_name}{type_args} {{\n"));
    out.push_str(&format!(
        "    /// Create a new `{struct_name}` bound to the given client.\n"
    ));
    out.push_str(&format!(
        "    pub fn new(client: &'a Client{client_field_args}) -> Self {{\n        Self {{ client }}\n    }}\n"
    ));

    for plan in &plans {
        out.push('\n');
        out.push_str(&emit_operation(plan, config, body_emitter));
    }

    out.push_str("}\n");

    // Response structs after the impl block
    for plan in &plans {
        out.push('\n');
        out.push_str(&emit_response_struct(plan));
    }

    out
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
}

pub struct TypedResponse {
    pub status: String,
    pub field_name: String,
    pub rust_type: String,
}

pub fn plan_operation<'a>(op: &'a IrOperation) -> OpPlan<'a> {
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
        let (rust_type, is_optional) = param_rust_type(p);
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
        .and_then(|b| plan_body(b, &mut used_names));

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

pub fn plan_body(b: &IrRequestBody, used_names: &mut HashSet<String>) -> Option<BodyBinding> {
    let t = pick_body_type(b)?;
    let rust_type = rust_type_str_qualified(&t);
    let var_name = unique_name("body", used_names);
    Some(BodyBinding {
        var_name,
        rust_type,
    })
}

pub fn plan_response(r: &IrResponse) -> Option<TypedResponse> {
    let t = pick_response_type(r)?;
    let rust_type = rust_type_str_qualified(&t);
    Some(TypedResponse {
        status: r.status.clone(),
        field_name: response_field_name(&r.status),
        rust_type,
    })
}

pub fn param_rust_type(p: &IrParameter) -> (String, bool) {
    let base = rust_type_str_qualified(&p.type_expr);
    if p.required {
        (base, false)
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
    body_emitter: &dyn Fn(&OpPlan<'_>) -> String,
) -> String {
    let OpPlan {
        op,
        method_name,
        response_type,
        ..
    } = plan;

    let mut out = String::new();

    if let Some(summary) = &op.summary {
        out.push_str(&format!("    /// {summary}\n"));
    } else {
        out.push_str(&format!(
            "    /// {} {}\n",
            op.method.to_uppercase(),
            op.path,
        ));
    }
    if let Some(desc) = &op.description {
        out.push_str("    ///\n");
        for line in desc.lines() {
            out.push_str(&format!("    /// {line}\n"));
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
        } else {
            format!("&{}", p.rust_type)
        };
        params.push(format!("{}: {ty}", p.var_name));
    }
    if let Some(body) = &plan.body {
        params.push(format!("{}: &{}", body.var_name, body.rust_type));
    }

    let async_kw = if config.is_async { "async " } else { "" };
    out.push_str(&format!(
        "    pub {async_kw}fn {method_name}(\n        {},\n    ) -> Result<{response_type}, Error> {{\n",
        params.join(",\n        "),
    ));

    out.push_str(&body_emitter(plan));
    out.push_str("    }\n");
    out
}

pub fn emit_response_struct(plan: &OpPlan<'_>) -> String {
    let mut out = String::new();
    out.push_str(&format!("/// Response from `{}`.\n", plan.method_name));
    out.push_str("#[derive(Debug)]\n");
    out.push_str(&format!("pub struct {} {{\n", plan.response_type));
    out.push_str("    pub status_code: u16,\n");

    let mut seen: HashSet<String> = HashSet::new();
    for tr in &plan.typed_responses {
        if !seen.insert(tr.field_name.clone()) {
            continue;
        }
        out.push_str(&format!(
            "    pub {}: Option<{}>,\n",
            tr.field_name, tr.rust_type
        ));
    }

    out.push_str("}\n");
    out
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
        s => format!("status_{s}"),
    }
}

pub fn pick_body_type(b: &IrRequestBody) -> Option<IrTypeExpr> {
    b.content
        .get("application/json")
        .or_else(|| b.content.values().next())
        .cloned()
}

pub fn pick_response_type(r: &IrResponse) -> Option<IrTypeExpr> {
    r.content
        .get("application/json")
        .or_else(|| r.content.values().next())
        .cloned()
}

pub fn render_to_string(var: &str, _type_expr: &IrTypeExpr, _is_optional: bool) -> String {
    format!("{var}.to_string()")
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
