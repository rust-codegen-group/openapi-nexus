use std::collections::{BTreeMap, HashSet};

use crate::codegen::traits::file_writer::FileInfo;
use crate::ir::types::{
    IrOperation, IrParameter, IrRequestBody, IrResponse, IrSpec, IrTypeExpr, ParameterLocation,
};
use heck::{ToLowerCamelCase, ToPascalCase};
use sigil_stitch::lang::kotlin::Kotlin;
use sigil_stitch::prelude::*;

use super::util::{
    kt_ident, kt_type_str, render_value_as_string, sanitize_operation_id, unique_name,
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
        let filename = format!("{class_name}.kt");
        let body = emit_api_file(tag, ops, package_name);
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

fn emit_api_file(tag: &str, ops: &[&IrOperation], package_name: &str) -> String {
    let class_name = format!("{}Api", tag.to_pascal_case());
    let plans: Vec<OpPlan> = ops.iter().map(|op| plan_operation(op)).collect();

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
            if p.param.required {
                cb.add(&format!("query[\"{}\"] = {stringified}", p.param.name), ());
                cb.add_line();
            } else {
                cb.begin_control_flow(&format!("if ({} != null)", p.var_name), ());
                cb.add(&format!("query[\"{}\"] = {stringified}", p.param.name), ());
                cb.add_line();
                cb.end_control_flow();
            }
        }
    }

    // Body serialization
    let body_arg = if let Some(body) = &plan.body {
        cb.add(
            &format!("val jsonBody = gson.toJson({})", body.var_name),
            (),
        );
        cb.add_line();
        "jsonBody"
    } else {
        "null"
    };

    // Build request
    let query_arg = if has_query { "query" } else { "null" };
    cb.add(
        &format!(
            "val request = client.newRequest(\"{}\", path, {query_arg}, {body_arg})",
            plan.op.method.to_uppercase(),
        ),
        (),
    );
    cb.add_line();

    // Headers
    if !plan.header_params.is_empty() {
        cb.add("var finalRequest = request", ());
        cb.add_line();
        for p in &plan.header_params {
            let stringified = render_value_as_string(&p.var_name, &p.param.type_expr);
            if p.param.required {
                cb.add(
                    &format!(
                        "finalRequest = finalRequest.newBuilder().header(\"{}\", {stringified}).build()",
                        p.param.name
                    ),
                    (),
                );
                cb.add_line();
            } else {
                cb.begin_control_flow(&format!("if ({} != null)", p.var_name), ());
                cb.add(
                    &format!(
                        "finalRequest = finalRequest.newBuilder().header(\"{}\", {stringified}).build()",
                        p.param.name
                    ),
                    (),
                );
                cb.add_line();
                cb.end_control_flow();
            }
        }
    }

    // Execute
    let request_var = if plan.header_params.is_empty() {
        "request"
    } else {
        "finalRequest"
    };
    cb.add(&format!("val response = client.execute({request_var})"), ());
    cb.add_line();
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
        cb.add("val responseBody = response.body?.string()", ());
        cb.add_line();
        let mut seen: HashSet<String> = HashSet::new();
        for tr in &plan.typed_responses {
            if !seen.insert(tr.field_name.clone()) {
                continue;
            }
            cb.add_line();
            let deserialize_expr = format!(
                "gson.fromJson<{}>(responseBody, object : TypeToken<{}>() {{}}.type)",
                tr.kt_type, tr.kt_type
            );
            if let Ok(code) = tr.status.parse::<u16>() {
                cb.add(
                    &format!(
                        "val {} = if (response.code == {code}) {} else null",
                        tr.field_name, deserialize_expr
                    ),
                    (),
                );
            } else {
                // "default" or wildcard status: populate unconditionally
                cb.add(&format!("val {} = {}", tr.field_name, deserialize_expr), ());
            }
            cb.add_line();
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
}

struct TypedResponse {
    status: String,
    field_name: String,
    kt_type: String,
}

fn plan_operation<'a>(op: &'a IrOperation) -> OpPlan<'a> {
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
    let kt_type = kt_type_str(&t);
    let var_name = unique_name("body", used_names);
    Some(BodyBinding { var_name, kt_type })
}

fn plan_response(r: &IrResponse) -> Option<TypedResponse> {
    let t = pick_response_type(r)?;
    let kt_type = kt_type_str(&t);
    Some(TypedResponse {
        status: r.status.clone(),
        field_name: response_field_name(&r.status),
        kt_type,
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

fn pick_response_type(r: &IrResponse) -> Option<IrTypeExpr> {
    r.content
        .get("application/json")
        .cloned()
        .or_else(|| r.content.values().next().cloned())
}

fn pick_body_type(body: &IrRequestBody) -> Option<IrTypeExpr> {
    body.content
        .get("application/json")
        .cloned()
        .or_else(|| body.content.values().next().cloned())
}
