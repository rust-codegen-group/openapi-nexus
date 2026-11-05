//! Sigil-stitch emit for TypeScript API class files.
//!
//! Produces one `FileSpec` per tag containing:
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

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::codegen::traits::file_writer::FileInfo;
use crate::generators::multipart::{MultipartValueEncoding, multipart_parts_for_request_body};
use crate::generators::request_inputs::{RequestInputPlan, request_input_for_operation};
use crate::generators::response_headers::{
    ResponseHeaderPlan, ResponseHeaderValueKind, collect_response_headers,
    unique_response_header_accessor_names, unique_response_header_accessor_names_with_used,
};
use crate::generators::response_names::response_entry_kind as response_detail_kind;
use crate::generators::type_application;
use crate::generators::{ArtifactKind, GeneratorError};
use crate::ir::types::{
    IrOperation, IrParameter, IrPrimitive, IrRequestBody, IrResponse, IrSpec, IrTypeExpr,
    ParameterLocation as IrParameterLocation,
};
use heck::{ToLowerCamelCase as _, ToPascalCase as _};
use sigil_stitch::code_block::{Arg, CodeBlock};
use sigil_stitch::error::SigilStitchError;
use sigil_stitch::lang::typescript::TypeScript;
use sigil_stitch::prelude::sigil_quote;
use sigil_stitch::spec::field_spec::FieldSpec;
use sigil_stitch::spec::file_spec::FileSpec;
use sigil_stitch::spec::fun_spec::FunSpec;
use sigil_stitch::spec::modifiers::{DeclarationContext, TypeKind, Visibility};
use sigil_stitch::spec::parameter_spec::ParameterSpec;
use sigil_stitch::spec::type_spec::TypeSpec;
use sigil_stitch::type_name::TypeName;

use super::sigil_emit::{EmitFlags, build_convertible_set, fn_base_name};

const RUNTIME_MOD: &str = "../runtime/runtime";

/// Lower every tag in the IR spec into a sigil-rendered API class `FileInfo`.
pub fn generate_api_files(
    ir: &IrSpec,
    property_naming_camel_case: bool,
    request_inputs: &RequestInputPlan,
    ts: &TypeScript,
) -> Result<Vec<FileInfo>, GeneratorError> {
    let header = super::project_files::render_file_header(&ir.info);
    let by_tag = group_by_tag(&ir.operations);
    let flags = EmitFlags {
        property_naming_camel_case,
        ..EmitFlags::default()
    };
    let convertible = build_convertible_set(ir, flags);

    let mut files = Vec::with_capacity(by_tag.len());
    for (tag, ops) in &by_tag {
        let file_spec = emit_api_file(
            tag,
            ops,
            ir,
            property_naming_camel_case,
            request_inputs,
            &convertible,
            ts,
        )
        .map_err(|source| {
            GeneratorError::emission(ArtifactKind::Api, format!("{tag} API"), source)
        })?;
        let body = file_spec.render(100).map_err(|source| {
            GeneratorError::emission(ArtifactKind::Api, format!("{tag} API"), source)
        })?;
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
        let mut value_names = vec![class_name.clone()];
        for op in ops {
            if !op.parameters.is_empty() || op.request_body.is_some() {
                type_names.push(format!(
                    "Api{}Request",
                    op.operation_id.to_lower_camel_case().to_pascal_case()
                ));
            }
            type_names.push(raw_response_alias_name(op));
            type_names.push(error_detail_alias_name(op));
            value_names.push(error_class_name(op));
        }
        type_names.push(interface_name);

        out.push(ApiFileExports {
            filename_base: class_name.clone(),
            type_names,
            value_names,
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

fn emit_api_file(
    tag: &str,
    ops: &[&IrOperation],
    ir: &IrSpec,
    property_naming_camel_case: bool,
    request_inputs: &RequestInputPlan,
    convertible: &HashSet<String>,
    ts: &TypeScript,
) -> Result<FileSpec, SigilStitchError> {
    let class_name = format!("{}Api", tag.to_pascal_case());
    let interface_name = format!("{}Interface", class_name);

    let mut fb = FileSpec::builder_with(&format!("{}.ts", class_name), ts.clone());

    // Request interfaces — one per op that has at least one parameter / body.
    for op in ops {
        if let Some(req_iface) = build_request_interface(op, request_inputs) {
            fb = fb.add_type(req_iface);
        }
    }

    // Per-operation raw response type aliases — emit each union member on its
    // own line so readers can scan each `Wrapper & { status: N }` pair without
    // the pretty printer splitting intersections across lines.
    fb = fb.add_code(build_response_aliases_block(ops)?);
    for accessor in build_success_header_accessors(ops, ir)? {
        fb = fb.add_function(accessor);
    }
    fb = fb.add_code(build_error_types_block(ops, ir)?);

    // ApiInterface — emit as a raw CodeBlock so `%T` slots propagate imports
    // for every arrow-function parameter and return type. (TypeSpec with
    // FieldSpec arrow-function fields can't carry structural TypeName for the
    // whole `(p: T) => R` shape because sigil's `TypeName::function` doesn't
    // emit parameter names.)
    fb = fb.add_code(build_api_interface_block(&interface_name, ops)?);

    // When property_naming_camel_case is enabled, add explicit imports for
    // converter functions and $Wire types referenced in raw code blocks.
    if property_naming_camel_case {
        let request_types =
            collect_request_named_types(ops, ir, property_naming_camel_case, convertible);
        let response_types = collect_response_named_types(ops, convertible);

        for name in request_types.union(&response_types) {
            let pascal = name.to_pascal_case();
            let module = format!("../models/{pascal}");
            let base = fn_base_name(&pascal);
            let in_request = request_types.contains(name);
            let in_response = response_types.contains(name);

            if in_response {
                let from_fn = format!("{base}FromJSON");
                let wire_name = format!("{}$Wire", pascal);
                fb = fb.add_import(sigil_stitch::spec::import_spec::ImportSpec::named(
                    &module, &from_fn,
                ));
                fb = fb.add_import(sigil_stitch::spec::import_spec::ImportSpec::named_type(
                    &module, &wire_name,
                ));
            }
            if in_request {
                let to_fn = format!("{base}ToJSON");
                fb = fb.add_import(sigil_stitch::spec::import_spec::ImportSpec::named(
                    &module, &to_fn,
                ));
            }
        }
    }

    // ApiClass stays structural so modifiers / docs / constructor delegation
    // use sigil's machinery.
    fb = fb.add_type(build_api_class(
        &class_name,
        &interface_name,
        ops,
        ir,
        property_naming_camel_case,
        convertible,
    )?);

    fb.build()
}

/// Name of the exported type alias holding the raw (wrapped) response union
/// for a given operation. E.g. `updatePet` → `UpdatePetRawResponse`.
fn raw_response_alias_name(op: &IrOperation) -> String {
    format!(
        "{}RawResponse",
        op.operation_id.to_lower_camel_case().to_pascal_case()
    )
}

fn error_class_name(op: &IrOperation) -> String {
    format!(
        "{}Error",
        op.operation_id.to_lower_camel_case().to_pascal_case()
    )
}

fn error_detail_alias_name(op: &IrOperation) -> String {
    format!("{}Detail", error_class_name(op))
}

/// Emit one `export type {OpId}RawResponse = | A | B | C;` block per op.
/// Each member is a `%T` slot so imports still flow through the collector,
/// and each sits on its own line so intersections (`Wrapper & { status: N }`)
/// stay intact.
fn build_response_aliases_block(ops: &[&IrOperation]) -> Result<CodeBlock, SigilStitchError> {
    let mut cb = CodeBlock::builder();
    for op in ops {
        let alias = raw_response_alias_name(op);
        let members = raw_response_members(op);
        let alias_block = if members.len() == 1 {
            let member = members.into_iter().next().unwrap();
            sigil_quote!(TypeScript {
                export type $N(alias.as_str()) = $T(member);
            })
        } else {
            sigil_quote!(TypeScript {
                export type $N(alias.as_str()) =
                $L("  | ")$for(member in &members; separator = "\n  | ") { $T((*member).clone()) };
            })
        }?;
        cb.add_code(alias_block);
        cb.add_line();
    }
    cb.build()
}

fn build_success_header_accessors(
    ops: &[&IrOperation],
    ir: &IrSpec,
) -> Result<Vec<FunSpec>, SigilStitchError> {
    let mut accessors = Vec::new();
    let mut used_accessor_names = HashSet::new();
    for op in ops {
        let alias = raw_response_alias_name(op);
        let headers = collect_response_headers(
            op.responses
                .iter()
                .filter(|response| is_success_status(&response.status)),
            ir,
        );
        let function_names = unique_response_header_accessor_names_with_used(
            &headers,
            |wire_name| {
                format!(
                    "get{}{}Header",
                    op.operation_id.to_lower_camel_case().to_pascal_case(),
                    wire_name.to_pascal_case()
                )
            },
            &mut used_accessor_names,
        );
        for (header, function_name) in headers.iter().zip(function_names) {
            accessors.push(build_ts_header_accessor(
                &function_name,
                Some(ParameterSpec::of("response", TypeName::primitive(&alias))),
                header,
                "response.headers",
                true,
            )?);
        }
    }
    Ok(accessors)
}

fn build_error_types_block(
    ops: &[&IrOperation],
    ir: &IrSpec,
) -> Result<CodeBlock, SigilStitchError> {
    let mut cb = CodeBlock::builder();
    for op in ops {
        let detail_alias = error_detail_alias_name(op);
        let class_name = error_class_name(op);
        let error_responses: Vec<&IrResponse> = op
            .responses
            .iter()
            .filter(|resp| !is_success_status(&resp.status))
            .collect();
        cb.add(&format!("export type {detail_alias} =\n"), vec![]);
        let mut used = HashSet::new();
        for (idx, resp) in error_responses.iter().enumerate() {
            let prefix = if idx == 0 { "  " } else { "  | " };
            let kind = unique_ts_kind(&response_detail_kind(&resp.status), &mut used);
            let status_ty = resp
                .status
                .parse::<u16>()
                .ok()
                .map(|n| n.to_string())
                .unwrap_or_else(|| "number".to_string());
            let value_ty = response_value_type(resp);
            cb.add(prefix, vec![]);
            cb.add(
                &format!(
                    "{{ kind: '{}'; status: {}; raw: Response; value(): Promise<%T> }}\n",
                    kind, status_ty
                ),
                vec![Arg::TypeName(value_ty)],
            );
        }
        let prefix = if error_responses.is_empty() {
            "  "
        } else {
            "  | "
        };
        cb.add(
            &format!(
                "{prefix}{{ kind: 'unexpected'; status: number; raw: Response; value(): Promise<%T> }};\n"
            ),
            vec![Arg::TypeName(TypeName::primitive("Blob"))],
        );
        cb.add_line();
        cb.add(
            &format!(
                "export class {class_name} extends %T {{\n  readonly detail: {detail_alias};\n\n  constructor(response: Response, detail: {detail_alias}, msg = 'Response returned an error code') {{\n    super(response, msg);\n    this.detail = detail;\n  }}\n"
            ),
            vec![Arg::TypeName(rt_value("ResponseError"))],
        );
        cb.add("%>", ());
        let headers = collect_response_headers(
            op.responses
                .iter()
                .filter(|response| !is_success_status(&response.status)),
            ir,
        );
        let method_names = unique_response_header_accessor_names(&headers, |wire_name| {
            format!("get{}Header", wire_name.to_pascal_case())
        });
        for (header, method_name) in headers.iter().zip(method_names) {
            cb.add_line();
            cb.add_code(
                build_ts_header_accessor(
                    &method_name,
                    None,
                    header,
                    "this.response.headers",
                    false,
                )?
                .emit(&TypeScript::new(), DeclarationContext::Member)?,
            );
        }
        cb.add("%<}\n\n", ());
    }
    cb.build()
}

fn build_ts_header_accessor(
    name: &str,
    parameter: Option<ParameterSpec>,
    header: &ResponseHeaderPlan,
    headers_expr: &str,
    exported: bool,
) -> Result<FunSpec, SigilStitchError> {
    let mut accessor = FunSpec::builder(name)
        .returns(vec![TypeName::union(vec![
            ts_header_type(header.value_kind),
            TypeName::primitive("undefined"),
        ])])
        .body(ts_header_body(header, headers_expr)?);
    if let Some(parameter) = parameter {
        accessor = accessor.add_param(parameter);
    }
    if exported {
        accessor = accessor.visibility(Visibility::Public);
    }
    accessor.build()
}

fn ts_header_type(kind: ResponseHeaderValueKind) -> TypeName {
    match kind {
        ResponseHeaderValueKind::String => TypeName::primitive("string"),
        ResponseHeaderValueKind::Integer | ResponseHeaderValueKind::Number => {
            TypeName::primitive("number")
        }
        ResponseHeaderValueKind::Boolean => TypeName::primitive("boolean"),
    }
}

fn ts_header_body(
    header: &ResponseHeaderPlan,
    headers_expr: &str,
) -> Result<CodeBlock, SigilStitchError> {
    let wire_name = header.wire_name.as_str();
    let integer_regex = r"/^[+-]?\d+$/";
    let number_regex = r"/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/";
    match header.value_kind {
        ResponseHeaderValueKind::String => sigil_quote!(TypeScript {
            return $L(headers_expr).get($S(wire_name)) ?? undefined;
        }),
        ResponseHeaderValueKind::Integer => sigil_quote!(TypeScript {
            const value = $L(headers_expr).get($S(wire_name));
            if (value === null || !$L(integer_regex).test(value)) {
                return undefined;
            }
            const parsed = Number(value);
            return Number.isSafeInteger(parsed) ? parsed : undefined;
        }),
        ResponseHeaderValueKind::Number => sigil_quote!(TypeScript {
            const value = $L(headers_expr).get($S(wire_name));
            if (value === null || !$L(number_regex).test(value)) {
                return undefined;
            }
            const parsed = Number(value);
            return Number.isFinite(parsed) ? parsed : undefined;
        }),
        ResponseHeaderValueKind::Boolean => sigil_quote!(TypeScript {
            const value = $L(headers_expr).get($S(wire_name));
            if (value === $S("true")) {
                return true;
            }
            if (value === $S("false")) {
                return false;
            }
            return undefined;
        }),
    }
}

/// Compute the deduplicated list of union members for an operation's raw
/// response type (wrapper intersected with status literal).
fn raw_response_members(op: &IrOperation) -> Vec<TypeName> {
    let mut members: Vec<TypeName> = Vec::new();
    let mut any_body = false;
    let mut has_default = false;

    for resp in op
        .responses
        .iter()
        .filter(|resp| is_success_status(&resp.status))
    {
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

fn build_request_interface(
    op: &IrOperation,
    request_inputs: &RequestInputPlan,
) -> Option<TypeSpec> {
    let has_params = !op.parameters.is_empty() || op.request_body.is_some();
    if !has_params {
        return None;
    }

    let method_base = op.operation_id.to_lower_camel_case();
    let interface_name = format!("Api{}Request", method_base.to_pascal_case());
    let names = resolve_param_names(op);

    let mut tb =
        TypeSpec::builder(&interface_name, TypeKind::Interface).visibility(Visibility::Public);

    for param in &op.parameters {
        if matches!(param.location, IrParameterLocation::Cookie) {
            continue;
        }
        tb = tb.add_field(build_param_field(param, &resolved_param(&names, param)));
    }
    if let Some(rb) = &op.request_body {
        tb = tb.add_field(build_body_field(
            op,
            rb,
            &resolved_body(&names),
            request_inputs,
        ));
    }

    tb.build().ok()
}

fn build_param_field(param: &IrParameter, name: &str) -> FieldSpec {
    let ty = type_expr_to_typename(&param.type_expr);
    let mut fb = FieldSpec::builder(name, ty);
    if !param.required {
        fb = fb.is_optional();
    }
    if let Some(desc) = &param.description {
        fb = fb.doc(desc);
    }
    fb.build().expect("FieldSpec builds")
}

/// Choose the preferred media type from a request body, matching
/// `build_body_field`'s type-selection logic so the emitted `Content-Type`
/// agrees with the schema we typed the body as. Prefers `application/json`
/// if declared, otherwise the first media type in spec order.
fn preferred_request_media_type(rb: &IrRequestBody) -> Option<String> {
    pick_media_type(&rb.content, |media_type| {
        media_type_base(media_type) == "application/json"
    })
    .or_else(|| pick_media_type(&rb.content, is_json_media_type))
    .or_else(|| {
        pick_media_type(&rb.content, |media_type| {
            media_type_base(media_type) == "multipart/form-data"
        })
    })
    .or_else(|| {
        pick_media_type(&rb.content, |media_type| {
            media_type_base(media_type) == "application/x-www-form-urlencoded"
        })
    })
    .or_else(|| pick_media_type(&rb.content, is_xml_media_type))
    .or_else(|| {
        pick_media_type(&rb.content, |media_type| {
            media_type_base(media_type) == "text/plain"
        })
    })
    .or_else(|| {
        pick_media_type(&rb.content, |media_type| {
            media_type_base(media_type) == "application/octet-stream"
        })
    })
    .or_else(|| pick_first_media_type(&rb.content))
}

fn build_body_field(
    op: &IrOperation,
    rb: &IrRequestBody,
    name: &str,
    request_inputs: &RequestInputPlan,
) -> FieldSpec {
    let ty = preferred_request_media_type(rb)
        .and_then(|mt| {
            if media_type_base(&mt) == "multipart/form-data" {
                request_input_for_operation(request_inputs, op, &mt).map(|input| {
                    let ts_name = input.name.to_pascal_case();
                    TypeName::importable_type(&format!("../models/{ts_name}"), &ts_name)
                })
            } else {
                rb.content.get(mt.as_str()).map(type_expr_to_typename)
            }
        })
        .unwrap_or_else(|| TypeName::primitive("unknown"));
    let mut fb = FieldSpec::builder(name, ty);
    if !rb.required {
        fb = fb.is_optional();
    }
    if let Some(desc) = &rb.description {
        fb = fb.doc(desc);
    }
    fb.build().expect("FieldSpec builds")
}

// ============================================================================
// ApiInterface (raw CodeBlock with %T slots)
// ============================================================================

fn build_api_interface_block(
    interface_name: &str,
    ops: &[&IrOperation],
) -> Result<CodeBlock, SigilStitchError> {
    let mut cb = CodeBlock::builder();
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
            type_application(
                TypeName::primitive("Promise"),
                vec![convenience_body_type(op)],
            ),
        );
        cb.add(";\n", vec![]);
    }

    cb.add("}", vec![]);
    cb.build()
}

/// Append `(requestParameters: ApiXRequest, initOverrides?: RequestInit | InitOverrideFunction) => %T`
/// onto the given block, using `%T` slots for every named type so imports
/// flow through.
fn emit_arrow_signature(
    cb: &mut sigil_stitch::code_block::CodeBlockBuilder,
    op: &IrOperation,
    return_ty: TypeName,
) {
    let mut parts: Vec<String> = Vec::new();
    let mut args: Vec<Arg> = Vec::new();

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
    ir: &IrSpec,
    property_naming_camel_case: bool,
    convertible: &HashSet<String>,
) -> Result<TypeSpec, SigilStitchError> {
    let mut tb = TypeSpec::builder(class_name, TypeKind::Class)
        .visibility(Visibility::Public)
        .extends(rt_value("BaseAPI"))
        .implements(TypeName::raw(interface_name))
        .add_method(build_constructor());

    for op in ops {
        tb = tb.add_method(build_raw_method(
            op,
            ir,
            property_naming_camel_case,
            convertible,
        )?);
        tb = tb.add_method(build_convenience_method(op)?);
    }

    tb.build()
}

fn build_constructor() -> FunSpec {
    let body = sigil_quote!(TypeScript {
        super(configuration ?? $T(rt_value("DefaultConfig")));
    })
    .expect("CodeBlock builds");

    FunSpec::builder("constructor")
        .is_constructor()
        .doc("Initialize the API client")
        .add_param(
            ParameterSpec::builder("configuration?", rt_type("Configuration"))
                .build()
                .expect("ParameterSpec builds"),
        )
        .body(body)
        .build()
        .expect("Constructor FunSpec builds")
}

// ============================================================================
// Raw method — full request body with parameter handling and response dispatch
// ============================================================================

fn build_raw_method(
    op: &IrOperation,
    ir: &IrSpec,
    property_naming_camel_case: bool,
    convertible: &HashSet<String>,
) -> Result<FunSpec, SigilStitchError> {
    let method_base = op.operation_id.to_lower_camel_case();
    let method_name = format!("{}Raw", method_base);

    let mut fb = FunSpec::builder(&method_name).is_async();

    for param in method_param_specs(op) {
        fb = fb.add_param(param);
    }
    fb = fb.returns(vec![raw_return_type(op)]);

    let mut body = CodeBlock::builder();
    emit_required_param_checks(&mut body, op, &method_name);
    emit_url_path(&mut body, op)?;
    emit_query_params(&mut body, op)?;
    emit_headers(&mut body, op)?;
    emit_request_body(&mut body, op, ir, property_naming_camel_case, convertible)?;
    emit_make_request(&mut body, op, op.request_body.is_some());
    emit_response_handler(&mut body, op, property_naming_camel_case, convertible)?;

    fb = fb.body(body.build()?);

    fb.build()
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
    cb: &mut sigil_stitch::code_block::CodeBlockBuilder,
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
    cb: &mut sigil_stitch::code_block::CodeBlockBuilder,
    op: &IrOperation,
) -> Result<(), SigilStitchError> {
    cb.add("// Build path with path parameters\n", vec![]);
    let has_path_params = op
        .parameters
        .iter()
        .any(|p| matches!(p.location, IrParameterLocation::Path));
    let path = op.path.as_str();
    cb.add_code(sigil_quote!(TypeScript {
        $if(has_path_params) {
            let urlPath = $V(path);
        } $else {
            const urlPath = $V(path);
        }
    })?);

    let names = resolve_param_names(op);
    for p in op
        .parameters
        .iter()
        .filter(|p| matches!(p.location, IrParameterLocation::Path))
    {
        let resolved = resolved_param(&names, p);
        let original = &p.name;
        let access = request_parameters_access(&resolved);
        let placeholder = format!("{{{original}}}");
        cb.add_code(
            sigil_quote!(TypeScript {
                urlPath = urlPath.replace($V(placeholder.as_str()), encodeURIComponent(String($L(access.as_str()))));
            })
            ?,
        );
    }
    Ok(())
}

fn emit_query_params(
    cb: &mut sigil_stitch::code_block::CodeBlockBuilder,
    op: &IrOperation,
) -> Result<(), SigilStitchError> {
    cb.add("// Build query parameters\n", vec![]);
    cb.add_code(sigil_quote!(TypeScript {
        const queryParameters: $T(rt_type("HTTPQuery")) = {};
    })?);
    let names = resolve_param_names(op);
    for p in op
        .parameters
        .iter()
        .filter(|p| matches!(p.location, IrParameterLocation::Query))
    {
        let resolved = resolved_param(&names, p);
        let access = request_parameters_access(&resolved);
        let key = p.name.as_str();
        cb.add_code(sigil_quote!(TypeScript {
            if ($L(access.as_str()) !== undefined) {
                queryParameters[$S(key)] = $L(access.as_str());
            }
        })?);
    }
    Ok(())
}

fn emit_headers(
    cb: &mut sigil_stitch::code_block::CodeBlockBuilder,
    op: &IrOperation,
) -> Result<(), SigilStitchError> {
    cb.add("// Build headers\n", vec![]);
    cb.add(
        "const headerParameters: Record<string, string> = {\n",
        vec![],
    );
    if let Some(rb) = &op.request_body
        && let Some(media_type) = preferred_request_media_type(rb)
        && media_type_base(&media_type) != "multipart/form-data"
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
        let key = p.name.as_str();
        cb.add_code(sigil_quote!(TypeScript {
            if ($L(access.as_str()) !== undefined) {
                headerParameters[$S(key)] = String($L(access.as_str()));
            }
        })?);
    }
    Ok(())
}

fn emit_request_body(
    cb: &mut sigil_stitch::code_block::CodeBlockBuilder,
    op: &IrOperation,
    ir: &IrSpec,
    property_naming_camel_case: bool,
    convertible: &HashSet<String>,
) -> Result<(), SigilStitchError> {
    if let Some(ref body) = op.request_body {
        cb.add("// Prepare request body\n", vec![]);
        let names = resolve_param_names(op);
        let body_name = resolved_body(&names);
        let access = request_parameters_access(&body_name);

        if let Some(media_type) = preferred_request_media_type(body)
            && media_type_base(&media_type) == "multipart/form-data"
        {
            if let Some(parts) =
                multipart_parts_for(body, &media_type, ir, property_naming_camel_case)
            {
                if !body.required {
                    cb.add_code(sigil_quote!(TypeScript {
                        let requestBody: Blob | undefined = undefined;
                    })?);
                    cb.add(
                        &format!("if ({access} !== undefined && {access} !== null) {{\n"),
                        vec![],
                    );
                    emit_multipart_blob_setup(cb)?;
                } else {
                    emit_multipart_blob_setup(cb)?;
                }
                for part in parts {
                    let part_access = format!("{access}{}", ts_property_access(&part.field_name));
                    if part.required {
                        emit_multipart_blob_part(cb, &part, &part_access, convertible)?;
                    } else {
                        cb.add(
                            &format!(
                                "if ({part_access} !== undefined && {part_access} !== null) {{\n"
                            ),
                            vec![],
                        );
                        emit_multipart_blob_part(cb, &part, &part_access, convertible)?;
                        cb.add("}\n", vec![]);
                    }
                }
                emit_multipart_blob_finish(cb, body.required)?;
                if !body.required {
                    cb.add("}\n", vec![]);
                }
            } else {
                emit_unsupported_ts_body(
                    cb,
                    &access,
                    body.required,
                    "unsupported multipart request body: schema must be object-shaped",
                )?;
            }
        } else if let Some(media_type) = preferred_request_media_type(body)
            && is_unsupported_ts_request_media_type(&media_type)
        {
            emit_unsupported_ts_body(
                cb,
                &access,
                body.required,
                &format!("unsupported request body media type: {media_type}"),
            )?;
        } else if property_naming_camel_case {
            let json_type = preferred_request_media_type(body).and_then(|media_type| {
                if is_json_media_type(&media_type) {
                    body.content.get(media_type.as_str())
                } else {
                    None
                }
            });
            if let Some(to_json) =
                json_type.and_then(|ty| body_to_json_expr(ty, &access, convertible))
            {
                cb.add_code(sigil_quote!(TypeScript {
                    const requestBody = $L(to_json.as_str());
                })?);
            } else {
                cb.add_code(sigil_quote!(TypeScript {
                    const requestBody = $L(access.as_str());
                })?);
            }
        } else {
            cb.add_code(sigil_quote!(TypeScript {
                const requestBody = $L(access.as_str());
            })?);
        }
    }
    Ok(())
}

fn emit_unsupported_ts_body(
    cb: &mut sigil_stitch::code_block::CodeBlockBuilder,
    access: &str,
    required: bool,
    message: &str,
) -> Result<(), SigilStitchError> {
    cb.add_code(
        sigil_quote!(TypeScript {
            $if(required) {
                const requestBody = (() => { throw new Error($S(message)); })();
            } $else {
                const requestBody = $L(access) === undefined || $L(access) === null ? undefined : (() => { throw new Error($S(message)); })();
            }
        })
        ?,
    );
    Ok(())
}

fn is_unsupported_ts_request_media_type(media_type: &str) -> bool {
    let base = media_type_base(media_type);
    is_xml_media_type(media_type) || base == "application/x-www-form-urlencoded"
}

fn emit_multipart_blob_setup(
    cb: &mut sigil_stitch::code_block::CodeBlockBuilder,
) -> Result<(), SigilStitchError> {
    cb.add_code(
        sigil_quote!(TypeScript {
            const multipartBoundary = $S("----openapi-nexus-") + Math.random().toString(16).slice(2);
            const multipartChunks: Array<string | Blob> = [];
            headerParameters[$S("Content-Type")] = $S("multipart/form-data; boundary=") + multipartBoundary;
        })
        ?,
    );
    Ok(())
}

fn emit_multipart_blob_finish(
    cb: &mut sigil_stitch::code_block::CodeBlockBuilder,
    required: bool,
) -> Result<(), SigilStitchError> {
    let closing_boundary_tail = ts_string_literal("--\r\n");
    cb.add_code(sigil_quote!(TypeScript {
        multipartChunks.push($S("--") + multipartBoundary + $L(closing_boundary_tail));
        $if(required) {
            const requestBody = new Blob(multipartChunks);
        } $else {
            requestBody = new Blob(multipartChunks);
        }
    })?);
    Ok(())
}

fn emit_multipart_blob_part(
    cb: &mut sigil_stitch::code_block::CodeBlockBuilder,
    part: &MultipartPart,
    part_access: &str,
    convertible: &HashSet<String>,
) -> Result<(), SigilStitchError> {
    if part.value_encoding == MultipartValueEncoding::Unsupported {
        cb.add_code(sigil_quote!(TypeScript {
            throw new Error($S("unsupported multipart part content type"));
        })?);
        return Ok(());
    }
    if part.is_binary {
        cb.add("{\n", vec![]);
        let header_prefix = format!(
            "\r\nContent-Disposition: form-data; name=\"{}\"; filename=\"",
            multipart_header_quoted(&part.wire_name)
        );
        let header_suffix = format!(
            "\"\r\nContent-Type: {}\r\n\r\n",
            multipart_header_value(&part.content_type)
        );
        let filename_literal = ts_string_literal(&part.wire_name);
        cb.add_code(
            sigil_quote!(TypeScript {
                const multipartFilename = $T(rt_value("uploadFileFilename"))($L(part_access), $L(filename_literal.as_str()));
            })
            ?,
        );
        let header_prefix_literal = ts_string_literal(&header_prefix);
        let header_suffix_literal = ts_string_literal(&header_suffix);
        cb.add_code(
            sigil_quote!(TypeScript {
                multipartChunks.push($S("--") + multipartBoundary + $L(header_prefix_literal.as_str()) + $T(rt_value("multipartHeaderValue"))(multipartFilename) + $L(header_suffix_literal.as_str()));
            })
            ?,
        );
    } else {
        let disposition = format!(
            "form-data; name=\"{}\"",
            multipart_header_quoted(&part.wire_name)
        );
        let header_tail = format!(
            "\r\nContent-Disposition: {}\r\nContent-Type: {}\r\n\r\n",
            disposition,
            multipart_header_value(&part.content_type)
        );
        let header_tail_literal = ts_string_literal(&header_tail);
        cb.add_code(sigil_quote!(TypeScript {
            multipartChunks.push($S("--") + multipartBoundary + $L(header_tail_literal));
        })?);
    }
    if part.is_binary {
        cb.add_code(sigil_quote!(TypeScript {
            multipartChunks.push($T(rt_value("uploadFileData"))($L(part_access)));
        })?);
    } else {
        let value_expr = if part.value_encoding == MultipartValueEncoding::Json {
            let json_value = multipart_part_to_json_expr(part, part_access, convertible)
                .unwrap_or_else(|| part_access.to_string());
            format!("JSON.stringify({json_value})")
        } else {
            format!("String({part_access})")
        };
        cb.add_code(sigil_quote!(TypeScript {
            multipartChunks.push($L(value_expr));
        })?);
    }
    let crlf_literal = ts_string_literal("\r\n");
    cb.add_code(sigil_quote!(TypeScript {
        multipartChunks.push($L(crlf_literal));
    })?);
    if part.is_binary {
        cb.add("}\n", vec![]);
    }
    Ok(())
}

fn emit_make_request(
    cb: &mut sigil_stitch::code_block::CodeBlockBuilder,
    op: &IrOperation,
    has_body: bool,
) {
    let method = op.method.to_uppercase();
    let request_with_body = format!(
        "const response = await this.request({{\n    path: urlPath,\n    method: '{}',\n    headers: headerParameters,\n    query: queryParameters,\n    body: requestBody,\n}}, initOverrides);",
        method
    );
    let request_without_body = format!(
        "const response = await this.request({{\n    path: urlPath,\n    method: '{}',\n    headers: headerParameters,\n    query: queryParameters,\n    body: undefined,\n}}, initOverrides);",
        method
    );
    cb.add("// Make request\n", vec![]);
    cb.add_code(
        sigil_quote!(TypeScript {
            $if(has_body) {
                $L(request_with_body)
            } $else {
                $L(request_without_body)
            }
        })
        .expect("make request block builds"),
    );
    cb.add_line();
}

fn emit_response_handler(
    cb: &mut sigil_stitch::code_block::CodeBlockBuilder,
    op: &IrOperation,
    property_naming_camel_case: bool,
    convertible: &HashSet<String>,
) -> Result<(), SigilStitchError> {
    cb.add_code(sigil_quote!(TypeScript {
        $comment("Handle responses")
    })?);
    cb.add_code(build_error_dispatch(
        op,
        property_naming_camel_case,
        convertible,
    )?);

    let mut numeric: Vec<(u16, &IrResponse)> = Vec::new();
    let mut wildcards: Vec<(&str, &IrResponse)> = Vec::new();
    let mut default: Option<&IrResponse> = None;
    for resp in op
        .responses
        .iter()
        .filter(|resp| is_success_status(&resp.status))
    {
        if resp.status.eq_ignore_ascii_case("default") {
            default = Some(resp);
        } else if let Ok(code) = resp.status.parse::<u16>() {
            numeric.push((code, resp));
        } else {
            wildcards.push((&resp.status, resp));
        }
    }
    numeric.sort_by_key(|(code, _)| *code);

    let fallback_has_body = op
        .responses
        .iter()
        .filter(|resp| is_success_status(&resp.status))
        .any(|r| !r.content.is_empty());

    let fallback = if let Some(response) = default {
        build_response_return(response, property_naming_camel_case, convertible)?
    } else {
        build_fallback_return(fallback_has_body)?
    };
    if numeric.is_empty() && wildcards.is_empty() {
        cb.add_code(fallback);
        return Ok(());
    }

    let mut branches = Vec::new();
    for response in numeric
        .iter()
        .map(|(_, response)| *response)
        .chain(wildcards.iter().map(|(_, response)| *response))
    {
        branches.push((
            response_status_condition(&response.status)?,
            build_response_return(response, property_naming_camel_case, convertible)?,
        ));
    }
    cb.add_code(build_status_dispatch(branches, fallback)?);
    Ok(())
}

fn response_status_condition(status: &str) -> Result<CodeBlock, SigilStitchError> {
    if let Ok(code) = status.parse::<u16>() {
        sigil_quote!(TypeScript { response.status === $L(code.to_string()) })
    } else {
        let (low, high) = wildcard_status_range(status);
        sigil_quote!(TypeScript { response.status >= $L(low.to_string()) && response.status < $L(high.to_string()) })
    }
}

fn build_status_dispatch(
    branches: Vec<(CodeBlock, CodeBlock)>,
    fallback: CodeBlock,
) -> Result<CodeBlock, SigilStitchError> {
    let mut cb = CodeBlock::builder();
    let has_branches = !branches.is_empty();
    for (index, (condition, body)) in branches.into_iter().enumerate() {
        let branch = if index == 0 {
            sigil_quote!(TypeScript {
                if ($C(condition)) {
                    $C(body)
                }
            })
        } else {
            sigil_quote!(TypeScript {
                else if ($C(condition)) {
                    $C(body)
                }
            })
        }?;
        cb.add_code(branch);
    }
    if has_branches {
        cb.add_code(sigil_quote!(TypeScript {
            else {
                $C(fallback)
            }
        })?);
    } else {
        // A bare scope is ambiguous with an object literal in sigil_quote!;
        // the structural block API expresses that intent without raw braces.
        cb.begin_control_flow_with_intent(sigil_stitch::code_node::BlockIntent::Generic, "", ());
        cb.add_code(fallback);
        cb.end_control_flow();
    }
    cb.build()
}

fn wildcard_status_range(status: &str) -> (u16, u16) {
    match status.to_uppercase().as_str() {
        "1XX" => (100, 200),
        "2XX" => (200, 300),
        "3XX" => (300, 400),
        "4XX" => (400, 500),
        "5XX" => (500, 600),
        _ => (0, 1000),
    }
}

fn is_success_status(status: &str) -> bool {
    status
        .parse::<u16>()
        .is_ok_and(|code| (200..300).contains(&code))
        || status.eq_ignore_ascii_case("2XX")
}

fn build_error_dispatch(
    op: &IrOperation,
    property_naming_camel_case: bool,
    convertible: &HashSet<String>,
) -> Result<CodeBlock, SigilStitchError> {
    let mut numeric = Vec::new();
    let mut wildcards = Vec::new();
    let mut default = None;
    for resp in op
        .responses
        .iter()
        .filter(|resp| !is_success_status(&resp.status))
    {
        if resp.status.eq_ignore_ascii_case("default") {
            default = Some(resp);
        } else if let Ok(code) = resp.status.parse::<u16>() {
            numeric.push((code, resp));
        } else {
            wildcards.push((&resp.status, resp));
        }
    }
    numeric.sort_by_key(|(code, _)| *code);

    let mut used = HashSet::new();
    let mut branches = Vec::new();
    for resp in numeric
        .iter()
        .map(|(_, response)| *response)
        .chain(wildcards.iter().map(|(_, response)| *response))
    {
        let kind = unique_ts_kind(&response_detail_kind(&resp.status), &mut used);
        branches.push((
            response_status_condition(&resp.status)?,
            build_error_throw_for_response(
                op,
                resp,
                &kind,
                property_naming_camel_case,
                convertible,
            )?,
        ));
    }

    let fallback = if let Some(resp) = default {
        let kind = unique_ts_kind(&response_detail_kind(&resp.status), &mut used);
        build_error_throw_for_response(op, resp, &kind, property_naming_camel_case, convertible)?
    } else {
        build_error_throw(
            op,
            "unexpected",
            sigil_quote!(TypeScript { response.status })?,
            TypeName::primitive("Blob"),
            sigil_quote!(TypeScript { errorRaw.clone().blob() })?,
        )?
    };
    let dispatch = build_status_dispatch(branches, fallback)?;
    sigil_quote!(TypeScript {
        if (!(response.status >= 200 && response.status < 300)) {
            const errorRaw = response.clone();
            $C(dispatch)
        }
    })
}

fn build_error_throw_for_response(
    op: &IrOperation,
    resp: &IrResponse,
    kind: &str,
    property_naming_camel_case: bool,
    convertible: &HashSet<String>,
) -> Result<CodeBlock, SigilStitchError> {
    let status = if let Ok(code) = resp.status.parse::<u16>() {
        sigil_quote!(TypeScript { response.status as $L(code.to_string()) })
    } else {
        sigil_quote!(TypeScript { response.status })
    }?;
    build_error_throw(
        op,
        kind,
        status,
        response_value_type(resp),
        error_value_expr(resp, property_naming_camel_case, convertible)?,
    )
}

fn build_error_throw(
    op: &IrOperation,
    kind: &str,
    status: CodeBlock,
    value_type: TypeName,
    value: CodeBlock,
) -> Result<CodeBlock, SigilStitchError> {
    let class_name = error_class_name(op);
    let cached_type = TypeName::union(vec![
        type_application(TypeName::primitive("Promise"), vec![value_type]),
        TypeName::primitive("undefined"),
    ]);
    // sigil_quote! normalizes inline object literals without brace padding.
    // Keep the existing brace padding with static spaces; values stay structured.
    sigil_quote!(TypeScript {
        let errorValue: $T(cached_type);
        throw new $N(class_name)(response, {$L(" ")$N("kind"): $S(kind), status: $C(status), raw: response, value: () => errorValue ??= $C(value)$L(" ")});
    })
}

fn error_value_expr(
    resp: &IrResponse,
    property_naming_camel_case: bool,
    convertible: &HashSet<String>,
) -> Result<CodeBlock, SigilStitchError> {
    let kind = classify_response(resp);
    if property_naming_camel_case
        && let ResponseKind::Json(Some(_)) = &kind
        && let Some(from_json) = response_from_json_transformer(resp, convertible)?
    {
        let wrapper_type =
            type_application(rt_value("JSONApiResponse"), vec![response_value_type(resp)]);
        return sigil_quote!(TypeScript {
            new $T(wrapper_type)(errorRaw.clone(), $C(from_json)).value()
        });
    }
    let wrapper_type = match kind {
        ResponseKind::Json(body_ty) => type_application(
            rt_value("JSONApiResponse"),
            vec![body_ty.unwrap_or_else(|| TypeName::primitive("unknown"))],
        ),
        ResponseKind::Text => rt_value("TextApiResponse"),
        ResponseKind::Blob => rt_value("BlobApiResponse"),
        ResponseKind::None => rt_value("VoidApiResponse"),
    };
    sigil_quote!(TypeScript { new $T(wrapper_type)(errorRaw.clone()).value() })
}

/// `return new Wrapper(response) as Wrapper<Body> & { status: X };`
fn build_response_return(
    resp: &IrResponse,
    property_naming_camel_case: bool,
    convertible: &HashSet<String>,
) -> Result<CodeBlock, SigilStitchError> {
    let kind = classify_response(resp);
    let status_ty = match resp.status.parse::<u16>() {
        Ok(n) => n.to_string(),
        Err(_) => "number".to_string(),
    };

    // When camelCase is enabled and the response is JSON with a Named type,
    // inject a fromJSON transformer: `new JSONApiResponse<Pet>(response, (json) => petFromJSON(json as Pet$Wire))`
    if property_naming_camel_case
        && let ResponseKind::Json(Some(_)) = &kind
        && let Some(from_json) = response_from_json_transformer(resp, convertible)?
    {
        let wrapper_type = match kind {
            ResponseKind::Json(Some(body_ty)) => {
                type_application(rt_value("JSONApiResponse"), vec![body_ty])
            }
            _ => unreachable!(),
        };
        let status_type = response_status_type(&status_ty);
        return sigil_quote!(TypeScript {
            return new $T(rt_value("JSONApiResponse"))(response, $C(from_json)) as $T(wrapper_type)$L(" & ")$T(status_type);
        });
    }

    let wrapper_value = match kind {
        ResponseKind::Json(_) => rt_value("JSONApiResponse"),
        ResponseKind::Text => rt_value("TextApiResponse"),
        ResponseKind::Blob => rt_value("BlobApiResponse"),
        ResponseKind::None => rt_value("VoidApiResponse"),
    };
    let wrapper_type = match kind.clone() {
        ResponseKind::Json(body_ty) => {
            let body = body_ty.unwrap_or_else(|| TypeName::primitive("unknown"));
            type_application(rt_value("JSONApiResponse"), vec![body])
        }
        ResponseKind::Text => rt_value("TextApiResponse"),
        ResponseKind::Blob => rt_value("BlobApiResponse"),
        ResponseKind::None => rt_value("VoidApiResponse"),
    };
    let status_type = response_status_type(&status_ty);

    sigil_quote!(TypeScript {
        return new $T(wrapper_value)(response) as $T(wrapper_type)$L(" & ")$T(status_type);
    })
}

fn response_status_type(status_ty: &str) -> TypeName {
    // TypeName has no inline object-type variant yet. Keep this raw member
    // narrow so the response wrapper remains a semantic imported type.
    TypeName::raw(&format!("{{ status: {status_ty} }}"))
}

/// `return new JSONApiResponse(response) as JSONApiResponse<unknown> & { status: 2xx };`
/// (or VoidApiResponse equivalent when no body appears anywhere).
fn build_fallback_return(any_body: bool) -> Result<CodeBlock, SigilStitchError> {
    let json_response = rt_value("JSONApiResponse");
    let void_response = rt_value("VoidApiResponse");
    let unknown = TypeName::primitive("unknown");
    let success_response = rt_type("HttpSuccessResponse");
    sigil_quote!(TypeScript {
        $if(any_body) {
            return new $T(json_response.clone())(response) as $T(json_response)<$T(unknown)> & $T(success_response.clone());
        } $else {
            return new $T(void_response.clone())(response) as $T(void_response) & $T(success_response);
        }
    })
}

// ============================================================================
// Convenience method
// ============================================================================

fn build_convenience_method(op: &IrOperation) -> Result<FunSpec, SigilStitchError> {
    let method_base = op.operation_id.to_lower_camel_case();
    let raw_name = format!("{}Raw", method_base);

    let mut fb = FunSpec::builder(&method_base).is_async();

    for param in method_param_specs(op) {
        fb = fb.add_param(param);
    }
    let body_ty = convenience_body_type(op);
    fb = fb.returns(vec![type_application(
        TypeName::primitive("Promise"),
        vec![body_ty.clone()],
    )]);

    let args_list = raw_call_args(op);
    let is_void = is_void_type(&body_ty);
    let call_expr = format!("this.{raw_name}({args_list})");
    let body = sigil_quote!(TypeScript {
        const response = await $L(call_expr);
        $if(is_void) {
            return await response.value();
        } $else {
            return await response.value() as $T(body_ty);
        }
    })
    .expect("CodeBlock builds");
    fb = fb.body(body);

    fb.build()
}

fn is_void_type(ty: &TypeName) -> bool {
    is_primitive_type(ty, "void")
}

fn is_unknown_type(ty: &TypeName) -> bool {
    is_primitive_type(ty, "unknown")
}

fn is_primitive_type(ty: &TypeName, primitive: &str) -> bool {
    let Ok(val) = serde_json::to_value(ty) else {
        return false;
    };
    let Ok(primitive_val) = serde_json::to_value(TypeName::primitive(primitive)) else {
        return false;
    };
    val == primitive_val
}

// ============================================================================
// Parameter helpers
// ============================================================================

fn method_param_specs(op: &IrOperation) -> Vec<ParameterSpec> {
    let mut out = Vec::new();
    if operation_has_params(op) {
        let iface_name = format!(
            "Api{}Request",
            op.operation_id.to_lower_camel_case().to_pascal_case()
        );
        out.push(
            ParameterSpec::builder("requestParameters", TypeName::raw(&iface_name))
                .build()
                .expect("ParameterSpec builds"),
        );
    }
    out.push(init_overrides_param());
    out
}

fn init_overrides_param() -> ParameterSpec {
    ParameterSpec::builder("initOverrides?", init_overrides_type())
        .build()
        .expect("ParameterSpec builds")
}

fn init_overrides_type() -> TypeName {
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

fn raw_return_type(op: &IrOperation) -> TypeName {
    // Returns `Promise<{OpId}RawResponse>` — the alias itself lives in the
    // same file (see `build_response_aliases_block`) so no import is needed.
    type_application(
        TypeName::primitive("Promise"),
        vec![TypeName::raw(&raw_response_alias_name(op))],
    )
}

fn raw_response_member(resp: &IrResponse, kind: &ResponseKind) -> TypeName {
    let status_literal = resp
        .status
        .parse::<u16>()
        .ok()
        .map(|n| format!("{{ status: {n} }}"))
        .unwrap_or_else(|| "{ status: number }".to_string());
    let wrapper_type = match kind.clone() {
        ResponseKind::Json(body_ty) => {
            let body = body_ty.unwrap_or_else(|| TypeName::primitive("unknown"));
            type_application(rt_value("JSONApiResponse"), vec![body])
        }
        ResponseKind::Text => rt_value("TextApiResponse"),
        ResponseKind::Blob => rt_value("BlobApiResponse"),
        ResponseKind::None => rt_value("VoidApiResponse"),
    };
    TypeName::intersection(vec![wrapper_type, TypeName::raw(&status_literal)])
}

fn fallback_member(any_body: bool) -> TypeName {
    let wrapper = if any_body {
        type_application(
            rt_value("JSONApiResponse"),
            vec![TypeName::primitive("unknown")],
        )
    } else {
        rt_value("VoidApiResponse")
    };
    TypeName::intersection(vec![wrapper, rt_type("HttpSuccessResponse")])
}

fn convenience_body_type(op: &IrOperation) -> TypeName {
    let mut members: Vec<TypeName> = Vec::new();
    let mut any_body = false;
    for resp in op
        .responses
        .iter()
        .filter(|resp| is_success_status(&resp.status))
    {
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

fn response_value_type(resp: &IrResponse) -> TypeName {
    match classify_response(resp) {
        ResponseKind::Json(Some(body)) => body,
        ResponseKind::Json(None) => TypeName::primitive("unknown"),
        ResponseKind::Text => TypeName::primitive("string"),
        ResponseKind::Blob => TypeName::primitive("Blob"),
        ResponseKind::None => TypeName::primitive("void"),
    }
}

/// Stable de-dup of union members by `Debug` representation (cheap, correct
/// for sigil's `TypeName` variants).
fn dedup_union_members(members: Vec<TypeName>) -> Vec<TypeName> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut out: Vec<TypeName> = Vec::new();
    for m in members {
        let key = format!("{:?}", m);
        if seen.insert(key) {
            out.push(m);
        }
    }
    out
}

fn dedup_union(members: Vec<TypeName>) -> TypeName {
    let mut out = dedup_union_members(members);
    if out.iter().any(is_unknown_type) {
        return TypeName::primitive("unknown");
    }
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
    Json(Option<TypeName>),
    Text,
    Blob,
    None,
}

fn classify_response(resp: &IrResponse) -> ResponseKind {
    let Some((media_type, ty)) = pick_response_content(resp) else {
        return ResponseKind::None;
    };
    if is_json_media_type(&media_type) {
        return ResponseKind::Json(Some(type_expr_to_typename(ty)));
    }
    match media_type_base(&media_type).as_str() {
        "text/plain"
        | "text/html"
        | "application/xml"
        | "text/xml"
        | "application/x-www-form-urlencoded"
        | "text/event-stream" => ResponseKind::Text,
        _ => ResponseKind::Blob,
    }
}

fn unique_ts_kind(desired: &str, used: &mut HashSet<String>) -> String {
    if used.insert(desired.to_string()) {
        return desired.to_string();
    }
    for i in 2..=u32::MAX {
        let candidate = format!("{desired}{i}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!("kind collision space exhausted")
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
fn rt_value(name: &str) -> TypeName {
    TypeName::importable(RUNTIME_MOD, name)
}

/// Runtime symbol imported type-only: emits `import type { Name } from '../runtime/runtime'`.
fn rt_type(name: &str) -> TypeName {
    TypeName::importable_type(RUNTIME_MOD, name)
}

// ============================================================================
// IrTypeExpr → TypeName
// ============================================================================

fn type_expr_to_typename(expr: &IrTypeExpr) -> TypeName {
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
        IrTypeExpr::StringLiteral(s) => TypeName::string_literal(s.clone()),
        IrTypeExpr::StringEnum(values) => TypeName::union(
            values
                .iter()
                .map(|v| TypeName::string_literal(v.clone()))
                .collect(),
        ),
        IrTypeExpr::Map(inner) => type_application(
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
        IrPrimitive::Binary => "Blob | File",
        IrPrimitive::String
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

// ============================================================================
// Property naming (camelCase) helpers for API layer
// ============================================================================

/// For a JSON response with a Named body type, produce the transformer lambda:
/// `(json) => petFromJSON(json as Pet$Wire)`
/// For Array(Named), produce: `(json) => (json as Pet$Wire[]).map(petFromJSON)`
/// Returns None for non-Named types (no conversion needed).
fn response_from_json_transformer(
    resp: &IrResponse,
    convertible: &HashSet<String>,
) -> Result<Option<CodeBlock>, SigilStitchError> {
    let Some((media_type, type_expr)) = pick_response_content(resp) else {
        return Ok(None);
    };
    if !is_json_media_type(&media_type) {
        return Ok(None);
    }
    let (name, is_array) = match type_expr {
        IrTypeExpr::Named(name) if convertible.contains(name) => (name, false),
        IrTypeExpr::Array(inner) => match inner.as_ref() {
            IrTypeExpr::Named(name) if convertible.contains(name) => (name, true),
            _ => return Ok(None),
        },
        _ => return Ok(None),
    };
    let pascal = name.to_pascal_case();
    let module = format!("../models/{pascal}");
    let from_fn = TypeName::importable(&module, &format!("{}FromJSON", fn_base_name(&pascal)));
    let wire = TypeName::importable_type(&module, &format!("{pascal}$Wire"));
    let transformer = if is_array {
        sigil_quote!(TypeScript {
            (json) => (json as $T(TypeName::array(wire))).map($T(from_fn))
        })
    } else {
        sigil_quote!(TypeScript {
            (json) => $T(from_fn)(json as $T(wire))
        })
    }?;
    Ok(Some(transformer))
}

/// For a request body with a Named type, produce the toJSON call:
/// `petToJSON(requestParameters.body)` or `requestParameters.body.map(petToJSON)`
/// Returns None for non-Named types (no conversion needed).
fn body_to_json_expr(
    content_type: &IrTypeExpr,
    access: &str,
    convertible: &HashSet<String>,
) -> Option<String> {
    match content_type {
        IrTypeExpr::Named(name) if convertible.contains(name) => {
            let pascal = name.to_pascal_case();
            let to_fn = format!("{}ToJSON", fn_base_name(&pascal));
            Some(format!("{to_fn}({access})"))
        }
        IrTypeExpr::Array(inner) => match inner.as_ref() {
            IrTypeExpr::Named(name) if convertible.contains(name) => {
                let pascal = name.to_pascal_case();
                let to_fn = format!("{}ToJSON", fn_base_name(&pascal));
                Some(format!("{access}.map({to_fn})"))
            }
            _ => None,
        },
        _ => None,
    }
}

fn multipart_part_to_json_expr(
    part: &MultipartPart,
    access: &str,
    convertible: &HashSet<String>,
) -> Option<String> {
    body_to_json_expr(&part.type_expr, access, convertible)
}

struct MultipartPart {
    wire_name: String,
    field_name: String,
    type_expr: IrTypeExpr,
    is_binary: bool,
    required: bool,
    content_type: String,
    value_encoding: MultipartValueEncoding,
}

fn multipart_parts_for(
    body: &IrRequestBody,
    media_type: &str,
    ir: &IrSpec,
    property_naming_camel_case: bool,
) -> Option<Vec<MultipartPart>> {
    multipart_parts_for_request_body(body, media_type, ir).map(|parts| {
        parts
            .into_iter()
            .map(|part| MultipartPart {
                field_name: if property_naming_camel_case {
                    part.wire_name.to_lower_camel_case()
                } else {
                    part.wire_name.clone()
                },
                wire_name: part.wire_name,
                type_expr: part.type_expr,
                is_binary: part.is_binary,
                required: part.required,
                content_type: part.content_type,
                value_encoding: part.value_encoding,
            })
            .collect()
    })
}

fn ts_property_access(field_name: &str) -> String {
    if is_js_identifier(field_name) {
        format!(".{field_name}")
    } else {
        format!("[{}]", ts_string_literal(field_name))
    }
}

fn ts_string_literal(value: &str) -> String {
    format!(
        "'{}'",
        value
            .replace('\\', "\\\\")
            .replace('\'', "\\'")
            .replace('\r', "\\r")
            .replace('\n', "\\n")
    )
}

fn multipart_header_quoted(value: &str) -> String {
    multipart_header_value(value)
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

fn multipart_header_value(value: &str) -> String {
    value.replace(['\r', '\n'], "")
}

/// Collect unique Named type references from request bodies only
/// that are in the convertible set (i.e., have $Wire/fromJSON/toJSON emitted).
fn collect_request_named_types(
    ops: &[&IrOperation],
    ir: &IrSpec,
    property_naming_camel_case: bool,
    convertible: &HashSet<String>,
) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for op in ops {
        if let Some(ref body) = op.request_body
            && let Some(media_type) = preferred_request_media_type(body)
        {
            if is_json_media_type(&media_type) {
                if let Some(ty) = body.content.get(media_type.as_str()) {
                    collect_convertible_named_refs(ty, convertible, &mut names);
                }
            } else if media_type_base(&media_type) == "multipart/form-data"
                && let Some(parts) =
                    multipart_parts_for(body, &media_type, ir, property_naming_camel_case)
            {
                for part in parts {
                    if part.value_encoding == MultipartValueEncoding::Json {
                        collect_convertible_named_refs(&part.type_expr, convertible, &mut names);
                    }
                }
            }
        }
    }
    names
}

fn pick_response_content(resp: &IrResponse) -> Option<(String, &IrTypeExpr)> {
    pick_media_type_ref(&resp.content, |media_type| {
        media_type_base(media_type) == "application/json"
    })
    .or_else(|| pick_media_type_ref(&resp.content, is_json_media_type))
    .or_else(|| {
        pick_media_type_ref(&resp.content, |media_type| {
            media_type_base(media_type) == "application/octet-stream"
        })
    })
    .or_else(|| {
        pick_media_type_ref(&resp.content, |media_type| {
            media_type_base(media_type) == "text/plain"
        })
    })
    .or_else(|| pick_media_type_ref(&resp.content, is_xml_media_type))
    .or_else(|| pick_first_media_type_ref(&resp.content))
}

fn pick_media_type(
    content: &indexmap::IndexMap<String, IrTypeExpr>,
    predicate: impl Fn(&str) -> bool,
) -> Option<String> {
    content
        .keys()
        .find(|media_type| predicate(media_type))
        .cloned()
}

fn pick_first_media_type(content: &indexmap::IndexMap<String, IrTypeExpr>) -> Option<String> {
    content.keys().next().cloned()
}

fn pick_media_type_ref(
    content: &indexmap::IndexMap<String, IrTypeExpr>,
    predicate: impl Fn(&str) -> bool,
) -> Option<(String, &IrTypeExpr)> {
    content
        .iter()
        .find(|(media_type, _)| predicate(media_type))
        .map(|(media_type, t)| (media_type.clone(), t))
}

fn pick_first_media_type_ref(
    content: &indexmap::IndexMap<String, IrTypeExpr>,
) -> Option<(String, &IrTypeExpr)> {
    content
        .iter()
        .next()
        .map(|(media_type, t)| (media_type.clone(), t))
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

/// Collect unique Named type references from JSON responses only
/// that are in the convertible set (i.e., have $Wire/fromJSON/toJSON emitted).
fn collect_response_named_types(
    ops: &[&IrOperation],
    convertible: &HashSet<String>,
) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for op in ops {
        for resp in &op.responses {
            if let Some((media_type, ty)) = pick_response_content(resp)
                && is_json_media_type(&media_type)
            {
                collect_convertible_named_refs(ty, convertible, &mut names);
            }
        }
    }
    names
}

fn collect_convertible_named_refs(
    expr: &IrTypeExpr,
    convertible: &HashSet<String>,
    names: &mut BTreeSet<String>,
) {
    match expr {
        IrTypeExpr::Named(name) if convertible.contains(name) => {
            names.insert(name.clone());
        }
        IrTypeExpr::Array(inner) | IrTypeExpr::Nullable(inner) | IrTypeExpr::Map(inner) => {
            collect_convertible_named_refs(inner, convertible, names);
        }
        IrTypeExpr::Union(members) => {
            for member in members {
                collect_convertible_named_refs(member, convertible, names);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sigil_stitch::assert_rendered;

    fn render_header_accessor(kind: ResponseHeaderValueKind) -> String {
        let header = ResponseHeaderPlan {
            wire_name: "X-Value".to_string(),
            value_kind: kind,
        };
        let accessor =
            build_ts_header_accessor("getXValueHeader", None, &header, "response.headers", false)
                .expect("TypeScript header accessor builds");
        FileSpec::builder_with("header.ts", TypeScript::new())
            .add_function(accessor)
            .build()
            .expect("TypeScript header accessor file builds")
            .render(100)
            .expect("TypeScript header accessor renders")
    }

    fn type_json(ty: &TypeName) -> serde_json::Value {
        serde_json::to_value(ty).expect("TypeName serializes")
    }

    #[test]
    fn dedup_union_collapses_unknown_members() {
        let ty = dedup_union(vec![
            TypeName::primitive("unknown"),
            TypeName::importable_type("../models/ErrorResponse", "ErrorResponse"),
        ]);

        assert_eq!(type_json(&ty), type_json(&TypeName::primitive("unknown")));
    }

    #[test]
    fn numeric_response_header_accessors_reject_empty_values() {
        for kind in [
            ResponseHeaderValueKind::Integer,
            ResponseHeaderValueKind::Number,
        ] {
            let accessor = render_header_accessor(kind);
            assert!(accessor.contains(".test(value)"));
        }

        let integer = render_header_accessor(ResponseHeaderValueKind::Integer);
        assert!(integer.contains("Number.isSafeInteger(parsed)"));
    }

    #[test]
    fn string_singletons_use_structured_string_literal_type_names() {
        assert_eq!(
            type_expr_to_typename(&IrTypeExpr::StringLiteral("quoted'".to_string())),
            TypeName::string_literal("quoted'")
        );
        assert_eq!(
            type_expr_to_typename(&IrTypeExpr::StringEnum(vec![
                "a".to_string(),
                "b".to_string()
            ])),
            TypeName::union(vec![
                TypeName::string_literal("a"),
                TypeName::string_literal("b"),
            ])
        );
    }

    #[test]
    fn response_status_type_keeps_inline_object_spacing() -> Result<(), SigilStitchError> {
        let wrapper = TypeName::primitive("JSONApiResponse<Foo>");
        let status_type = response_status_type("200");
        let block = sigil_quote!(TypeScript {
            return new $T(wrapper.clone())(response) as $T(wrapper)$L(" & ")$T(status_type);
        })?;

        assert_rendered!(
            TypeScript::new(),
            width = 100,
            block,
            "return new JSONApiResponse<Foo>(response) as JSONApiResponse<Foo> & { status: 200 };\n"
        );
        Ok(())
    }

    fn response_dispatch_ir() -> IrSpec {
        let parsed = crate::parser::parse_content_yaml(include_str!(
            "../../../../tests/fixtures/valid/typescript-response-dispatch.yaml"
        ))
        .expect("response dispatch fixture parses");
        crate::ir::lower::lower(parsed).expect("response dispatch fixture lowers")
    }

    #[test]
    fn error_throw_composes_lazy_array_conversion_and_imports() -> Result<(), SigilStitchError> {
        let ir = response_dispatch_ir();
        let op = ir
            .operations
            .iter()
            .find(|op| op.operation_id == "readMixed")
            .expect("mixed response operation exists");
        let response = op
            .responses
            .iter()
            .find(|response| response.status == "401")
            .expect("array error exists");
        let convertible = HashSet::from(["Detail".to_string()]);
        let block =
            build_error_throw_for_response(op, response, "unauthorized", true, &convertible)?;

        assert_rendered!(
            TypeScript::new(),
            width = 100,
            block,
            r#"import type { Detail, Detail$Wire } from '../models/Detail';
import { detailFromJSON } from '../models/Detail';
import { JSONApiResponse } from '../runtime/runtime';

let errorValue: Promise<Detail[]> | undefined;
throw new ReadMixedError(response, { kind: 'unauthorized', status: response.status as 401, raw: response, value: () => errorValue ??= new JSONApiResponse<Detail[]>(errorRaw.clone(), (json) => (json as Detail$Wire[]).map(detailFromJSON)).value() });
"#
        );
        Ok(())
    }

    #[test]
    fn status_dispatch_composes_conditions_and_returns_without_extra_newlines()
    -> Result<(), SigilStitchError> {
        let ir = response_dispatch_ir();
        let op = ir
            .operations
            .iter()
            .find(|op| op.operation_id == "readMixed")
            .expect("mixed response operation exists");
        let mut branches = Vec::new();
        for status in ["201", "2XX"] {
            let response = op
                .responses
                .iter()
                .find(|response| response.status == status)
                .expect("success response exists");
            branches.push((
                response_status_condition(status)?,
                build_response_return(response, false, &HashSet::new())?,
            ));
        }
        let block = build_status_dispatch(branches, build_fallback_return(true)?)?;
        assert_rendered!(
            TypeScript::new(),
            width = 100,
            block,
            r#"import type { Detail } from '../models/Detail';
import type { HttpSuccessResponse } from '../runtime/runtime';
import { JSONApiResponse } from '../runtime/runtime';

if (response.status === 201) {
  return new JSONApiResponse(response) as JSONApiResponse<Detail> & { status: 201 };
}
else if (response.status >= 200 && response.status < 300) {
  return new JSONApiResponse(response) as JSONApiResponse<Detail[]> & { status: number };
}
else {
  return new JSONApiResponse(response) as JSONApiResponse<unknown> &HttpSuccessResponse;
}
"#
        );
        Ok(())
    }

    #[test]
    fn default_only_error_keeps_its_scope_and_lazy_named_conversion() -> Result<(), SigilStitchError>
    {
        let ir = response_dispatch_ir();
        let op = ir
            .operations
            .iter()
            .find(|op| op.operation_id == "readDefault")
            .expect("default-only operation exists");
        let block = build_error_dispatch(op, true, &HashSet::from(["Detail".to_string()]))?;
        assert_rendered!(
            TypeScript::new(),
            width = 100,
            block,
            r#"import type { Detail, Detail$Wire } from '../models/Detail';
import { detailFromJSON } from '../models/Detail';
import { JSONApiResponse } from '../runtime/runtime';

if (!(response.status >= 200 && response.status < 300)) {
  const errorRaw = response.clone();
   {
    let errorValue: Promise<Detail> | undefined;
    throw new ReadDefaultError(response, { kind: 'default', status: response.status, raw: response, value: () => errorValue ??= new JSONApiResponse<Detail>(errorRaw.clone(), (json) => detailFromJSON(json as Detail$Wire)).value() });
  }
}
"#
        );
        Ok(())
    }
}
