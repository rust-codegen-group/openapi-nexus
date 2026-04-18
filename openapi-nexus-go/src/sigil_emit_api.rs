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
//! We build each file as a plain string here — the method body is imperative
//! Go (path templating, query building, JSON marshal/unmarshal, status switch)
//! which doesn't fit sigil-stitch's structural builders cleanly. Structural
//! emission can be layered back in when/if import-tracking across files
//! becomes load-bearing.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use heck::{ToLowerCamelCase, ToPascalCase, ToSnakeCase};
use openapi_nexus_core::traits::file_writer::FileInfo;
use openapi_nexus_ir::types::{
    IrOperation, IrParameter, IrPrimitive, IrRequestBody, IrResponse, IrSpec, IrTypeExpr,
    ParameterLocation,
};

/// Generate every API file from the IR.
pub fn generate_api_files(
    ir: &IrSpec,
    _module_path: &str,
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
        let body = emit_api_file(tag, ops);
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

fn emit_api_file(tag: &str, ops: &[&IrOperation]) -> String {
    let struct_name = format!("{}API", tag.to_pascal_case());

    // Pre-plan each operation so we can compute imports from actual usage.
    let plans: Vec<OpPlan> = ops.iter().map(|op| plan_operation(op)).collect();

    let imports = compute_imports(&plans);

    let mut out = String::new();
    out.push_str("package apis\n\n");
    out.push_str(&render_imports(&imports));
    out.push('\n');

    out.push_str(&format!(
        "// {struct_name} groups operations under the \"{tag}\" tag.\n"
    ));
    out.push_str(&format!("type {struct_name} struct {{\n"));
    out.push_str("\tclient *runtime.Client\n");
    out.push_str("}\n\n");

    out.push_str(&format!(
        "// New{struct_name} constructs a {struct_name} bound to client.\n"
    ));
    out.push_str(&format!(
        "func New{struct_name}(client *runtime.Client) *{struct_name} {{\n\treturn &{struct_name}{{client: client}}\n}}\n",
    ));

    for plan in &plans {
        out.push('\n');
        out.push_str(&emit_operation(&struct_name, plan));
    }

    out
}

// ---------------------------------------------------------------------------
// Planning: resolve parameter names / types so emission is deterministic
// ---------------------------------------------------------------------------

struct OpPlan<'a> {
    op: &'a IrOperation,
    method_name: String,
    response_type: String,
    /// Path parameters, in the order they appear as method args.
    path_params: Vec<ParamBinding<'a>>,
    /// Query parameters.
    query_params: Vec<ParamBinding<'a>>,
    /// Header parameters.
    header_params: Vec<ParamBinding<'a>>,
    /// Optional request body with its final Go variable name + type.
    body: Option<BodyBinding>,
    /// Typed responses with resolved Go types.
    typed_responses: Vec<TypedResponse>,
}

struct ParamBinding<'a> {
    param: &'a IrParameter,
    /// Final Go identifier for the function argument.
    var_name: String,
    /// Go type as it appears in the method signature (may be pointer for optional).
    go_type: String,
    /// Whether `var_name` is a pointer type that must be dereferenced to get the value.
    is_pointer: bool,
}

struct BodyBinding {
    var_name: String,
    /// Go type in the method signature. Always a pointer for Named/primitive
    /// types; bare for slice/map.
    go_type: String,
}

struct TypedResponse {
    status: String,
    field_name: String,
    /// Go type (e.g. `models.Foo`, `[]string`, etc.).
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
            ParameterLocation::Cookie => {
                // Cookies are rare; treat like headers (caller sets via Cookie header).
                header_params.push(binding);
            }
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
    if p.required {
        let pointer = base.starts_with('*');
        (base, pointer)
    } else if base.starts_with('*') || base.starts_with('[') || base.starts_with("map[") {
        // Already nilable — leave as is.
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
// Import tracking
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Imports {
    bytes: bool,
    context: bool,
    encoding_json: bool,
    fmt: bool,
    io: bool,
    net_http: bool,
    net_url: bool,
    strconv: bool,
    strings: bool,
    models: bool,
}

fn compute_imports(plans: &[OpPlan<'_>]) -> Imports {
    let mut imp = Imports {
        context: true,
        io: true,
        net_http: true,
        ..Imports::default()
    };

    for plan in plans {
        if !plan.path_params.is_empty() {
            imp.strings = true;
        }
        if !plan.query_params.is_empty() {
            imp.net_url = true;
        }
        for p in plan
            .path_params
            .iter()
            .chain(&plan.query_params)
            .chain(&plan.header_params)
        {
            if needs_strconv(&p.param.type_expr) {
                imp.strconv = true;
            }
            if refs_models(&p.param.type_expr) {
                imp.models = true;
            }
        }
        if plan.body.is_some() {
            imp.bytes = true;
            imp.encoding_json = true;
            imp.fmt = true;
        }
        if let Some(body) = &plan.body
            && body.go_type.contains("models.")
        {
            imp.models = true;
        }
        if !plan.typed_responses.is_empty() {
            imp.encoding_json = true;
            imp.fmt = true;
        }
        for tr in &plan.typed_responses {
            if tr.go_type.contains("models.") {
                imp.models = true;
            }
        }
    }

    imp
}

fn needs_strconv(t: &IrTypeExpr) -> bool {
    match t {
        IrTypeExpr::Primitive(
            IrPrimitive::Boolean
            | IrPrimitive::Integer
            | IrPrimitive::IntegerWithFormat(_)
            | IrPrimitive::Number
            | IrPrimitive::NumberWithFormat(_),
        ) => true,
        IrTypeExpr::Nullable(inner) => needs_strconv(inner),
        _ => false,
    }
}

fn refs_models(t: &IrTypeExpr) -> bool {
    match t {
        IrTypeExpr::Named(_) => true,
        IrTypeExpr::Array(inner) | IrTypeExpr::Map(inner) | IrTypeExpr::Nullable(inner) => {
            refs_models(inner)
        }
        IrTypeExpr::Union(members) => members.iter().any(refs_models),
        _ => false,
    }
}

fn render_imports(imp: &Imports) -> String {
    let mut stdlib = Vec::new();
    if imp.bytes {
        stdlib.push("\"bytes\"");
    }
    if imp.context {
        stdlib.push("\"context\"");
    }
    if imp.encoding_json {
        stdlib.push("\"encoding/json\"");
    }
    if imp.fmt {
        stdlib.push("\"fmt\"");
    }
    if imp.io {
        stdlib.push("\"io\"");
    }
    if imp.net_http {
        stdlib.push("\"net/http\"");
    }
    if imp.net_url {
        stdlib.push("\"net/url\"");
    }
    if imp.strconv {
        stdlib.push("\"strconv\"");
    }
    if imp.strings {
        stdlib.push("\"strings\"");
    }

    let mut project = Vec::new();
    if imp.models {
        project.push("\"example.com/sdk/models\"");
    }
    project.push("\"example.com/sdk/runtime\"");

    let mut out = String::from("import (\n");
    for s in &stdlib {
        out.push_str(&format!("\t{s}\n"));
    }
    if !stdlib.is_empty() {
        out.push('\n');
    }
    for s in &project {
        out.push_str(&format!("\t{s}\n"));
    }
    out.push_str(")\n");
    out
}

// ---------------------------------------------------------------------------
// Per-operation emission
// ---------------------------------------------------------------------------

fn emit_operation(struct_name: &str, plan: &OpPlan<'_>) -> String {
    let OpPlan {
        op,
        method_name,
        response_type,
        ..
    } = plan;

    let mut out = String::new();

    out.push_str(&emit_response_struct(response_type, plan));
    out.push('\n');

    if let Some(summary) = &op.summary {
        out.push_str(&format!("// {method_name} — {summary}\n"));
    } else {
        out.push_str(&format!(
            "// {method_name} calls {} {}.\n",
            op.method.to_uppercase(),
            op.path,
        ));
    }
    if let Some(desc) = &op.description {
        out.push_str("//\n");
        for line in desc.lines() {
            out.push_str(&format!("// {line}\n"));
        }
    }

    let mut params = vec!["ctx context.Context".to_string()];
    for p in plan
        .path_params
        .iter()
        .chain(&plan.query_params)
        .chain(&plan.header_params)
    {
        params.push(format!("{} {}", p.var_name, p.go_type));
    }
    if let Some(body) = &plan.body {
        params.push(format!("{} {}", body.var_name, body.go_type));
    }

    out.push_str(&format!(
        "func (a *{struct_name}) {method_name}(\n\t{},\n) (*{response_type}, error) {{\n",
        params.join(",\n\t"),
    ));

    out.push_str(&emit_method_body(plan));
    out.push_str("}\n");
    out
}

fn emit_response_struct(response_type: &str, plan: &OpPlan<'_>) -> String {
    let mut out =
        format!("// {response_type} carries the response from the corresponding operation.\n");
    out.push_str(&format!("type {response_type} struct {{\n"));
    out.push_str("\tStatusCode int\n");
    out.push_str("\tRaw        *http.Response\n");
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for tr in &plan.typed_responses {
        if !seen.insert(tr.field_name.clone()) {
            continue;
        }
        let ptr = pointerize(&tr.go_type);
        out.push_str(&format!("\t{} {}\n", tr.field_name, ptr));
    }
    out.push_str("}\n");
    out
}

/// Wrap bare struct types in `*`; leave slices/maps/pointers alone.
fn pointerize(go_ty: &str) -> String {
    if go_ty.starts_with('[') || go_ty.starts_with('*') || go_ty.starts_with("map[") {
        go_ty.to_string()
    } else {
        format!("*{go_ty}")
    }
}

fn emit_method_body(plan: &OpPlan<'_>) -> String {
    let OpPlan {
        op,
        response_type,
        path_params,
        query_params,
        header_params,
        body,
        ..
    } = plan;

    let mut out = String::new();

    // Path.
    out.push_str(&format!("\tpath := \"{}\"\n", op.path));
    for p in path_params {
        let placeholder = format!("{{{}}}", p.param.name);
        let value_expr = deref_if_pointer(&p.var_name, p.is_pointer);
        let stringified = render_value_as_string(&value_expr, &p.param.type_expr);
        out.push_str(&format!(
            "\tpath = strings.Replace(path, \"{placeholder}\", {stringified}, 1)\n",
        ));
    }

    // Query.
    let has_query = !query_params.is_empty();
    if has_query {
        out.push_str("\tquery := url.Values{}\n");
        for p in query_params {
            let value_expr = deref_if_pointer(&p.var_name, p.is_pointer);
            let stringified = render_value_as_string(&value_expr, &p.param.type_expr);
            let set = format!("\tquery.Set(\"{}\", {stringified})\n", p.param.name,);
            if p.param.required || !p.is_pointer {
                out.push_str(&set);
            } else {
                out.push_str(&format!("\tif {} != nil {{\n\t", p.var_name));
                out.push_str(&set);
                out.push_str("\t}\n");
            }
        }
    }

    // Body.
    if let Some(body) = body {
        out.push_str("\tvar bodyReader io.Reader\n");
        out.push_str(&format!("\tif {} != nil {{\n", body.var_name));
        out.push_str(&format!(
            "\t\tbuf, err := json.Marshal({})\n",
            body.var_name,
        ));
        out.push_str(
            "\t\tif err != nil {\n\t\t\treturn nil, fmt.Errorf(\"marshal body: %w\", err)\n\t\t}\n",
        );
        out.push_str("\t\tbodyReader = bytes.NewReader(buf)\n");
        out.push_str("\t}\n");
    }

    // Build request.
    let query_arg = if has_query { "query" } else { "nil" };
    let body_arg = if body.is_some() { "bodyReader" } else { "nil" };
    out.push_str(&format!(
        "\treq, err := a.client.NewRequest(ctx, \"{}\", path, {query_arg}, {body_arg})\n",
        op.method.to_uppercase(),
    ));
    out.push_str("\tif err != nil {\n\t\treturn nil, err\n\t}\n");

    // Headers.
    for p in header_params {
        let value_expr = deref_if_pointer(&p.var_name, p.is_pointer);
        let stringified = render_value_as_string(&value_expr, &p.param.type_expr);
        if p.param.required || !p.is_pointer {
            out.push_str(&format!(
                "\treq.Header.Set(\"{}\", {stringified})\n",
                p.param.name,
            ));
        } else {
            out.push_str(&format!(
                "\tif {} != nil {{\n\t\treq.Header.Set(\"{}\", {stringified})\n\t}}\n",
                p.var_name, p.param.name,
            ));
        }
    }

    if body.is_some() {
        out.push_str("\treq.Header.Set(\"Content-Type\", \"application/json\")\n");
    }
    out.push_str("\treq.Header.Set(\"Accept\", \"application/json\")\n");

    // Dispatch.
    out.push_str("\thttpResp, err := a.client.Do(req)\n");
    out.push_str("\tif err != nil {\n\t\treturn nil, err\n\t}\n");
    out.push_str("\tdefer httpResp.Body.Close()\n\n");

    out.push_str(&format!(
        "\tresp := &{response_type}{{StatusCode: httpResp.StatusCode, Raw: httpResp}}\n",
    ));

    if !plan.typed_responses.is_empty() {
        out.push_str("\tswitch httpResp.StatusCode {\n");
        for tr in &plan.typed_responses {
            // We only emit numeric status cases; `default` and range statuses
            // like "2XX" fall through to the default arm.
            let Some(code) = tr.status.parse::<u16>().ok() else {
                continue;
            };
            out.push_str(&format!("\tcase {code}:\n"));
            out.push_str(&emit_decode_into(&tr.field_name, &tr.go_type));
        }
        out.push_str("\tdefault:\n");
        out.push_str("\t\tif httpResp.StatusCode >= 400 {\n");
        out.push_str("\t\t\tbody, _ := io.ReadAll(httpResp.Body)\n");
        out.push_str("\t\t\treturn nil, &runtime.APIError{StatusCode: httpResp.StatusCode, Status: httpResp.Status, Body: body}\n");
        out.push_str("\t\t}\n");
        out.push_str("\t}\n");
    } else {
        out.push_str("\tif httpResp.StatusCode >= 400 {\n");
        out.push_str("\t\tbody, _ := io.ReadAll(httpResp.Body)\n");
        out.push_str("\t\treturn nil, &runtime.APIError{StatusCode: httpResp.StatusCode, Status: httpResp.Status, Body: body}\n");
        out.push_str("\t}\n");
    }

    out.push_str("\treturn resp, nil\n");
    out
}

fn emit_decode_into(field: &str, go_ty: &str) -> String {
    let (elem_ty, assignment) = if go_ty.starts_with('[') || go_ty.starts_with("map[") {
        (go_ty.to_string(), format!("resp.{field} = payload\n"))
    } else {
        (
            go_ty.trim_start_matches('*').to_string(),
            format!("resp.{field} = &payload\n"),
        )
    };
    let mut out = String::new();
    out.push_str(&format!("\t\tvar payload {elem_ty}\n"));
    out.push_str("\t\tif err := json.NewDecoder(httpResp.Body).Decode(&payload); err != nil {\n");
    out.push_str("\t\t\treturn nil, fmt.Errorf(\"decode response: %w\", err)\n");
    out.push_str("\t\t}\n");
    out.push_str("\t\t");
    out.push_str(&assignment);
    out
}

fn deref_if_pointer(var: &str, is_pointer: bool) -> String {
    if is_pointer {
        format!("*{var}")
    } else {
        var.to_string()
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
// Param → Go identifier, value → Go string expression
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

/// Render a value expression as a Go `string`. Expects `value_expr` to already
/// be the pointer-dereferenced form for optional vars.
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
            // Named refs in path/query/header are almost always string enums.
            format!("string({value_expr})")
        }
        _ => format!("fmt.Sprintf(\"%v\", {value_expr})"),
    }
}

// ---------------------------------------------------------------------------
// Type-expr → Go type
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
