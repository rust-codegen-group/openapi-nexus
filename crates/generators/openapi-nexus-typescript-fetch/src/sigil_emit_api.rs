//! Sigil-stitch emit for TypeScript API class files.
//!
//! Produces one `FileSpec<TypeScript>` per tag containing:
//! - request interfaces (one per operation that has parameters)
//! - `{Tag}ApiInterface` — method arrow-function signatures (emitted as a raw
//!   `CodeBlock` so `%T` slots can carry import tracking for every type ref)
//! - `{Tag}Api` class extending `BaseAPI` with constructor + real Raw +
//!   convenience methods
//!
//! # Import tracking
//!
//! Every named reference (model types, runtime wrappers) is routed through a
//! structural [`TypeName`] (via `importable` for runtime-value symbols like
//! `BaseAPI` / `JSONApiResponse`, `importable_type` for pure TS types like
//! `Configuration`). Object-literal fragments like `{ status: 200 }` stay as
//! `TypeName::raw`. Method bodies use `CodeBlock::add(fmt, [%T, %L, ...])` so
//! sigil resolves all imports in one pass.

use std::collections::{BTreeMap, BTreeSet};

use heck::{ToLowerCamelCase as _, ToPascalCase as _};
use openapi_nexus_core::traits::file_writer::FileInfo;
use openapi_nexus_ir::types::{
    IrOperation, IrParameter, IrPrimitive, IrRequestBody, IrResponse, IrSpec, IrTypeExpr,
    ParameterLocation as IrParameterLocation,
};
use sigil_stitch::code_block::{Arg, CodeBlock};
use sigil_stitch::lang::typescript::TypeScript;
use sigil_stitch::spec::field_spec::FieldSpec;
use sigil_stitch::spec::file_spec::FileSpec;
use sigil_stitch::spec::fun_spec::FunSpec;
use sigil_stitch::spec::modifiers::{TypeKind, Visibility};
use sigil_stitch::spec::parameter_spec::ParameterSpec;
use sigil_stitch::spec::type_spec::TypeSpec;
use sigil_stitch::type_name::TypeName;

const RUNTIME_MOD: &str = "../runtime/runtime";

/// Lower every tag in the IR spec into a sigil-rendered API class `FileInfo`.
pub fn generate_api_files(ir: &IrSpec) -> Result<Vec<FileInfo>, String> {
    let header = crate::project_files::render_file_header(&ir.info);
    let by_tag = group_by_tag(&ir.operations);

    let mut files = Vec::with_capacity(by_tag.len());
    for (tag, ops) in &by_tag {
        let file_spec = emit_api_file(tag, ops)?;
        let body = file_spec
            .render(100)
            .map_err(|e| format!("sigil_emit_api: render {tag}: {e}"))?;
        let filename = format!("{}Api.ts", tag.to_pascal_case());
        let content = format!("{header}{body}");
        files.push(FileInfo::api(filename, content));
    }

    Ok(files)
}

/// Exported symbols from a single `{Tag}Api.ts` file, split into type-only
/// and value entries so the `apis/index.ts` barrel can emit
/// `export type { ... }` and `export { ClassName }` separately.
#[derive(Debug, Clone)]
pub struct ApiFileExports {
    pub filename_base: String,
    pub type_names: Vec<String>,
    pub value_names: Vec<String>,
}

/// Enumerate, per tag, the symbols that [`generate_api_files`] emits so
/// callers can build a named-export barrel instead of `export *`.
///
/// The ordering mirrors emission: per-op request interface (when present),
/// per-op raw-response alias, then `{Tag}ApiInterface`; the class goes into
/// `value_names`.
pub fn collect_api_file_exports(ir: &IrSpec) -> Vec<ApiFileExports> {
    let by_tag = group_by_tag(&ir.operations);
    let mut out = Vec::with_capacity(by_tag.len());
    for (tag, ops) in &by_tag {
        let class_name = format!("{}Api", tag.to_pascal_case());
        let interface_name = format!("{}Interface", class_name);

        let mut type_names = Vec::new();
        for op in ops {
            if !op.parameters.is_empty() || op.request_body.is_some() {
                type_names.push(format!(
                    "Api{}Request",
                    op.operation_id.to_lower_camel_case().to_pascal_case()
                ));
            }
            type_names.push(raw_response_alias_name(op));
        }
        type_names.push(interface_name);

        out.push(ApiFileExports {
            filename_base: class_name.clone(),
            type_names,
            value_names: vec![class_name],
        });
    }
    out
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

fn emit_api_file(tag: &str, ops: &[&IrOperation]) -> Result<FileSpec<TypeScript>, String> {
    let class_name = format!("{}Api", tag.to_pascal_case());
    let interface_name = format!("{}Interface", class_name);

    let mut fb = FileSpec::<TypeScript>::builder(&format!("{}.ts", class_name));

    // Request interfaces — one per op that has at least one parameter / body.
    for op in ops {
        if let Some(req_iface) = build_request_interface(op) {
            fb.add_type(req_iface);
        }
    }

    // Per-operation raw response type aliases — emit each union member on its
    // own line so readers can scan each `Wrapper & { status: N }` pair without
    // the pretty printer splitting intersections across lines.
    fb.add_code(build_response_aliases_block(ops)?);

    // ApiInterface — emit as a raw CodeBlock so `%T` slots propagate imports
    // for every arrow-function parameter and return type. (TypeSpec with
    // FieldSpec arrow-function fields can't carry structural TypeName for the
    // whole `(p: T) => R` shape because sigil's `TypeName::function` doesn't
    // emit parameter names.)
    fb.add_code(build_api_interface_block(&interface_name, ops)?);

    // ApiClass stays structural so modifiers / docs / constructor delegation
    // use sigil's machinery.
    fb.add_type(build_api_class(&class_name, &interface_name, ops)?);

    fb.build()
        .map_err(|e| format!("sigil_emit_api: FileSpec build {tag}: {e}"))
}

/// Name of the exported type alias holding the raw (wrapped) response union
/// for a given operation. E.g. `updatePet` → `UpdatePetRawResponse`.
fn raw_response_alias_name(op: &IrOperation) -> String {
    format!(
        "{}RawResponse",
        op.operation_id.to_lower_camel_case().to_pascal_case()
    )
}

/// Emit one `export type {OpId}RawResponse = | A | B | C;` block per op.
/// Each member is a `%T` slot so imports still flow through the collector,
/// and each sits on its own line so intersections (`Wrapper & { status: N }`)
/// stay intact.
fn build_response_aliases_block(ops: &[&IrOperation]) -> Result<CodeBlock<TypeScript>, String> {
    let mut cb = CodeBlock::<TypeScript>::builder();
    for op in ops {
        let alias = raw_response_alias_name(op);
        let members = raw_response_members(op);
        if members.len() == 1 {
            cb.add(
                &format!("export type {alias} = %T;\n\n"),
                vec![Arg::TypeName(members.into_iter().next().unwrap())],
            );
        } else {
            cb.add(&format!("export type {alias} =\n"), vec![]);
            for (i, member) in members.into_iter().enumerate() {
                let sep = if i == 0 { "  | " } else { "\n  | " };
                cb.add(&format!("{sep}%T"), vec![Arg::TypeName(member)]);
            }
            cb.add(";\n\n", vec![]);
        }
    }
    cb.build()
        .map_err(|e| format!("sigil_emit_api: response aliases block: {e}"))
}

/// Compute the deduplicated list of union members for an operation's raw
/// response type (wrapper intersected with status literal).
fn raw_response_members(op: &IrOperation) -> Vec<TypeName<TypeScript>> {
    let mut members: Vec<TypeName<TypeScript>> = Vec::new();
    let mut any_body = false;
    let mut has_default = false;

    for resp in &op.responses {
        if resp.status.eq_ignore_ascii_case("default") {
            has_default = true;
        }
        let kind = classify_response(resp);
        if !matches!(kind, ResponseKind::None) {
            any_body = true;
        }
        members.push(raw_response_member(resp, &kind));
    }
    if !has_default {
        members.push(fallback_member(any_body));
    }

    if members.is_empty() {
        return vec![rt_value("VoidApiResponse")];
    }
    dedup_union_members(members)
}

// ============================================================================
// Request interfaces
// ============================================================================

fn build_request_interface(op: &IrOperation) -> Option<TypeSpec<TypeScript>> {
    let has_params = !op.parameters.is_empty() || op.request_body.is_some();
    if !has_params {
        return None;
    }

    let method_base = op.operation_id.to_lower_camel_case();
    let interface_name = format!("Api{}Request", method_base.to_pascal_case());
    let names = resolve_param_names(op);

    let mut tb = TypeSpec::<TypeScript>::builder(&interface_name, TypeKind::Interface);
    tb.visibility(Visibility::Public);

    for param in &op.parameters {
        if matches!(param.location, IrParameterLocation::Cookie) {
            continue;
        }
        tb.add_field(build_param_field(param, &resolved_param(&names, param)));
    }
    if let Some(rb) = &op.request_body {
        tb.add_field(build_body_field(rb, &resolved_body(&names)));
    }

    tb.build().ok()
}

fn build_param_field(param: &IrParameter, name: &str) -> FieldSpec<TypeScript> {
    let ty = type_expr_to_typename(&param.type_expr);
    let mut fb = FieldSpec::<TypeScript>::builder(name, ty);
    if !param.required {
        fb.is_optional();
    }
    if let Some(desc) = &param.description {
        fb.doc(desc);
    }
    fb.build().expect("FieldSpec builds")
}

/// Choose the preferred media type from a request body, matching
/// `build_body_field`'s type-selection logic so the emitted `Content-Type`
/// agrees with the schema we typed the body as. Prefers `application/json`
/// if declared, otherwise the first media type in spec order.
fn preferred_request_media_type(rb: &IrRequestBody) -> Option<&str> {
    if rb.content.contains_key("application/json") {
        Some("application/json")
    } else {
        rb.content.keys().next().map(String::as_str)
    }
}

fn build_body_field(rb: &IrRequestBody, name: &str) -> FieldSpec<TypeScript> {
    let ty = preferred_request_media_type(rb)
        .and_then(|mt| rb.content.get(mt))
        .map(type_expr_to_typename)
        .unwrap_or_else(|| TypeName::primitive("unknown"));
    let mut fb = FieldSpec::<TypeScript>::builder(name, ty);
    if !rb.required {
        fb.is_optional();
    }
    if let Some(desc) = &rb.description {
        fb.doc(desc);
    }
    fb.build().expect("FieldSpec builds")
}

// ============================================================================
// ApiInterface (raw CodeBlock with %T slots)
// ============================================================================

fn build_api_interface_block(
    interface_name: &str,
    ops: &[&IrOperation],
) -> Result<CodeBlock<TypeScript>, String> {
    let mut cb = CodeBlock::<TypeScript>::builder();
    cb.add(&format!("export interface {} {{\n", interface_name), vec![]);

    for op in ops {
        let method_base = op.operation_id.to_lower_camel_case();
        let raw_name = format!("{}Raw", method_base);

        cb.add(&format!("  {}: ", raw_name), vec![]);
        emit_arrow_signature(&mut cb, op, raw_return_type(op));
        cb.add(";\n", vec![]);

        cb.add(&format!("  {}: ", method_base), vec![]);
        emit_arrow_signature(
            &mut cb,
            op,
            TypeName::generic(
                TypeName::primitive("Promise"),
                vec![convenience_body_type(op)],
            ),
        );
        cb.add(";\n", vec![]);
    }

    cb.add("}", vec![]);
    cb.build()
        .map_err(|e| format!("sigil_emit_api: ApiInterface {interface_name}: {e}"))
}

/// Append `(requestParameters: ApiXRequest, initOverrides?: RequestInit | InitOverrideFunction) => %T`
/// onto the given block, using `%T` slots for every named type so imports
/// flow through.
fn emit_arrow_signature(
    cb: &mut sigil_stitch::code_block::CodeBlockBuilder<TypeScript>,
    op: &IrOperation,
    return_ty: TypeName<TypeScript>,
) {
    let mut parts: Vec<String> = Vec::new();
    let mut args: Vec<Arg<TypeScript>> = Vec::new();

    if operation_has_params(op) {
        let iface = format!(
            "Api{}Request",
            op.operation_id.to_lower_camel_case().to_pascal_case()
        );
        parts.push(format!("requestParameters: {}", iface));
    }
    // initOverrides?: RequestInit | InitOverrideFunction
    parts.push("initOverrides?: RequestInit | %T".to_string());
    args.push(Arg::TypeName(TypeName::importable_type(
        RUNTIME_MOD,
        "InitOverrideFunction",
    )));

    cb.add(&format!("({}) => %T", parts.join(", ")), {
        let mut merged = args;
        merged.push(Arg::TypeName(return_ty));
        merged
    });
}

// ============================================================================
// ApiClass
// ============================================================================

fn build_api_class(
    class_name: &str,
    interface_name: &str,
    ops: &[&IrOperation],
) -> Result<TypeSpec<TypeScript>, String> {
    let mut tb = TypeSpec::<TypeScript>::builder(class_name, TypeKind::Class);
    tb.visibility(Visibility::Public);
    tb.extends(rt_value("BaseAPI"));
    tb.implements(TypeName::raw(interface_name));

    tb.add_method(build_constructor());

    for op in ops {
        tb.add_method(build_raw_method(op)?);
        tb.add_method(build_convenience_method(op)?);
    }

    tb.build()
        .map_err(|e| format!("sigil_emit_api: ApiClass {class_name}: {e}"))
}

fn build_constructor() -> FunSpec<TypeScript> {
    let mut fb = FunSpec::<TypeScript>::builder("constructor");
    fb.is_constructor();
    fb.doc("Initialize the API client");

    fb.add_param(
        ParameterSpec::<TypeScript>::builder("configuration?", rt_type("Configuration"))
            .build()
            .expect("ParameterSpec builds"),
    );

    let mut body = CodeBlock::<TypeScript>::builder();
    body.add(
        "super(configuration ?? %T);",
        vec![Arg::TypeName(rt_value("DefaultConfig"))],
    );
    fb.body(body.build().expect("CodeBlock builds"));

    fb.build().expect("Constructor FunSpec builds")
}

// ============================================================================
// Raw method — full request body with parameter handling and response dispatch
// ============================================================================

fn build_raw_method(op: &IrOperation) -> Result<FunSpec<TypeScript>, String> {
    let method_base = op.operation_id.to_lower_camel_case();
    let method_name = format!("{}Raw", method_base);

    let mut fb = FunSpec::<TypeScript>::builder(&method_name);
    fb.is_async();

    for param in method_param_specs(op) {
        fb.add_param(param);
    }
    fb.returns(raw_return_type(op));

    let mut body = CodeBlock::<TypeScript>::builder();
    emit_required_param_checks(&mut body, op, &method_name);
    emit_url_path(&mut body, op);
    emit_query_params(&mut body, op);
    emit_headers(&mut body, op);
    emit_request_body(&mut body, op);
    emit_make_request(&mut body, op, op.request_body.is_some());
    emit_response_handler(&mut body, op);

    fb.body(body.build().map_err(|e| format!("body build: {e}"))?);

    fb.build()
        .map_err(|e| format!("sigil_emit_api: raw method {method_name}: {e}"))
}

/// True if `s` is shaped like a JS identifier — safe to use with dot access.
/// ES5+ permits reserved words (`class`, `if`, ...) after a dot as property
/// names, and ESLint `dot-notation` accepts them, so shape is sufficient.
fn is_js_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_' || first == '$') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// Render `requestParameters.{key}` when `key` is a valid identifier,
/// otherwise `requestParameters['{key}']`.
fn request_parameters_access(key: &str) -> String {
    if is_js_identifier(key) {
        format!("requestParameters.{key}")
    } else {
        format!("requestParameters['{key}']")
    }
}

fn emit_required_param_checks(
    cb: &mut sigil_stitch::code_block::CodeBlockBuilder<TypeScript>,
    op: &IrOperation,
    method_name: &str,
) {
    let names = resolve_param_names(op);
    let required_named = op
        .parameters
        .iter()
        .filter(|p| p.required && !matches!(p.location, IrParameterLocation::Cookie))
        .map(|p| resolved_param(&names, p))
        .collect::<Vec<_>>();
    let mut all_required = required_named;
    if let Some(rb) = &op.request_body
        && rb.required
    {
        all_required.push(resolved_body(&names));
    }

    for pname in all_required {
        let access = request_parameters_access(&pname);
        cb.add(
            &format!(
                "if ({0} === undefined || {0} === null) {{\n  throw new %T(\n    '{1}',\n    'Required parameter \"{1}\" was null or undefined when calling {2}().'\n  );\n}}\n",
                access, pname, method_name
            ),
            vec![Arg::TypeName(rt_value("RequiredError"))],
        );
    }
}

fn emit_url_path(
    cb: &mut sigil_stitch::code_block::CodeBlockBuilder<TypeScript>,
    op: &IrOperation,
) {
    cb.add("// Build path with path parameters\n", vec![]);
    let has_path_params = op
        .parameters
        .iter()
        .any(|p| matches!(p.location, IrParameterLocation::Path));
    let binding = if has_path_params { "let" } else { "const" };
    cb.add(&format!("{} urlPath = `{}`;\n", binding, op.path), vec![]);

    let names = resolve_param_names(op);
    for p in op
        .parameters
        .iter()
        .filter(|p| matches!(p.location, IrParameterLocation::Path))
    {
        let resolved = resolved_param(&names, p);
        let original = &p.name;
        let access = request_parameters_access(&resolved);
        // The backtick template and the ${...} must both survive into TS output.
        cb.add(
            &format!(
                "urlPath = urlPath.replace(`{{{}}}`, encodeURIComponent(String({})));\n",
                original, access
            ),
            vec![],
        );
    }
}

fn emit_query_params(
    cb: &mut sigil_stitch::code_block::CodeBlockBuilder<TypeScript>,
    op: &IrOperation,
) {
    cb.add("// Build query parameters\n", vec![]);
    cb.add(
        "const queryParameters: %T = {};\n",
        vec![Arg::TypeName(rt_type("HTTPQuery"))],
    );
    let names = resolve_param_names(op);
    for p in op
        .parameters
        .iter()
        .filter(|p| matches!(p.location, IrParameterLocation::Query))
    {
        let resolved = resolved_param(&names, p);
        let access = request_parameters_access(&resolved);
        cb.add(
            &format!(
                "if ({0} !== undefined) {{\n  queryParameters['{1}'] = {0};\n}}\n",
                access, p.name
            ),
            vec![],
        );
    }
}

fn emit_headers(cb: &mut sigil_stitch::code_block::CodeBlockBuilder<TypeScript>, op: &IrOperation) {
    cb.add("// Build headers\n", vec![]);
    cb.add(
        "const headerParameters: Record<string, string> = {\n",
        vec![],
    );
    if let Some(rb) = &op.request_body
        && let Some(media_type) = preferred_request_media_type(rb)
    {
        cb.add(&format!("  'Content-Type': '{}',\n", media_type), vec![]);
    }
    cb.add("};\n\n", vec![]);
    let names = resolve_param_names(op);
    for p in op
        .parameters
        .iter()
        .filter(|p| matches!(p.location, IrParameterLocation::Header))
    {
        let resolved = resolved_param(&names, p);
        let access = request_parameters_access(&resolved);
        cb.add(
            &format!(
                "if ({0} !== undefined) {{\n  headerParameters['{1}'] = String({0});\n}}\n",
                access, p.name
            ),
            vec![],
        );
    }
}

fn emit_request_body(
    cb: &mut sigil_stitch::code_block::CodeBlockBuilder<TypeScript>,
    op: &IrOperation,
) {
    if op.request_body.is_some() {
        cb.add("// Prepare request body\n", vec![]);
        let names = resolve_param_names(op);
        let body_name = resolved_body(&names);
        let access = request_parameters_access(&body_name);
        cb.add(&format!("const requestBody = {};\n", access), vec![]);
    }
}

fn emit_make_request(
    cb: &mut sigil_stitch::code_block::CodeBlockBuilder<TypeScript>,
    op: &IrOperation,
    has_body: bool,
) {
    let method = op.method.to_uppercase();
    let body_expr = if has_body { "requestBody" } else { "undefined" };
    cb.add("// Make request\n", vec![]);
    cb.add(
        &format!(
            "const response = await this.request({{\n    path: urlPath,\n    method: '{}',\n    headers: headerParameters,\n    query: queryParameters,\n    body: {},\n}}, initOverrides);\n\n",
            method, body_expr
        ),
        vec![],
    );
}

fn emit_response_handler(
    cb: &mut sigil_stitch::code_block::CodeBlockBuilder<TypeScript>,
    op: &IrOperation,
) {
    cb.add("// Handle responses\n", vec![]);

    // Classify: conditional (explicit 2xx/4xx status), default (the "default"
    // response key), fallback (we synthesize one when no default is given).
    let mut conditional: Vec<(String, &IrResponse)> = Vec::new();
    let mut default: Option<&IrResponse> = None;
    for resp in &op.responses {
        if resp.status.eq_ignore_ascii_case("default") {
            default = Some(resp);
        } else {
            conditional.push((resp.status.clone(), resp));
        }
    }
    conditional.sort_by(|a, b| a.0.cmp(&b.0));

    let fallback_has_body = op.responses.iter().any(|r| !r.content.is_empty());

    if conditional.is_empty() {
        if let Some(d) = default {
            emit_response_return(cb, d, false);
        } else {
            emit_fallback_return(cb, fallback_has_body, false);
        }
    } else {
        for (i, (_, resp)) in conditional.iter().enumerate() {
            let keyword = if i == 0 { "if" } else { "else if" };
            let status_code: u16 = resp.status.parse().unwrap_or(0);
            cb.add(
                &format!("{} (response.status === {}) {{\n  ", keyword, status_code),
                vec![],
            );
            emit_response_return(cb, resp, true);
            cb.add("}\n", vec![]);
        }
        cb.add("else {\n  ", vec![]);
        if let Some(d) = default {
            emit_response_return(cb, d, true);
        } else {
            emit_fallback_return(cb, fallback_has_body, true);
        }
        cb.add("}\n", vec![]);
    }
}

/// `return new Wrapper(response) as Wrapper<Body> & { status: X };`
fn emit_response_return(
    cb: &mut sigil_stitch::code_block::CodeBlockBuilder<TypeScript>,
    resp: &IrResponse,
    _inside_block: bool,
) {
    let kind = classify_response(resp);
    let status_ty = match resp.status.parse::<u16>() {
        Ok(n) => n.to_string(),
        Err(_) => "number".to_string(),
    };

    let wrapper_value = match kind {
        ResponseKind::Json(_) => rt_value("JSONApiResponse"),
        ResponseKind::Text => rt_value("TextApiResponse"),
        ResponseKind::Blob => rt_value("BlobApiResponse"),
        ResponseKind::None => rt_value("VoidApiResponse"),
    };
    let wrapper_type = match kind.clone() {
        ResponseKind::Json(body_ty) => {
            let body = body_ty.unwrap_or_else(|| TypeName::primitive("unknown"));
            TypeName::generic(rt_value("JSONApiResponse"), vec![body])
        }
        ResponseKind::Text => rt_value("TextApiResponse"),
        ResponseKind::Blob => rt_value("BlobApiResponse"),
        ResponseKind::None => rt_value("VoidApiResponse"),
    };

    cb.add(
        &format!(
            "return new %T(response) as %T & {{ status: {} }};\n",
            status_ty
        ),
        vec![Arg::TypeName(wrapper_value), Arg::TypeName(wrapper_type)],
    );
}

/// `return new JSONApiResponse(response) as JSONApiResponse<unknown> & { status: number };`
/// (or VoidApiResponse equivalent when no body appears anywhere).
fn emit_fallback_return(
    cb: &mut sigil_stitch::code_block::CodeBlockBuilder<TypeScript>,
    any_body: bool,
    _inside_block: bool,
) {
    if any_body {
        cb.add(
            "return new %T(response) as %T<%T> & { status: number };\n",
            vec![
                Arg::TypeName(rt_value("JSONApiResponse")),
                Arg::TypeName(rt_value("JSONApiResponse")),
                Arg::TypeName(TypeName::primitive("unknown")),
            ],
        );
    } else {
        cb.add(
            "return new %T(response) as %T & { status: number };\n",
            vec![
                Arg::TypeName(rt_value("VoidApiResponse")),
                Arg::TypeName(rt_value("VoidApiResponse")),
            ],
        );
    }
}

// ============================================================================
// Convenience method
// ============================================================================

fn build_convenience_method(op: &IrOperation) -> Result<FunSpec<TypeScript>, String> {
    let method_base = op.operation_id.to_lower_camel_case();
    let raw_name = format!("{}Raw", method_base);

    let mut fb = FunSpec::<TypeScript>::builder(&method_base);
    fb.is_async();

    for param in method_param_specs(op) {
        fb.add_param(param);
    }
    let body_ty = convenience_body_type(op);
    fb.returns(TypeName::generic(
        TypeName::primitive("Promise"),
        vec![body_ty.clone()],
    ));

    let args_list = raw_call_args(op);
    let mut body = CodeBlock::<TypeScript>::builder();
    // The raw union may include `JSONApiResponse<unknown>` for the fallback,
    // so `response.value()` widens to `unknown`. Narrow it with a cast to
    // the declared body type — this is a "genuine boundary" where the
    // fallback's runtime shape isn't knowable from the OpenAPI spec.
    if is_void_type(&body_ty) {
        body.add(
            &format!(
                "const response = await this.{}({});\nreturn await response.value();",
                raw_name, args_list
            ),
            vec![],
        );
    } else {
        body.add(
            &format!(
                "const response = await this.{}({});\nreturn await response.value() as %T;",
                raw_name, args_list
            ),
            vec![Arg::TypeName(body_ty)],
        );
    }
    fb.body(body.build().expect("CodeBlock builds"));

    fb.build()
        .map_err(|e| format!("sigil_emit_api: convenience method {method_base}: {e}"))
}

fn is_void_type(ty: &TypeName<TypeScript>) -> bool {
    let Ok(val) = serde_json::to_value(ty) else {
        return false;
    };
    let Ok(void_val) = serde_json::to_value(TypeName::<TypeScript>::primitive("void")) else {
        return false;
    };
    val == void_val
}

// ============================================================================
// Parameter helpers
// ============================================================================

fn method_param_specs(op: &IrOperation) -> Vec<ParameterSpec<TypeScript>> {
    let mut out = Vec::new();
    if operation_has_params(op) {
        let iface_name = format!(
            "Api{}Request",
            op.operation_id.to_lower_camel_case().to_pascal_case()
        );
        out.push(
            ParameterSpec::<TypeScript>::builder("requestParameters", TypeName::raw(&iface_name))
                .build()
                .expect("ParameterSpec builds"),
        );
    }
    out.push(init_overrides_param());
    out
}

fn init_overrides_param() -> ParameterSpec<TypeScript> {
    ParameterSpec::<TypeScript>::builder("initOverrides?", init_overrides_type())
        .build()
        .expect("ParameterSpec builds")
}

fn init_overrides_type() -> TypeName<TypeScript> {
    TypeName::union(vec![
        TypeName::primitive("RequestInit"),
        rt_type("InitOverrideFunction"),
    ])
}

fn operation_has_params(op: &IrOperation) -> bool {
    let has_non_cookie = op
        .parameters
        .iter()
        .any(|p| !matches!(p.location, IrParameterLocation::Cookie));
    has_non_cookie || op.request_body.is_some()
}

fn raw_call_args(op: &IrOperation) -> String {
    if operation_has_params(op) {
        "requestParameters, initOverrides".to_string()
    } else {
        "initOverrides".to_string()
    }
}

// ============================================================================
// Return types (structural — imports flow through %T)
// ============================================================================

fn raw_return_type(op: &IrOperation) -> TypeName<TypeScript> {
    // Returns `Promise<{OpId}RawResponse>` — the alias itself lives in the
    // same file (see `build_response_aliases_block`) so no import is needed.
    TypeName::generic(
        TypeName::primitive("Promise"),
        vec![TypeName::raw(&raw_response_alias_name(op))],
    )
}

fn raw_response_member(resp: &IrResponse, kind: &ResponseKind) -> TypeName<TypeScript> {
    let status_literal = resp
        .status
        .parse::<u16>()
        .ok()
        .map(|n| format!("{{ status: {n} }}"))
        .unwrap_or_else(|| "{ status: number }".to_string());
    let wrapper_type = match kind.clone() {
        ResponseKind::Json(body_ty) => {
            let body = body_ty.unwrap_or_else(|| TypeName::primitive("unknown"));
            TypeName::generic(rt_value("JSONApiResponse"), vec![body])
        }
        ResponseKind::Text => rt_value("TextApiResponse"),
        ResponseKind::Blob => rt_value("BlobApiResponse"),
        ResponseKind::None => rt_value("VoidApiResponse"),
    };
    TypeName::intersection(vec![wrapper_type, TypeName::raw(&status_literal)])
}

fn fallback_member(any_body: bool) -> TypeName<TypeScript> {
    let wrapper = if any_body {
        TypeName::generic(
            rt_value("JSONApiResponse"),
            vec![TypeName::primitive("unknown")],
        )
    } else {
        rt_value("VoidApiResponse")
    };
    TypeName::intersection(vec![wrapper, TypeName::raw("{ status: number }")])
}

fn convenience_body_type(op: &IrOperation) -> TypeName<TypeScript> {
    let mut members: Vec<TypeName<TypeScript>> = Vec::new();
    let mut any_body = false;
    for resp in &op.responses {
        match classify_response(resp) {
            ResponseKind::Json(Some(body)) => {
                any_body = true;
                members.push(body);
            }
            ResponseKind::Json(None) => {
                any_body = true;
                members.push(TypeName::primitive("unknown"));
            }
            ResponseKind::Text => {
                any_body = true;
                members.push(TypeName::primitive("string"));
            }
            ResponseKind::Blob => {
                any_body = true;
                members.push(TypeName::primitive("Blob"));
            }
            ResponseKind::None => {}
        }
    }
    if !any_body {
        TypeName::primitive("void")
    } else if members.len() == 1 {
        members.pop().unwrap()
    } else {
        dedup_union(members)
    }
}

/// Stable de-dup of union members by `Debug` representation (cheap, correct
/// for sigil's `TypeName` variants).
fn dedup_union_members(members: Vec<TypeName<TypeScript>>) -> Vec<TypeName<TypeScript>> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut out: Vec<TypeName<TypeScript>> = Vec::new();
    for m in members {
        let key = format!("{:?}", m);
        if seen.insert(key) {
            out.push(m);
        }
    }
    out
}

fn dedup_union(members: Vec<TypeName<TypeScript>>) -> TypeName<TypeScript> {
    let mut out = dedup_union_members(members);
    if out.len() == 1 {
        out.pop().unwrap()
    } else {
        TypeName::union(out)
    }
}

// ============================================================================
// Response classification
// ============================================================================

#[derive(Clone)]
enum ResponseKind {
    Json(Option<TypeName<TypeScript>>),
    Text,
    Blob,
    None,
}

fn classify_response(resp: &IrResponse) -> ResponseKind {
    if let Some(ty) = resp.content.get("application/json") {
        return ResponseKind::Json(Some(type_expr_to_typename(ty)));
    }
    if resp.content.keys().any(|k| k.contains("json")) {
        return ResponseKind::Json(None);
    }
    let text_types = [
        "text/plain",
        "text/html",
        "application/xml",
        "text/xml",
        "application/x-www-form-urlencoded",
        "text/event-stream",
    ];
    if resp
        .content
        .keys()
        .any(|k| text_types.iter().any(|t| k.contains(t)))
    {
        return ResponseKind::Text;
    }
    if !resp.content.is_empty() {
        return ResponseKind::Blob;
    }
    ResponseKind::None
}

// ============================================================================
// Parameter-name resolution (collision disambiguation)
// ============================================================================

/// Key used for collision detection — tracks both params (by location tag)
/// and the synthetic "body" field from the request body.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct ParamKey {
    location_tag: &'static str,
    original_name: String,
}

impl ParamKey {
    fn body() -> Self {
        Self {
            location_tag: "body",
            original_name: String::new(),
        }
    }

    fn param(loc: &IrParameterLocation, name: &str) -> Self {
        Self {
            location_tag: location_tag(loc),
            original_name: name.to_string(),
        }
    }
}

fn location_tag(loc: &IrParameterLocation) -> &'static str {
    match loc {
        IrParameterLocation::Path => "path",
        IrParameterLocation::Query => "query",
        IrParameterLocation::Header => "header",
        IrParameterLocation::Cookie => "cookie",
    }
}

/// Resolve the TypeScript field name for every param + optional request body
/// on an operation. When two or more entries camelCase to the same name we
/// prefix each colliding entry with its location (`pathId`, `queryId`,
/// `headerId`, `queryBody`, `bodyBody`, ...). Single occurrences keep the
/// camelCased original.
fn resolve_param_names(op: &IrOperation) -> BTreeMap<ParamKey, String> {
    let mut entries: Vec<(ParamKey, String)> = Vec::new();
    for p in &op.parameters {
        if matches!(p.location, IrParameterLocation::Cookie) {
            continue;
        }
        let cc = p.name.to_lower_camel_case();
        entries.push((ParamKey::param(&p.location, &p.name), cc));
    }
    if op.request_body.is_some() {
        entries.push((ParamKey::body(), "body".to_string()));
    }

    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for (_, cc) in &entries {
        *counts.entry(cc.clone()).or_insert(0) += 1;
    }

    let mut out = BTreeMap::new();
    for (key, cc) in entries {
        let final_name = if counts.get(&cc).copied().unwrap_or(0) > 1 {
            format!("{}{}", key.location_tag, cc.to_pascal_case())
        } else {
            cc
        };
        out.insert(key, final_name);
    }
    out
}

fn resolved_param(names: &BTreeMap<ParamKey, String>, p: &IrParameter) -> String {
    names
        .get(&ParamKey::param(&p.location, &p.name))
        .cloned()
        .unwrap_or_else(|| p.name.to_lower_camel_case())
}

fn resolved_body(names: &BTreeMap<ParamKey, String>) -> String {
    names
        .get(&ParamKey::body())
        .cloned()
        .unwrap_or_else(|| "body".to_string())
}

// ============================================================================
// Runtime-symbol TypeName constructors
// ============================================================================

/// Runtime symbol imported as a value (class, function, const): emits
/// `import { Name } from '../runtime/runtime'`.
fn rt_value(name: &str) -> TypeName<TypeScript> {
    TypeName::importable(RUNTIME_MOD, name)
}

/// Runtime symbol imported type-only: emits `import type { Name } from '../runtime/runtime'`.
fn rt_type(name: &str) -> TypeName<TypeScript> {
    TypeName::importable_type(RUNTIME_MOD, name)
}

// ============================================================================
// IrTypeExpr → TypeName
// ============================================================================

fn type_expr_to_typename(expr: &IrTypeExpr) -> TypeName<TypeScript> {
    match expr {
        IrTypeExpr::Named(name) => {
            let ts_name = name.to_pascal_case();
            TypeName::importable_type(&format!("../models/{ts_name}"), &ts_name)
        }
        IrTypeExpr::Primitive(p) => TypeName::primitive(primitive_to_ts(p)),
        IrTypeExpr::Array(inner) => TypeName::array(type_expr_to_typename(inner)),
        IrTypeExpr::Nullable(inner) => TypeName::union(vec![
            type_expr_to_typename(inner),
            TypeName::primitive("null"),
        ]),
        IrTypeExpr::StringLiteral(s) => TypeName::raw(&format!("'{s}'")),
        IrTypeExpr::StringEnum(values) => TypeName::union(
            values
                .iter()
                .map(|v| TypeName::raw(&format!("'{v}'")))
                .collect(),
        ),
        IrTypeExpr::Map(inner) => TypeName::generic(
            TypeName::primitive("Record"),
            vec![TypeName::primitive("string"), type_expr_to_typename(inner)],
        ),
        IrTypeExpr::Union(members) => {
            TypeName::union(members.iter().map(type_expr_to_typename).collect())
        }
        IrTypeExpr::Any => TypeName::primitive("unknown"),
    }
}

fn primitive_to_ts(p: &IrPrimitive) -> &'static str {
    match p {
        IrPrimitive::String
        | IrPrimitive::Binary
        | IrPrimitive::Date
        | IrPrimitive::DateTime
        | IrPrimitive::Uuid
        | IrPrimitive::StringWithFormat(_) => "string",
        IrPrimitive::Integer
        | IrPrimitive::Number
        | IrPrimitive::IntegerWithFormat(_)
        | IrPrimitive::NumberWithFormat(_) => "number",
        IrPrimitive::Boolean => "boolean",
    }
}
