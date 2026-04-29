//! API emission for IR operations (Python API classes).
//!
//! Uses sigil-stitch high-level APIs (TypeSpec, FunSpec, TypeName, FileSpec) for
//! structured code generation with automatic import tracking. Groups operations
//! by tag, emits one `apis/{tag}_api.py` per tag.

use std::collections::{BTreeMap, HashSet};

use crate::codegen::traits::file_writer::FileInfo;
use crate::ir::types::{
    IrOperation, IrParameter, IrPrimitive, IrRequestBody, IrResponse, IrSpec, IrTypeExpr,
    ParameterLocation,
};
use heck::{ToPascalCase, ToSnakeCase};
use sigil_stitch::code_block::CodeBlock;
use sigil_stitch::lang::python::Python;
use sigil_stitch::prelude::*;

use super::emit_models::{api_type_name, future_annotations_header, is_object_schema};

/// Generate every API file from the IR.
pub fn generate_api_files(ir: &IrSpec, header: &str) -> Result<Vec<FileInfo>, String> {
    let by_tag = group_by_tag(&ir.operations);
    let mut files = Vec::with_capacity(by_tag.len());
    for (tag, ops) in &by_tag {
        let stem = tag.to_snake_case();
        let filename = format!("{stem}_api.py");
        let body = emit_api_file(tag, ops, ir, header);
        files.push(FileInfo::api(filename, body));
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

fn emit_api_file(tag: &str, ops: &[&IrOperation], ir: &IrSpec, header: &str) -> String {
    let class_name = format!("{}Api", tag.to_pascal_case());
    let plans: Vec<OpPlan> = ops.iter().map(|op| plan_operation(op)).collect();

    let client_type = TypeName::importable("..runtime.client", "Client");
    let error_type = TypeName::importable("..runtime.errors", "ApiError");

    // __init__ method via FunSpec
    let init_body = CodeBlock::of("self._client = client", ()).expect("static body");
    let init = FunSpec::builder("__init__")
        .add_param(ParameterSpec::of("self", TypeName::primitive("")))
        .add_param(ParameterSpec::of("client", client_type))
        .returns(TypeName::primitive("None"))
        .body(init_body)
        .build()
        .expect("__init__ FunSpec builds");

    let mut cls = TypeSpec::builder(&class_name, TypeKind::Class).add_method(init);

    for plan in &plans {
        cls = cls.add_method(build_api_method(plan, ir, &error_type));
    }

    let file = FileSpec::builder_with(&format!("{}_api.py", tag.to_snake_case()), Python::new())
        .header(future_annotations_header())
        .add_type(cls.build().expect("API TypeSpec builds"))
        .build()
        .expect("API FileSpec builds");

    let body = file.render(120).unwrap_or_default();
    let mut content = String::with_capacity(header.len() + body.len());
    content.push_str(header);
    content.push_str(&body);
    content
}

fn build_api_method(plan: &OpPlan<'_>, ir: &IrSpec, error_type: &TypeName) -> FunSpec {
    let mut fun = FunSpec::builder(&plan.method_name);

    // self (bare, no type annotation)
    fun = fun.add_param(ParameterSpec::of("self", TypeName::primitive("")));

    // Positional params (path params)
    for p in &plan.path_params {
        fun = fun.add_param(ParameterSpec::of(
            &p.var_name,
            api_type_name(&p.param.type_expr),
        ));
    }

    // Keyword-only separator
    let has_keyword_params =
        !plan.query_params.is_empty() || !plan.header_params.is_empty() || plan.body.is_some();
    if has_keyword_params {
        fun = fun.add_param(ParameterSpec::of("*", TypeName::primitive("")));
    }

    // Required query/header params first
    for p in plan.query_params.iter().chain(&plan.header_params) {
        if p.param.required {
            fun = fun.add_param(ParameterSpec::of(
                &p.var_name,
                api_type_name(&p.param.type_expr),
            ));
        }
    }

    // Body param
    if let Some(b) = &plan.body {
        let ty = api_type_name(&b.type_expr);
        if b.required {
            fun = fun.add_param(ParameterSpec::of(&b.var_name, ty));
        } else {
            fun = fun.add_param(
                ParameterSpec::builder(&b.var_name, TypeName::optional(ty))
                    .default_value(CodeBlock::of("None", ()).expect("None"))
                    .build()
                    .expect("optional body param"),
            );
        }
    }

    // Optional query/header params last
    for p in plan.query_params.iter().chain(&plan.header_params) {
        if !p.param.required {
            fun = fun.add_param(
                ParameterSpec::builder(
                    &p.var_name,
                    TypeName::optional(api_type_name(&p.param.type_expr)),
                )
                .default_value(CodeBlock::of("None", ()).expect("None"))
                .build()
                .expect("optional param"),
            );
        }
    }

    // Return type — auto-tracked via TypeName
    let return_type = if plan.typed_responses.is_empty() {
        TypeName::primitive("None")
    } else {
        api_type_name(&plan.typed_responses[0].type_expr)
    };
    fun = fun.returns(return_type);

    // Docstring
    if let Some(summary) = &plan.op.summary {
        fun = fun.doc(&format!("{summary}."));
    }

    // Method body (imperative control flow, stays as CodeBlock)
    fun = fun.body(build_method_body(plan, ir, error_type));

    fun.build().expect("API method FunSpec builds")
}

fn build_method_body(plan: &OpPlan<'_>, ir: &IrSpec, error_type: &TypeName) -> CodeBlock {
    let mut cb = CodeBlock::builder();

    // Path interpolation
    let path_expr = if plan.path_params.is_empty() {
        format!("\"{}\"", plan.op.path)
    } else {
        let mut path_template = plan.op.path.clone();
        for p in &plan.path_params {
            let placeholder = format!("{{{}}}", p.param.name);
            let replacement = format!("{{{}}}", p.var_name);
            path_template = path_template.replace(&placeholder, &replacement);
        }
        format!("f\"{}\"", path_template)
    };
    cb.add_statement(&format!("path = {path_expr}"), ());

    // Query params
    let has_query = !plan.query_params.is_empty();
    if has_query {
        cb.add_statement("params: dict[str, str] = {}", ());
        for p in &plan.query_params {
            let stringify = render_stringify(&p.var_name, &p.param.type_expr);
            if p.param.required {
                cb.add_statement(&format!("params[\"{}\"] = {stringify}", p.param.name), ());
            } else {
                cb.add_statement(&format!("if {} is not None:%>", p.var_name), ());
                cb.add_statement(&format!("params[\"{}\"] = {stringify}%<", p.param.name), ());
            }
        }
    }

    // Header params
    let has_headers = !plan.header_params.is_empty();
    if has_headers {
        cb.add_statement("headers: dict[str, str] = {}", ());
        for p in &plan.header_params {
            let stringify = render_stringify(&p.var_name, &p.param.type_expr);
            if p.param.required {
                cb.add_statement(&format!("headers[\"{}\"] = {stringify}", p.param.name), ());
            } else {
                cb.add_statement(&format!("if {} is not None:%>", p.var_name), ());
                cb.add_statement(
                    &format!("headers[\"{}\"] = {stringify}%<", p.param.name),
                    (),
                );
            }
        }
    }

    // Body serialization
    let body_expr = if let Some(b) = &plan.body {
        if is_object_type(&b.type_expr, ir) {
            if b.required {
                format!("{}.to_dict()", b.var_name)
            } else {
                format!(
                    "{}.to_dict() if {} is not None else None",
                    b.var_name, b.var_name
                )
            }
        } else {
            b.var_name.clone()
        }
    } else {
        String::new()
    };

    // Request call
    let mut request_args = vec![
        format!("\"{}\"", plan.op.method.to_uppercase()),
        "path".to_string(),
    ];
    if has_query {
        request_args.push("params=params".to_string());
    }
    if plan.body.is_some() {
        request_args.push(format!("json={body_expr}"));
    }
    if has_headers {
        request_args.push("headers=headers".to_string());
    }

    cb.add_statement(
        &format!(
            "response = self._client.request({})",
            request_args.join(", "),
        ),
        (),
    );

    // Error handling — %T for ApiError auto-import
    cb.add_statement("if response.status_code >= 400:%>", ());
    cb.add_statement(
        "raise %T(response.status_code, response.reason, response.content)%<",
        (error_type.clone(),),
    );

    // Response parsing
    if !plan.typed_responses.is_empty() {
        let tr = &plan.typed_responses[0];
        let parse_expr = render_response_parse(&tr.type_expr, ir);
        cb.add_statement(&format!("return {parse_expr}"), ());
    } else {
        cb.add_statement("return None", ());
    }

    cb.build().expect("API method body builds")
}

fn render_stringify(var: &str, type_expr: &IrTypeExpr) -> String {
    match type_expr {
        IrTypeExpr::Primitive(
            IrPrimitive::String
            | IrPrimitive::Date
            | IrPrimitive::DateTime
            | IrPrimitive::Uuid
            | IrPrimitive::StringWithFormat(_),
        )
        | IrTypeExpr::StringLiteral(_)
        | IrTypeExpr::StringEnum(_)
        | IrTypeExpr::Named(_) => format!("str({var})"),
        IrTypeExpr::Primitive(IrPrimitive::Boolean) => format!("str({var}).lower()"),
        IrTypeExpr::Primitive(
            IrPrimitive::Integer
            | IrPrimitive::IntegerWithFormat(_)
            | IrPrimitive::Number
            | IrPrimitive::NumberWithFormat(_),
        ) => format!("str({var})"),
        IrTypeExpr::Nullable(inner) => render_stringify(var, inner),
        _ => format!("str({var})"),
    }
}

fn render_response_parse(type_expr: &IrTypeExpr, ir: &IrSpec) -> String {
    match type_expr {
        IrTypeExpr::Named(name) => {
            let py_name = name.to_pascal_case();
            if is_object_schema(name, ir) {
                format!("{py_name}.from_dict(response.json())")
            } else {
                "response.json()  # type: ignore[return-value]".to_string()
            }
        }
        IrTypeExpr::Array(inner) => {
            if let IrTypeExpr::Named(name) = inner.as_ref()
                && is_object_schema(name, ir)
            {
                let py_name = name.to_pascal_case();
                return format!("[{py_name}.from_dict(item) for item in response.json()]");
            }
            "response.json()  # type: ignore[return-value]".to_string()
        }
        IrTypeExpr::Primitive(IrPrimitive::String | IrPrimitive::StringWithFormat(_)) => {
            "response.text".to_string()
        }
        _ => "response.json()  # type: ignore[return-value]".to_string(),
    }
}

fn is_object_type(type_expr: &IrTypeExpr, ir: &IrSpec) -> bool {
    if let IrTypeExpr::Named(name) = type_expr {
        return is_object_schema(name, ir);
    }
    false
}

// ---------------------------------------------------------------------------
// Planning
// ---------------------------------------------------------------------------

struct OpPlan<'a> {
    op: &'a IrOperation,
    method_name: String,
    path_params: Vec<ParamBinding<'a>>,
    query_params: Vec<ParamBinding<'a>>,
    header_params: Vec<ParamBinding<'a>>,
    body: Option<BodyBinding>,
    typed_responses: Vec<TypedResponse>,
}

struct ParamBinding<'a> {
    param: &'a IrParameter,
    var_name: String,
}

struct BodyBinding {
    var_name: String,
    type_expr: IrTypeExpr,
    required: bool,
}

struct TypedResponse {
    type_expr: IrTypeExpr,
}

fn plan_operation<'a>(op: &'a IrOperation) -> OpPlan<'a> {
    let op_id = sanitize_operation_id(&op.operation_id, &op.method, &op.path);
    let method_name = op_id.to_snake_case();

    let mut used_names: HashSet<String> = HashSet::new();
    used_names.insert("self".to_string());

    let mut path_params = Vec::new();
    let mut query_params = Vec::new();
    let mut header_params = Vec::new();

    for p in &op.parameters {
        let var_name = unique_name(&python_param_name(&p.name), &mut used_names);
        let binding = ParamBinding { param: p, var_name };
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
        path_params,
        query_params,
        header_params,
        body,
        typed_responses,
    }
}

fn plan_body(b: &IrRequestBody, used_names: &mut HashSet<String>) -> Option<BodyBinding> {
    let t = pick_body_type(b)?;
    let var_name = unique_name("body", used_names);
    Some(BodyBinding {
        var_name,
        type_expr: t,
        required: b.required,
    })
}

fn plan_response(r: &IrResponse) -> Option<TypedResponse> {
    let t = pick_response_type(r)?;
    Some(TypedResponse { type_expr: t })
}

fn pick_body_type(body: &IrRequestBody) -> Option<IrTypeExpr> {
    body.content
        .get("application/json")
        .cloned()
        .or_else(|| body.content.values().next().cloned())
}

fn pick_response_type(r: &IrResponse) -> Option<IrTypeExpr> {
    r.content
        .get("application/json")
        .cloned()
        .or_else(|| r.content.values().next().cloned())
}

fn python_param_name(name: &str) -> String {
    let snake = name.to_snake_case();
    if snake.is_empty() {
        return "param".to_string();
    }
    match snake.as_str() {
        "and" | "as" | "assert" | "async" | "await" | "break" | "class" | "continue" | "def"
        | "del" | "elif" | "else" | "except" | "finally" | "for" | "from" | "global" | "if"
        | "import" | "in" | "is" | "lambda" | "nonlocal" | "not" | "or" | "pass" | "raise"
        | "return" | "try" | "while" | "with" | "yield" | "type" | "self" => {
            format!("{snake}_")
        }
        _ => snake,
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
