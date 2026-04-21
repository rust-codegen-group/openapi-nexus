//! API emission for IR operations (Rust reqwest APIs).
//!
//! Groups operations by tag, emits one `apis/<tag>.rs` per tag group. Each file
//! declares a `{Tag}Api` struct holding a `&runtime::Client` and exposes one
//! async method per operation.
//!
//! Built as plain string concatenation (same approach as the Go API generator)
//! because imperative method bodies don't fit sigil-stitch's structural builders.

use std::collections::{BTreeMap, HashSet};

use heck::{ToPascalCase, ToSnakeCase};
use openapi_nexus_core::traits::file_writer::FileInfo;
use openapi_nexus_ir::types::{
    IrOperation, IrParameter, IrRequestBody, IrResponse, IrSpec, IrTypeExpr, ParameterLocation,
};

use crate::sigil_emit::rust_type_str_qualified;

/// Generate every API file from the IR.
pub fn generate_api_files(ir: &IrSpec, header: &str) -> Result<Vec<FileInfo>, String> {
    let by_tag = group_by_tag(&ir.operations);
    let mut files = Vec::with_capacity(by_tag.len());
    let mut mod_entries = Vec::new();

    for (tag, ops) in &by_tag {
        let stem = tag.to_snake_case();
        let filename = format!("{stem}.rs");
        mod_entries.push(stem);
        let body = emit_api_file(tag, ops);
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

fn emit_api_file(tag: &str, ops: &[&IrOperation]) -> String {
    let struct_name = format!("{}Api", tag.to_pascal_case());
    let plans: Vec<OpPlan> = ops.iter().map(|op| plan_operation(op)).collect();

    let mut out = String::new();
    out.push_str("use crate::runtime::client::Client;\n");
    out.push_str("use crate::runtime::error::Error;\n");
    out.push('\n');

    out.push_str(&format!("/// API operations under the \"{tag}\" tag.\n"));
    out.push_str(&format!("pub struct {struct_name}<'a> {{\n"));
    out.push_str("    client: &'a Client,\n");
    out.push_str("}\n\n");

    out.push_str(&format!("impl<'a> {struct_name}<'a> {{\n"));
    out.push_str(&format!(
        "    /// Create a new `{struct_name}` bound to the given client.\n"
    ));
    out.push_str("    pub fn new(client: &'a Client) -> Self {\n        Self { client }\n    }\n");

    for plan in &plans {
        out.push('\n');
        out.push_str(&emit_operation(plan));
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
// Operation planning
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
    rust_type: String,
    is_optional: bool,
}

struct BodyBinding {
    var_name: String,
    rust_type: String,
}

struct TypedResponse {
    status: String,
    field_name: String,
    rust_type: String,
}

fn plan_operation<'a>(op: &'a IrOperation) -> OpPlan<'a> {
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

fn plan_body(b: &IrRequestBody, used_names: &mut HashSet<String>) -> Option<BodyBinding> {
    let t = pick_body_type(b)?;
    let rust_type = rust_type_str_qualified(&t);
    let var_name = unique_name("body", used_names);
    Some(BodyBinding {
        var_name,
        rust_type,
    })
}

fn plan_response(r: &IrResponse) -> Option<TypedResponse> {
    let t = pick_response_type(r)?;
    let rust_type = rust_type_str_qualified(&t);
    Some(TypedResponse {
        status: r.status.clone(),
        field_name: response_field_name(&r.status),
        rust_type,
    })
}

fn param_rust_type(p: &IrParameter) -> (String, bool) {
    let base = rust_type_str_qualified(&p.type_expr);
    if p.required {
        (base, false)
    } else {
        (format!("Option<{base}>"), true)
    }
}

fn unique_name(desired: &str, used: &mut HashSet<String>) -> String {
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

fn emit_operation(plan: &OpPlan<'_>) -> String {
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

    out.push_str(&format!(
        "    pub async fn {method_name}(\n        {},\n    ) -> Result<{response_type}, Error> {{\n",
        params.join(",\n        "),
    ));

    out.push_str(&emit_method_body(plan));
    out.push_str("    }\n");
    out
}

fn emit_response_struct(plan: &OpPlan<'_>) -> String {
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

fn emit_method_body(plan: &OpPlan<'_>) -> String {
    let OpPlan {
        op,
        response_type,
        path_params,
        query_params,
        header_params,
        body,
        typed_responses,
        ..
    } = plan;

    let mut out = String::new();

    // Build path
    if path_params.is_empty() {
        out.push_str(&format!(
            "        let path = \"{}\".to_string();\n",
            op.path
        ));
    } else {
        out.push_str(&format!(
            "        let mut path = \"{}\".to_string();\n",
            op.path
        ));
        for p in path_params {
            let placeholder = format!("{{{}}}", p.param.name);
            let value_expr = render_to_string(&p.var_name, &p.param.type_expr, p.is_optional);
            out.push_str(&format!(
                "        path = path.replace(\"{placeholder}\", &{value_expr});\n"
            ));
        }
    }

    // Build query string
    if !query_params.is_empty() {
        out.push_str("        let mut query_parts: Vec<(&str, String)> = Vec::new();\n");
        for p in query_params {
            if p.is_optional {
                let value_expr = render_to_string("v", &p.param.type_expr, false);
                out.push_str(&format!(
                    "        if let Some(v) = &{} {{\n            query_parts.push((\"{}\", {value_expr}));\n        }}\n",
                    p.var_name, p.param.name,
                ));
            } else {
                let value_expr = render_to_string(&p.var_name, &p.param.type_expr, false);
                out.push_str(&format!(
                    "        query_parts.push((\"{}\", {value_expr}));\n",
                    p.param.name,
                ));
            }
        }
        out.push_str("        if !query_parts.is_empty() {\n");
        out.push_str("            let qs: Vec<String> = query_parts.iter().map(|(k, v)| format!(\"{}={}\", k, v)).collect();\n");
        out.push_str("            path = format!(\"{}?{}\", path, qs.join(\"&\"));\n");
        out.push_str("        }\n");
    }

    // Build request
    let method = op.method.to_uppercase();
    out.push_str(&format!(
        "        let mut req = self.client.request(reqwest::Method::{method}, &path).await?;\n"
    ));

    // Headers
    for p in header_params {
        if p.is_optional {
            let value_expr = render_to_string("v", &p.param.type_expr, false);
            out.push_str(&format!(
                "        if let Some(v) = &{} {{\n            req = req.header(\"{}\", {value_expr});\n        }}\n",
                p.var_name, p.param.name,
            ));
        } else {
            let value_expr = render_to_string(&p.var_name, &p.param.type_expr, false);
            out.push_str(&format!(
                "        req = req.header(\"{}\", {value_expr});\n",
                p.param.name,
            ));
        }
    }

    // Body
    if let Some(body) = body {
        out.push_str(&format!("        req = req.json(&{});\n", body.var_name));
    }

    // Send
    out.push_str("        let resp = self.client.send(req).await?;\n");
    out.push_str("        let status_code = resp.status().as_u16();\n");

    // Parse response
    if typed_responses.is_empty() {
        out.push_str(&format!("        Ok({response_type} {{ status_code }})\n"));
    } else {
        out.push_str("        let body_bytes = resp.bytes().await.map_err(Error::Network)?;\n");
        out.push_str(&format!("        let mut result = {response_type} {{\n"));
        out.push_str("            status_code,\n");
        let mut seen: HashSet<String> = HashSet::new();
        for tr in typed_responses {
            if !seen.insert(tr.field_name.clone()) {
                continue;
            }
            out.push_str(&format!("            {}: None,\n", tr.field_name));
        }
        out.push_str("        };\n");

        out.push_str("        match status_code {\n");
        seen.clear();
        for tr in typed_responses {
            if !seen.insert(format!("{}-{}", tr.status, tr.field_name)) {
                continue;
            }
            let status_pattern = if tr.status == "default" {
                "_".to_string()
            } else {
                tr.status.clone()
            };
            out.push_str(&format!("            {status_pattern} => {{\n"));
            out.push_str(&format!(
                "                result.{} = Some(serde_json::from_slice(&body_bytes).map_err(Error::Deserialize)?);\n",
                tr.field_name
            ));
            out.push_str("            }\n");
        }
        if !typed_responses.iter().any(|tr| tr.status == "default") {
            out.push_str("            _ => {}\n");
        }
        out.push_str("        }\n");
        out.push_str("        Ok(result)\n");
    }

    out
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn sanitize_operation_id(id: &str, method: &str, path: &str) -> String {
    if !id.is_empty() {
        return id.to_string();
    }
    format!(
        "{}_{}",
        method,
        path.replace('/', "_").replace(['{', '}'], "")
    )
}

fn response_field_name(status: &str) -> String {
    match status {
        "200" => "data".to_string(),
        "201" => "created".to_string(),
        "204" => "no_content".to_string(),
        "default" => "error_body".to_string(),
        s => format!("status_{s}"),
    }
}

fn pick_body_type(b: &IrRequestBody) -> Option<IrTypeExpr> {
    b.content
        .get("application/json")
        .or_else(|| b.content.values().next())
        .cloned()
}

fn pick_response_type(r: &IrResponse) -> Option<IrTypeExpr> {
    r.content
        .get("application/json")
        .or_else(|| r.content.values().next())
        .cloned()
}

fn render_to_string(var: &str, _type_expr: &IrTypeExpr, _is_optional: bool) -> String {
    format!("{var}.to_string()")
}

fn is_copy_type(ty: &str) -> bool {
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
