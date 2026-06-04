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
        let body = emit_api_file(tag, ops, module_path);
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

fn emit_api_file(tag: &str, ops: &[&IrOperation], module_path: &str) -> String {
    let struct_name = format!("{}API", tag.to_pascal_case());

    // Pre-plan each operation so we can build specs from the plans.
    let plans: Vec<OpPlan> = ops.iter().map(|op| plan_operation(op)).collect();

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
            pkgs.insert("encoding/json".to_string());
            pkgs.insert("fmt".to_string());
            pkgs.insert("bytes".to_string());
        }
        // io.ReadAll used in error handling (4xx branch)
        pkgs.insert("io".to_string());
        if has_typed_responses {
            pkgs.insert("encoding/json".to_string());
            pkgs.insert("fmt".to_string());
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
            if is_stringish_primitive(inner) {
                pkgs.insert("strings".to_string());
            } else {
                pkgs.insert("fmt".to_string());
            }
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
}

struct TypedResponse {
    status: String,
    field_name: String,
    go_type: String,
}

fn plan_operation<'a>(op: &'a IrOperation) -> OpPlan<'a> {
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
    let base_ty = go_type_str(&t);
    let go_type =
        if base_ty.starts_with('[') || base_ty.starts_with("map[") || base_ty.starts_with('*') {
            base_ty
        } else {
            format!("*{base_ty}")
        };
    let var_name = unique_name("body", used_names);
    Some(BodyBinding { var_name, go_type })
}

fn plan_response(r: &IrResponse) -> Option<TypedResponse> {
    let t = pick_response_type(r)?;
    let go_type = go_type_str(&t);
    Some(TypedResponse {
        status: r.status.clone(),
        field_name: response_field_name(&r.status),
        go_type,
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
    cb.add(&format!("path := \"{}\"", op.path), ());
    cb.add_line();
    for p in path_params {
        let placeholder = format!("{{{}}}", p.param.name);
        let value_expr = deref_if_pointer(&p.var_name, p.is_pointer);
        let stringified = render_value_as_string(&value_expr, &p.param.type_expr);
        cb.add(
            &format!("path = strings.Replace(path, \"{placeholder}\", {stringified}, 1)"),
            (),
        );
        cb.add_line();
    }

    // Query.
    let has_query = !query_params.is_empty();
    if has_query {
        cb.add("query := url.Values{}", ());
        cb.add_line();
        for p in query_params {
            let value_expr = deref_if_pointer(&p.var_name, p.is_pointer);
            let stringified = render_value_as_string(&value_expr, &p.param.type_expr);
            let set_line = format!("query.Set(\"{}\", {stringified})", p.param.name);
            if p.param.required || !p.is_pointer {
                cb.add(&set_line, ());
                cb.add_line();
            } else {
                cb.begin_control_flow(&format!("if {} != nil", p.var_name), ());
                cb.add(&set_line, ());
                cb.add_line();
                cb.end_control_flow();
            }
        }
    }

    // Body.
    if let Some(body) = body {
        cb.add("var bodyReader io.Reader", ());
        cb.add_line();
        cb.begin_control_flow(&format!("if {} != nil", body.var_name), ());
        cb.add(&format!("buf, err := json.Marshal({})", body.var_name), ());
        cb.add_line();
        cb.begin_control_flow("if err != nil", ());
        cb.add("return nil, fmt.Errorf(\"marshal body: %%w\", err)", ());
        cb.add_line();
        cb.end_control_flow();
        cb.add("bodyReader = bytes.NewReader(buf)", ());
        cb.add_line();
        cb.end_control_flow();
    }

    // Build request.
    let query_arg = if has_query { "query" } else { "nil" };
    let body_arg = if body.is_some() { "bodyReader" } else { "nil" };
    cb.add(
        &format!(
            "req, err := a.client.NewRequest(ctx, \"{}\", path, {query_arg}, {body_arg})",
            op.method.to_uppercase(),
        ),
        (),
    );
    cb.add_line();
    cb.begin_control_flow("if err != nil", ());
    cb.add("return nil, err", ());
    cb.add_line();
    cb.end_control_flow();

    // Headers.
    for p in header_params {
        let value_expr = deref_if_pointer(&p.var_name, p.is_pointer);
        let stringified = render_value_as_string(&value_expr, &p.param.type_expr);
        if p.param.required || !p.is_pointer {
            cb.add(
                &format!("req.Header.Set(\"{}\", {stringified})", p.param.name),
                (),
            );
            cb.add_line();
        } else {
            cb.begin_control_flow(&format!("if {} != nil", p.var_name), ());
            cb.add(
                &format!("req.Header.Set(\"{}\", {stringified})", p.param.name),
                (),
            );
            cb.add_line();
            cb.end_control_flow();
        }
    }

    if body.is_some() {
        cb.add("req.Header.Set(\"Content-Type\", \"application/json\")", ());
        cb.add_line();
    }
    cb.add("req.Header.Set(\"Accept\", \"application/json\")", ());
    cb.add_line();

    // Dispatch.
    cb.add("httpResp, err := a.client.Do(req)", ());
    cb.add_line();
    cb.begin_control_flow("if err != nil", ());
    cb.add("return nil, err", ());
    cb.add_line();
    cb.end_control_flow();
    cb.add("defer httpResp.Body.Close()", ());
    cb.add_line();
    cb.add_line();

    cb.add(
        &format!("resp := &{response_type}{{StatusCode: httpResp.StatusCode, Raw: httpResp}}"),
        (),
    );
    cb.add_line();

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
            cb.add("%L", emit_decode_into(&tr.field_name, &tr.go_type));
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
            cb.add("%L", emit_decode_into(&tr.field_name, &tr.go_type));
            cb.add(
                "return resp, &runtime.APIError{StatusCode: httpResp.StatusCode, Status: httpResp.Status}",
                (),
            );
            cb.add_line();
            cb.end_control_flow();
        }
        cb.begin_control_flow("if httpResp.StatusCode >= 400", ());
        cb.add("body, _ := io.ReadAll(httpResp.Body)", ());
        cb.add_line();
        cb.add(
            "return nil, &runtime.APIError{StatusCode: httpResp.StatusCode, Status: httpResp.Status, Body: body}",
            (),
        );
        cb.add_line();
        cb.end_control_flow();
        cb.add("%<", ());
        cb.end_control_flow();
    } else {
        cb.begin_control_flow("if httpResp.StatusCode >= 400", ());
        cb.add("body, _ := io.ReadAll(httpResp.Body)", ());
        cb.add_line();
        cb.add(
            "return nil, &runtime.APIError{StatusCode: httpResp.StatusCode, Status: httpResp.Status, Body: body}",
            (),
        );
        cb.add_line();
        cb.end_control_flow();
    }

    cb.add("return resp, nil", ());
    cb.build().expect("method body builds")
}

fn emit_decode_into(field: &str, go_ty: &str) -> CodeBlock {
    let (elem_ty, assignment) = if go_ty.starts_with('[') || go_ty.starts_with("map[") {
        (go_ty.to_string(), format!("resp.{field} = payload"))
    } else {
        (
            go_ty.trim_start_matches('*').to_string(),
            format!("resp.{field} = &payload"),
        )
    };
    sigil_quote!(GoLang {
        $>
        var $L("payload @{elem_ty}")
        if err := json.NewDecoder(httpResp.Body).Decode(&payload); err != nil {
            return nil, fmt.Errorf("decode response: %w", err)
        }
        $L(assignment)
        $<
    })
    .expect("decode body builds")
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
                format!("fmt.Sprintf(\"%v\", {value_expr})")
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
