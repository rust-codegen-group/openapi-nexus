use std::collections::{BTreeMap, HashSet};

use crate::codegen::traits::file_writer::FileInfo;
use crate::ir::types::{
    IrOperation, IrParameter, IrRequestBody, IrResponse, IrSpec, IrTypeExpr, ParameterLocation,
};
use heck::{ToLowerCamelCase, ToPascalCase};
use sigil_stitch::lang::java_lang::JavaLang;
use sigil_stitch::prelude::*;

use super::util::{
    build_java_getter, java_boxed_type_str, java_ident, java_type_str, render_value_as_string,
    sanitize_operation_id, unique_name,
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

    let filename = format!("{class_name}.java");
    let mut fb = FileSpec::builder_with(&filename, JavaLang::new())
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
        .add_import(ImportSpec::named("java.util", "HashMap"))
        .add_import(ImportSpec::named("java.util", "List"))
        .add_import(ImportSpec::named("java.util", "Map"))
        .add_import(ImportSpec::named("okhttp3", "Request"))
        .add_import(ImportSpec::named("okhttp3", "Response"));

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
    let ctor_body = sigil_quote!(JavaLang {
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
    sigil_quote!(JavaLang {
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
        sigil_quote!(JavaLang { this.statusCode = statusCode; }).expect("assign"),
        sigil_quote!(JavaLang { this.raw = raw; }).expect("assign"),
    ];
    let mut body_seen: HashSet<String> = HashSet::new();
    for tr in &plan.typed_responses {
        if !body_seen.insert(tr.field_name.clone()) {
            continue;
        }
        field_assignments.push(
            sigil_quote!(JavaLang {
                this.$L(tr.field_name.as_str()) = $L(tr.field_name.as_str());
            })
            .expect("assign"),
        );
    }
    let ctor_body = sigil_quote!(JavaLang {
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

    // Body serialization
    let body_arg = if let Some(body) = &plan.body {
        cb.add_statement(
            &format!("String jsonBody = gson.toJson({})", body.var_name),
            (),
        );
        "jsonBody"
    } else {
        "null"
    };

    // Build request
    let query_arg = if has_query { "query" } else { "null" };
    cb.add_statement(
        &format!(
            "Request request = client.newRequest(\"{}\", path, {query_arg}, {body_arg})",
            plan.op.method.to_uppercase(),
        ),
        (),
    );

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
    let error_block = sigil_quote!(JavaLang {
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
            "String responseBody = response.body() != null ? response.body().string() : \"null\"",
            (),
        );
        let mut seen: HashSet<String> = HashSet::new();

        // Numeric status codes
        for tr in &plan.typed_responses {
            if !seen.insert(tr.field_name.clone()) {
                continue;
            }
            let type_token = format!("new TypeToken<{}>() {{}}.getType()", tr.java_type);
            cb.add_statement(&format!("{} {} = null", tr.java_type, tr.field_name), ());
            if let Ok(code) = tr.status.parse::<u16>() {
                cb.begin_control_flow(&format!("if (response.code() == {code})"), ());
                cb.add_statement(
                    &format!(
                        "{} = gson.fromJson(responseBody, {})",
                        tr.field_name, type_token
                    ),
                    (),
                );
                cb.end_control_flow();
            } else {
                // Wildcard status ("4XX", "5XX", "default"): guard by range
                let guard = wildcard_status_guard_java(&tr.status);
                cb.begin_control_flow(&format!("if ({guard})"), ());
                cb.add_statement(
                    &format!(
                        "{} = gson.fromJson(responseBody, {})",
                        tr.field_name, type_token
                    ),
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
}

struct TypedResponse {
    status: String,
    field_name: String,
    java_type: String,
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
    let java_type = java_type_str(&t);
    let var_name = unique_name("body", used_names);
    Some(BodyBinding {
        var_name,
        java_type,
    })
}

fn plan_response(r: &IrResponse) -> Option<TypedResponse> {
    let t = pick_response_type(r)?;
    let java_type = java_type_str(&t);
    Some(TypedResponse {
        status: r.status.clone(),
        field_name: response_field_name(&r.status),
        java_type,
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
