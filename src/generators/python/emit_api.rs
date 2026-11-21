//! API emission for IR operations (Python API classes).
//!
//! Uses sigil-stitch high-level APIs (TypeSpec, FunSpec, TypeName, FileSpec) for
//! structured code generation with automatic import tracking. Groups operations
//! by tag, emits one `apis/{tag}_api.py` per tag.

use std::collections::{BTreeMap, HashSet};

use crate::codegen::traits::file_writer::FileInfo;
use crate::generators::multipart::{MultipartValueEncoding, multipart_parts_for_request_body};
use crate::generators::request_inputs::{RequestInputPlan, request_input_for_operation};
use crate::generators::response_headers::{
    ResponseHeaderPlan, ResponseHeaderValueKind, collect_response_headers,
};
use crate::generators::response_names::response_entry_name as response_variant_name;
use crate::ir::types::{
    IrOperation, IrParameter, IrPrimitive, IrRequestBody, IrResponse, IrSpec, IrTypeExpr,
    ParameterLocation,
};
use heck::{ToPascalCase, ToSnakeCase};
use sigil_stitch::code_block::CodeBlock;
use sigil_stitch::lang::python::{Python, keyword_argument};
use sigil_stitch::prelude::*;
use sigil_stitch::spec::fun_spec::FunSpecBuilder;

use super::emit_models::{
    api_type_name, future_annotations_header, is_object_schema, python_field_name,
};
use crate::generators::python::emit_api_values::{
    MultipartPart, multipart_data, union_error_body_property,
};
use crate::generators::python::operation_names::plan_python_operation_names;
use crate::generators::python::response_headers::build_header_accessors;
use crate::generators::python::value::{Direction, UntaggedUnionStrategy, ValuePlan};
use crate::generators::type_application;

#[derive(Clone, Copy)]
pub(super) enum Transport {
    Httpx,
    Requests,
}

impl Transport {
    fn raw_body_keyword(self) -> &'static str {
        match self {
            Self::Httpx => "content",
            Self::Requests => "data",
        }
    }

    fn response_reason(self) -> CodeBlock {
        match self {
            Self::Httpx => sigil_quote!(Python { response.reason_phrase }),
            Self::Requests => sigil_quote!(Python { response.reason }),
        }
        .expect("Python response reason builds")
    }
}

/// Generate API files with the same union policy used by the model emitter.
pub(super) fn generate_api_files_with_strategy(
    ir: &IrSpec,
    header: &str,
    request_inputs: &RequestInputPlan,
    strategy: UntaggedUnionStrategy,
    transport: Transport,
) -> Vec<FileInfo> {
    let values = ValuePlan::new(ir, strategy);
    let by_tag = group_by_tag(&ir.operations);
    let mut files = Vec::with_capacity(by_tag.len());
    for (tag, ops) in &by_tag {
        let stem = tag.to_snake_case();
        let filename = format!("{stem}_api.py");
        let body = emit_api_file(tag, ops, ir, header, request_inputs, &values, transport);
        files.push(FileInfo::api(filename, body));
    }
    files
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
    header: &str,
    request_inputs: &RequestInputPlan,
    values: &ValuePlan<'_>,
    transport: Transport,
) -> String {
    let class_name = format!("{}Api", tag.to_pascal_case());
    let mut plans: Vec<OpPlan> = ops
        .iter()
        .map(|op| plan_operation(op, ir, request_inputs))
        .collect();
    let operation_names =
        plan_python_operation_names(plans.iter().map(|plan| plan.method_name.clone()));
    for (plan, names) in plans.iter_mut().zip(operation_names) {
        plan.method_name = names.method;
        plan.with_http_info_method_name = names.with_http_info_method;
    }

    let client_type = TypeName::importable("..runtime.client", "Client");

    // __init__ method via FunSpec
    let init_body = sigil_quote!(Python { self._client = client })
        .expect("Python client initializer body builds");
    let init = FunSpec::builder("__init__")
        .add_param(ParameterSpec::of("self", TypeName::primitive("")))
        .add_param(ParameterSpec::of("client", client_type))
        .returns(vec![TypeName::primitive("None")])
        .body(init_body)
        .build()
        .expect("__init__ FunSpec builds");

    let mut cls = TypeSpec::builder(&class_name, TypeKind::Class).add_method(init);

    for plan in &plans {
        cls = cls.add_method(build_api_method_with_http_info(plan, ir, values, transport));
        cls = cls.add_method(build_api_method(plan, values));
    }

    let mut fb = FileSpec::builder_with(&format!("{}_api.py", tag.to_snake_case()), Python::new())
        .header(future_annotations_header());
    for plan in &plans {
        if !plan.success_headers.is_empty() {
            fb = fb.add_type(build_response_class(plan, values));
        }
    }
    for error_class in build_error_classes(&plans, ir, values) {
        fb = fb.add_type(error_class);
    }
    fb = fb.add_type(cls.build().expect("API TypeSpec builds"));
    if plans
        .iter()
        .flat_map(|plan| &plan.error_responses)
        .any(|response| response.decoding == Some(ResponseDecoding::Json))
    {
        fb = fb.add_import(ImportSpec::side_effect("json"));
    }
    if plans.iter().any(|plan| {
        plan.body.as_ref().is_some_and(|body| {
            body.multipart_parts.as_ref().is_some_and(|parts| {
                parts
                    .iter()
                    .any(|part| values.needs_conversion(&part.type_expr))
            })
        })
    }) || ops.iter().any(|op| {
        op.request_body
            .iter()
            .flat_map(|body| body.content.values())
            .chain(
                op.responses
                    .iter()
                    .flat_map(|response| response.content.values()),
            )
            .any(|expr| values.needs_conversion(expr))
    }) {
        fb = fb.add_import(ImportSpec::named("..models", "_union"));
        fb = fb.add_import(ImportSpec::side_effect("json"));
    }
    if plans.iter().any(|plan| {
        plan.body.as_ref().is_some_and(|body| {
            body.multipart_parts.as_ref().is_some_and(|parts| {
                parts
                    .iter()
                    .any(|part| part.value_encoding == MultipartValueEncoding::Json)
            })
        })
    }) {
        fb = fb.add_import(ImportSpec::side_effect("json"));
    }
    if plans.iter().any(|plan| {
        plan.success_headers
            .iter()
            .chain(&plan.error_headers)
            .any(|header| {
                matches!(
                    header.value_kind,
                    ResponseHeaderValueKind::Integer | ResponseHeaderValueKind::Number
                )
            })
    }) {
        fb = fb.add_import(ImportSpec::side_effect("re"));
    }
    if plans.iter().any(|plan| {
        plan.success_headers
            .iter()
            .chain(&plan.error_headers)
            .any(|header| header.value_kind == ResponseHeaderValueKind::Number)
    }) {
        fb = fb.add_import(ImportSpec::side_effect("math"));
    }
    let file = fb.build().expect("API FileSpec builds");

    let body = file
        .render(super::PYTHON_LINE_LENGTH)
        .expect("Python API FileSpec renders");
    let mut content = String::with_capacity(header.len() + body.len());
    content.push_str(header);
    content.push_str(&body);
    content
}

fn build_api_method(plan: &OpPlan<'_>, values: &ValuePlan<'_>) -> FunSpec {
    let mut fun = FunSpec::builder(&plan.method_name);
    fun = add_api_method_params(fun, plan, values);
    fun = fun.returns(vec![response_payload_type(plan, values)]);

    if let Some(summary) = &plan.op.summary {
        fun = fun.doc(&format!("{summary}."));
    }

    let mut args: Vec<_> = plan
        .path_params
        .iter()
        .map(|param| identifier(&param.var_name))
        .collect();
    for param in plan.query_params.iter().chain(&plan.header_params) {
        if param.param.required {
            args.push(forward_argument(&param.var_name));
        }
    }
    if let Some(body) = &plan.body {
        args.push(forward_argument(&body.var_name));
    }
    for param in plan.query_params.iter().chain(&plan.header_params) {
        if !param.param.required {
            args.push(forward_argument(&param.var_name));
        }
    }
    fun = fun.body(
        sigil_quote!(Python {
            return self.$N(plan.with_http_info_method_name.as_str())($for(arg in args; separator = ", ") { $C(arg) }).data
        })
        .expect("convenience response body builds"),
    );

    fun.build().expect("API method FunSpec builds")
}

fn build_api_method_with_http_info(
    plan: &OpPlan<'_>,
    ir: &IrSpec,
    values: &ValuePlan<'_>,
    transport: Transport,
) -> FunSpec {
    let mut fun = FunSpec::builder(&plan.with_http_info_method_name);
    fun = add_api_method_params(fun, plan, values);
    fun = fun.returns(vec![response_metadata_type(plan, values)]);

    if let Some(summary) = &plan.op.summary {
        fun = fun.doc(&format!("{summary}, including HTTP response metadata."));
    }

    fun = fun.body(build_method_body(plan, ir, values, transport));
    fun.build()
        .expect("API with-http-info method FunSpec builds")
}

fn add_api_method_params(
    mut fun: FunSpecBuilder,
    plan: &OpPlan<'_>,
    values: &ValuePlan<'_>,
) -> FunSpecBuilder {
    fun = fun.add_param(ParameterSpec::of("self", TypeName::primitive("")));

    for p in &plan.path_params {
        fun = fun.add_param(ParameterSpec::of(
            &p.var_name,
            values.api_type(&p.param.type_expr),
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
                values.api_type(&p.param.type_expr),
            ));
        }
    }

    // Body param
    if let Some(b) = &plan.body {
        let ty = values.api_type(&b.type_expr);
        if b.required {
            fun = fun.add_param(ParameterSpec::of(&b.var_name, ty));
        } else {
            fun = fun.add_param(
                ParameterSpec::builder(&b.var_name, TypeName::optional(ty))
                    .default_value(sigil_quote!(Python { None }).expect("None"))
                    .build()
                    .expect("optional body param"),
            );
        }
    }

    // Optional query/header params last
    for p in plan.query_params.iter().chain(&plan.header_params) {
        if !p.param.required {
            let param_ty = values.api_type(&p.param.type_expr);
            let param_ty = if is_already_optional(&p.param.type_expr) {
                param_ty
            } else {
                TypeName::optional(param_ty)
            };
            fun = fun.add_param(
                ParameterSpec::builder(&p.var_name, param_ty)
                    .default_value(sigil_quote!(Python { None }).expect("None"))
                    .build()
                    .expect("optional param"),
            );
        }
    }

    fun
}

fn response_payload_type(plan: &OpPlan<'_>, values: &ValuePlan<'_>) -> TypeName {
    if plan.typed_responses.is_empty() {
        TypeName::primitive("None")
    } else {
        response_type_name(&plan.typed_responses[0], values)
    }
}

fn response_metadata_type(plan: &OpPlan<'_>, values: &ValuePlan<'_>) -> TypeName {
    if plan.success_headers.is_empty() {
        type_application(
            TypeName::importable("..runtime.client", "ApiResponse"),
            vec![response_payload_type(plan, values)],
        )
    } else {
        TypeName::primitive(&plan.response_type)
    }
}

fn build_method_body(
    plan: &OpPlan<'_>,
    ir: &IrSpec,
    values: &ValuePlan<'_>,
    transport: Transport,
) -> CodeBlock {
    let mut cb = CodeBlock::builder();

    // Path interpolation
    let path = if plan.path_params.is_empty() {
        sigil_quote!(Python { path = $S(plan.op.path.as_str()) })
    } else {
        let mut path_template = plan.op.path.clone();
        for p in &plan.path_params {
            let placeholder = format!("{{{}}}", p.param.name);
            let replacement = format!("{{{}}}", p.var_name);
            path_template = path_template.replace(&placeholder, &replacement);
        }
        sigil_quote!(Python { path = $V(path_template) })
    };
    cb.add_code(path.expect("Python request path builds"));

    let has_query = !plan.query_params.is_empty();
    if has_query {
        cb.add_code(
            sigil_quote!(Python { params: dict[str, str] = {} }).expect("Python query map builds"),
        );
        for p in &plan.query_params {
            cb.add_code(parameter_assignment("params", p));
        }
    }

    let body_content_type = plan.body.as_ref().and_then(|body| {
        (media_type_base(&body.media_type) != "multipart/form-data")
            .then_some(body.media_type.as_str())
    });
    let has_headers = !plan.header_params.is_empty() || body_content_type.is_some();
    if has_headers {
        cb.add_code(
            sigil_quote!(Python { headers: dict[str, str] = {} })
                .expect("Python header map builds"),
        );
        if let Some(media_type) = body_content_type {
            cb.add_code(
                sigil_quote!(Python { headers["Content-Type"] = $S(media_type) })
                    .expect("Python Content-Type assignment builds"),
            );
        }
        for p in &plan.header_params {
            cb.add_code(parameter_assignment("headers", p));
        }
    }

    let union_body = plan
        .body
        .as_ref()
        .is_some_and(|body| values.needs_conversion(&body.type_expr));
    if let Some(body) = &plan.body
        && union_body
    {
        let name = body.var_name.as_str();
        let converted = values.call(&body.type_expr, identifier(name), Direction::To);
        cb.add_code(sigil_quote!(Python {
            _union_body = $if(body.required) { $C(converted) } $else { None if $N(name) is None else $C(converted) }
        }).expect("Python HTTP union request conversion builds"));
    }

    // Request call
    let mut request_args = vec![
        sigil_quote!(Python { $S(plan.op.method.to_uppercase()) })
            .expect("Python HTTP method argument builds"),
        sigil_quote!(Python { path }).expect("Python HTTP path argument builds"),
    ];
    let mut union_json_argument = None;
    if has_query {
        request_args.push(forward_argument("params"));
    }
    if let Some(body) = &plan.body {
        if media_type_base(&body.media_type) == "multipart/form-data" {
            if let Some(parts) = &body.multipart_parts {
                cb.add_code(multipart_data(
                    &body.var_name,
                    body.required,
                    parts,
                    ir,
                    values,
                ));
                request_args.push(call_keyword(
                    "files",
                    sigil_quote!(Python { files if files else None })
                        .expect("Python multipart files argument builds"),
                ));
            } else {
                cb.add_code(
                    sigil_quote!(Python { raise ValueError("unsupported multipart request body: schema must be object-shaped") })
                        .expect("Python unsupported multipart error builds"),
                );
            }
        } else {
            match body.encoding {
                BodyEncoding::Json if union_body => {
                    let value = sigil_quote!(Python {
                        $if(body.required) { json.dumps(_union_body, allow_nan=False) } $else { None if $N(body.var_name.as_str()) is None else json.dumps(_union_body, allow_nan=False) }
                    })
                    .expect("Python union JSON value builds");
                    union_json_argument = Some(call_keyword(transport.raw_body_keyword(), value));
                }
                BodyEncoding::Json
                | BodyEncoding::FormUrlEncoded
                | BodyEncoding::TextPlain
                | BodyEncoding::OctetStream => {
                    let keyword = match body.encoding {
                        BodyEncoding::Json => "json",
                        BodyEncoding::FormUrlEncoded => "data",
                        _ => transport.raw_body_keyword(),
                    };
                    let value = if union_body {
                        identifier("_union_body")
                    } else {
                        request_body_value(body, ir)
                    };
                    request_args.push(call_keyword(keyword, value));
                }
                BodyEncoding::Xml | BodyEncoding::Other => {
                    let message =
                        format!("unsupported request body media type: {}", body.media_type);
                    cb.add_code(
                        sigil_quote!(Python {
                            $if(body.required) { raise ValueError($S(message)) } $else {
                                if $N(body.var_name.as_str()) is not None {
                                    raise ValueError($S(message))
                                }
                            }
                        })
                        .expect("Python unsupported request media error builds"),
                    );
                }
                BodyEncoding::Multipart => unreachable!("multipart handled separately"),
            }
        }
    }
    if has_headers {
        request_args.push(forward_argument("headers"));
    }
    if let Some(argument) = union_json_argument {
        request_args.push(argument);
    }

    let request_call = sigil_quote!(Python {
        response = self._client.request($for(arg in request_args; separator = ", ") { $C(arg) })
    })
    .expect("Python HTTP request call builds");
    cb.add_code(request_call);

    // Error handling
    cb.add_code(emit_error_raise(plan, transport.response_reason()));

    let data = match plan.typed_responses.first() {
        Some(response)
            if response.decoding == ResponseDecoding::Json
                && values.needs_conversion(&response.type_expr) =>
        {
            let converted = values.call(
                &response.type_expr,
                sigil_quote!(Python { response.json() }).expect("Python union JSON input builds"),
                Direction::From,
            );
            let ty = values.api_type(&response.type_expr);
            let converted = values.coerce(&response.type_expr, ty, converted);
            cb.add_code(
                sigil_quote!(Python { _union_data = $C(converted) })
                    .expect("Python union response conversion builds"),
            );
            identifier("_union_data")
        }
        Some(response) => render_response_parse(response, ir),
        None => sigil_quote!(Python { None }).expect("Python empty response value builds"),
    };
    let response_type = if plan.success_headers.is_empty() {
        TypeName::importable("..runtime.client", "ApiResponse")
    } else {
        TypeName::primitive(&plan.response_type)
    };
    cb.add_code(
        sigil_quote!(Python {
            data = $C(data)
            return $T(response_type)(data=data, status_code=response.status_code, headers=response.headers, raw=response)
        })
        .expect("metadata response result builds"),
    );

    cb.build().expect("API method body builds")
}

fn identifier(name: &str) -> CodeBlock {
    sigil_quote!(Python { $N(name) }).expect("Python identifier builds")
}

fn call_keyword(name: &str, value: CodeBlock) -> CodeBlock {
    keyword_argument(identifier(name), value)
}

fn forward_argument(name: &str) -> CodeBlock {
    let id = identifier(name);
    keyword_argument(id.clone(), id)
}

fn parameter_assignment(mapping: &str, binding: &ParamBinding<'_>) -> CodeBlock {
    let name = binding.var_name.as_str();
    let wire_name = binding.param.name.as_str();
    let stringify = render_stringify(name, &binding.param.type_expr);
    sigil_quote!(Python {
        $if(binding.param.required) {
            $N(mapping)[$S(wire_name)] = $C(stringify)
        } $else {
            if $N(name) is not None {
                $N(mapping)[$S(wire_name)] = $C(stringify)
            }
        }
    })
    .expect("Python parameter assignment builds")
}

fn render_stringify(var: &str, type_expr: &IrTypeExpr) -> CodeBlock {
    match type_expr.representation() {
        IrTypeExpr::Primitive(IrPrimitive::Boolean) => {
            sigil_quote!(Python { str($N(var)).lower() })
        }
        IrTypeExpr::Nullable(inner) => return render_stringify(var, inner),
        IrTypeExpr::Array(_) => sigil_quote!(Python { ",".join(str(v) for v in $N(var)) }),
        _ => sigil_quote!(Python { str($N(var)) }),
    }
    .expect("Python parameter value builds")
}

fn request_body_value(body: &BodyBinding, ir: &IrSpec) -> CodeBlock {
    let name = body.var_name.as_str();
    let value = if is_object_type(&body.type_expr, ir) {
        sigil_quote!(Python { $N(name).to_dict() }).expect("Python object request value builds")
    } else if is_array_of_objects(&body.type_expr, ir) {
        sigil_quote!(Python { [item.to_dict() for item in $N(name)] })
            .expect("Python object-array request value builds")
    } else {
        return identifier(name);
    };
    if body.required {
        value
    } else {
        sigil_quote!(Python { $C(value) if $N(name) is not None else None })
            .expect("Python optional request value builds")
    }
}

fn response_type_name(response: &TypedResponse, values: &ValuePlan<'_>) -> TypeName {
    match response.decoding {
        ResponseDecoding::Json => values.api_type(&response.type_expr),
        ResponseDecoding::Text => TypeName::primitive("str"),
        ResponseDecoding::Bytes => TypeName::primitive("bytes"),
    }
}

fn render_response_parse(response: &TypedResponse, ir: &IrSpec) -> CodeBlock {
    match response.decoding {
        ResponseDecoding::Json => {
            let type_expr = response.type_expr.representation();
            if matches!(
                type_expr,
                IrTypeExpr::Primitive(IrPrimitive::String | IrPrimitive::StringWithFormat(_))
            ) {
                return sigil_quote!(Python { response.text })
                    .expect("Python text response builds");
            }
            let json =
                sigil_quote!(Python { response.json() }).expect("Python JSON response builds");
            return render_json_assignment_value(type_expr, json, ir);
        }
        ResponseDecoding::Text => sigil_quote!(Python { response.text }),
        ResponseDecoding::Bytes => sigil_quote!(Python { response.content }),
    }
    .expect("Python response value builds")
}

/// Emit the right-hand side of a JSON assignment, including the existing
/// type-checker pragma for untyped JSON values. Do not embed it in an argument.
fn render_json_assignment_value(type_expr: &IrTypeExpr, json: CodeBlock, ir: &IrSpec) -> CodeBlock {
    match type_expr {
        IrTypeExpr::Named(name) if is_object_schema(name, ir) => {
            let model = api_type_name(type_expr);
            sigil_quote!(Python { $T(model).from_dict($C(json)) })
                .expect("Python object response value builds")
        }
        IrTypeExpr::Array(inner) if is_object_type(inner, ir) => {
            let model = api_type_name(inner);
            sigil_quote!(Python { [$T(model).from_dict(item) for item in $C(json)] })
                .expect("Python object-array response value builds")
        }
        _ => sigil_quote!(Python { $C(json) $comment("type: ignore[return-value]") })
            .expect("Python JSON response pragma builds"),
    }
}

fn build_error_classes(plans: &[OpPlan<'_>], ir: &IrSpec, values: &ValuePlan<'_>) -> Vec<TypeSpec> {
    let mut declarations = Vec::new();
    for plan in plans {
        let mut seen = HashSet::new();
        let mut alias_members = Vec::new();
        for response in &plan.error_responses {
            if seen.insert(&response.class_name) {
                alias_members.push(TypeName::primitive(&response.class_name));
                declarations.push(build_error_detail_class(response, ir, values));
            }
        }

        let unexpected = format!("{}Unexpected", plan.error_type.trim_end_matches("Error"));
        alias_members.push(TypeName::primitive(&unexpected));
        declarations.push(build_unexpected_error_class(&unexpected));

        let detail_type = format!("{}Detail", plan.error_type);
        let alias_target = if alias_members.len() == 1 {
            alias_members
                .pop()
                .expect("unexpected detail is always present")
        } else {
            TypeName::union(alias_members)
        };
        declarations.push(
            TypeSpec::builder(&detail_type, TypeKind::TypeAlias)
                .extends(alias_target)
                .build()
                .expect("Python error detail alias builds"),
        );
        declarations.push(build_error_class(plan, &detail_type));
    }
    declarations
}

fn mapping_type() -> TypeName {
    type_application(
        TypeName::importable("collections.abc", "Mapping"),
        vec![TypeName::primitive("str"), TypeName::primitive("str")],
    )
}

fn build_error_detail_class(
    response: &ErrorResponse,
    ir: &IrSpec,
    values: &ValuePlan<'_>,
) -> TypeSpec {
    let mut class = TypeSpec::builder(&response.class_name, TypeKind::Class);
    let typed = response.type_expr.as_ref().zip(response.decoding);
    let body = if let Some((type_expr, decoding)) = typed {
        let return_ty = error_body_return_type(type_expr, decoding, values);
        sigil_quote!(Python {
            self.status_code: int = status_code
            self.headers: $T(mapping_type()) = headers
            self.raw_body: bytes = raw_body
            self._body_loaded: bool = False
            self._body_value: $T(return_ty) | None = None
            self._body_error: Exception | None = None
        })
    } else {
        sigil_quote!(Python {
            self.status_code: int = status_code
            self.headers: $T(mapping_type()) = headers
            self.raw_body: bytes = raw_body
        })
    }
    .expect("Python error detail constructor body builds");
    let init = FunSpec::builder("__init__")
        .add_param(ParameterSpec::of("self", TypeName::primitive("")))
        .add_param(ParameterSpec::of("status_code", TypeName::primitive("int")))
        .add_param(ParameterSpec::of("headers", mapping_type()))
        .add_param(ParameterSpec::of("raw_body", TypeName::primitive("bytes")))
        .returns(vec![TypeName::primitive("None")])
        .body(body)
        .build()
        .expect("Python error detail constructor builds");
    class = class.add_method(init);
    if let Some((type_expr, decoding)) = typed {
        let property = if decoding == ResponseDecoding::Json && values.needs_conversion(type_expr) {
            union_error_body_property(type_expr, ir, values)
        } else {
            error_body_property(type_expr, decoding, ir)
        };
        class = class.add_method(property);
    }
    class.build().expect("Python error detail class builds")
}

fn build_unexpected_error_class(name: &str) -> TypeSpec {
    let init = FunSpec::builder("__init__")
        .add_param(ParameterSpec::of("self", TypeName::primitive("")))
        .add_param(ParameterSpec::of("status_code", TypeName::primitive("int")))
        .add_param(ParameterSpec::of("headers", mapping_type()))
        .add_param(ParameterSpec::of("raw_body", TypeName::primitive("bytes")))
        .returns(vec![TypeName::primitive("None")])
        .body(
            sigil_quote!(Python {
                self.status_code: int = status_code
                self.headers: $T(mapping_type()) = headers
                self.raw_body: bytes = raw_body
            })
            .expect("Python unexpected error constructor builds"),
        )
        .build()
        .expect("Python unexpected error constructor builds");
    let body = FunSpec::builder("body")
        .annotate(AnnotationSpec::new("property"))
        .add_param(ParameterSpec::of("self", TypeName::primitive("")))
        .returns(vec![TypeName::primitive("bytes")])
        .body(sigil_quote!(Python { return self.raw_body }).expect("Python error body builds"))
        .build()
        .expect("Python error body property builds");
    TypeSpec::builder(name, TypeKind::Class)
        .add_method(init)
        .add_method(body)
        .build()
        .expect("Python unexpected error class builds")
}

fn build_error_class(plan: &OpPlan<'_>, detail_type: &str) -> TypeSpec {
    let body = sigil_quote!(Python {
        self.detail: $T(TypeName::primitive(detail_type)) = detail
        super().__init__(status_code, status, body, headers=headers, response=response)
    })
    .expect("Python API error constructor body builds");
    let mut class = TypeSpec::builder(&plan.error_type, TypeKind::Class)
        .extends(TypeName::importable("..runtime.errors", "ApiError"));
    let init = FunSpec::builder("__init__")
        .add_param(ParameterSpec::of("self", TypeName::primitive("")))
        .add_param(ParameterSpec::of("status_code", TypeName::primitive("int")))
        .add_param(ParameterSpec::of("status", TypeName::primitive("str")))
        .add_param(ParameterSpec::of("body", TypeName::primitive("bytes")))
        .add_param(ParameterSpec::of(
            "detail",
            TypeName::primitive(detail_type),
        ))
        .add_param(
            ParameterSpec::builder("headers", TypeName::optional(mapping_type()))
                .default_value(sigil_quote!(Python { None }).expect("None default builds"))
                .build()
                .expect("optional headers parameter builds"),
        )
        .add_param(
            ParameterSpec::builder(
                "response",
                TypeName::optional(TypeName::primitive("object")),
            )
            .default_value(sigil_quote!(Python { None }).expect("None default builds"))
            .build()
            .expect("optional response parameter builds"),
        )
        .returns(vec![TypeName::primitive("None")])
        .body(body)
        .build()
        .expect("Python API error constructor builds");
    class = class.add_method(init);
    for accessor in build_header_accessors(&plan.error_headers, true) {
        class = class.add_method(accessor);
    }
    class.build().expect("Python API error class builds")
}

fn build_response_class(plan: &OpPlan<'_>, values: &ValuePlan<'_>) -> TypeSpec {
    let mut response =
        TypeSpec::builder(&plan.response_type, TypeKind::Class).extends(type_application(
            TypeName::importable("..runtime.client", "ApiResponse"),
            vec![response_payload_type(plan, values)],
        ));
    for accessor in build_header_accessors(&plan.success_headers, false) {
        response = response.add_method(accessor);
    }
    response.build().expect("Python response class builds")
}

fn error_body_property(type_expr: &IrTypeExpr, decoding: ResponseDecoding, ir: &IrSpec) -> FunSpec {
    let (value, return_type) = match decoding {
        ResponseDecoding::Json => (
            render_json_assignment_value(
                type_expr,
                sigil_quote!(Python { json.loads(self.raw_body.decode("utf-8")) })
                    .expect("Python error JSON value builds"),
                ir,
            ),
            api_type_name(type_expr),
        ),
        ResponseDecoding::Text => (
            sigil_quote!(Python { self.raw_body.decode("utf-8") })
                .expect("Python error text value builds"),
            TypeName::primitive("str"),
        ),
        ResponseDecoding::Bytes => (
            sigil_quote!(Python { self.raw_body }).expect("Python error bytes value builds"),
            TypeName::primitive("bytes"),
        ),
    };
    let fallible = decoding != ResponseDecoding::Bytes;
    let body = sigil_quote!(Python {
        if not self._body_loaded {
            $if(fallible) {
                try {
                    self._body_value = $C(value)
                } except Exception as exc {
                    self._body_error = exc
                }
            } $else {
                self._body_value = $C(value)
            }
            self._body_loaded = True
        }
        $if(fallible) {
            if self._body_error is not None {
                raise self._body_error
            }
        }
        if self._body_value is None {
            raise RuntimeError("error body was not decoded")
        }
        return self._body_value
    })
    .expect("Python cached error body builds");
    FunSpec::builder("body")
        .annotate(AnnotationSpec::new("property"))
        .add_param(ParameterSpec::of("self", TypeName::primitive("")))
        .returns(vec![return_type])
        .body(body)
        .build()
        .expect("Python error body property builds")
}

fn error_body_return_type(
    type_expr: &IrTypeExpr,
    decoding: ResponseDecoding,
    values: &ValuePlan<'_>,
) -> TypeName {
    match decoding {
        ResponseDecoding::Json => values.api_type(type_expr),
        ResponseDecoding::Text => TypeName::primitive("str"),
        ResponseDecoding::Bytes => TypeName::primitive("bytes"),
    }
}

fn emit_error_raise(plan: &OpPlan<'_>, reason: CodeBlock) -> CodeBlock {
    let error_type = TypeName::primitive(&plan.error_type);
    sigil_quote!(Python {
        if not (200 <= response.status_code < 300) {
            $C(error_detail_assignment(plan))
            raise $T(error_type)(response.status_code, $C(reason), response.content, detail, headers=response.headers, response=response)
        }
    })
    .expect("Python error raise builds")
}

fn error_detail_assignment(plan: &OpPlan<'_>) -> CodeBlock {
    let mut exact: Vec<_> = plan
        .error_responses
        .iter()
        .filter_map(|r| r.status.parse::<u16>().ok().map(|status| (status, r)))
        .collect();
    exact.sort_by_key(|&(status, _)| status);
    let wildcards = plan
        .error_responses
        .iter()
        .filter(|r| r.status.ends_with("XX"));
    let unexpected = format!("{}Unexpected", plan.error_type.trim_end_matches("Error"));
    let fallback = plan
        .error_responses
        .iter()
        .find(|r| r.status.eq_ignore_ascii_case("default"))
        .map_or(unexpected.as_str(), |r| r.class_name.as_str());

    let cases: Vec<(String, &str)> = exact
        .into_iter()
        .map(|(status, r)| {
            (
                format!("response.status_code == {status}"),
                r.class_name.as_str(),
            )
        })
        .chain(wildcards.map(|r| {
            let (low, high) = wildcard_status_range(&r.status);
            (
                format!("{low} <= response.status_code < {high}"),
                r.class_name.as_str(),
            )
        }))
        .collect();
    let fallback = detail_ctor_statement(fallback);
    let Some((first_condition, first_class)) = cases.first() else {
        return fallback;
    };
    let first = detail_ctor_statement(first_class);
    let rest = cases
        .iter()
        .skip(1)
        .map(|(condition, class_name)| (condition.clone(), detail_ctor_statement(class_name)))
        .collect::<Vec<_>>();
    sigil_quote!(Python {
        if $L(first_condition.as_str()) {
            $C(first)
        }
        $for((condition, body) in &rest) {
            elif $L(condition.as_str()) {
                $C(body.clone())
            }
        }
        else {
            $C(fallback)
        }
    })
    .expect("Python error detail assignment builds")
}

fn detail_ctor_statement(class_name: &str) -> CodeBlock {
    sigil_quote!(Python {
        detail = $N(class_name)(response.status_code, response.headers, response.content)
    })
    .expect("Python error detail constructor builds")
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

fn is_object_type(type_expr: &IrTypeExpr, ir: &IrSpec) -> bool {
    if let IrTypeExpr::Named(name) = type_expr {
        return is_object_schema(name, ir);
    }
    false
}

fn is_array_of_objects(type_expr: &IrTypeExpr, ir: &IrSpec) -> bool {
    if let IrTypeExpr::Array(inner) = type_expr
        && let IrTypeExpr::Named(name) = inner.as_ref()
    {
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
    with_http_info_method_name: String,
    response_type: String,
    error_type: String,
    path_params: Vec<ParamBinding<'a>>,
    query_params: Vec<ParamBinding<'a>>,
    header_params: Vec<ParamBinding<'a>>,
    body: Option<BodyBinding>,
    typed_responses: Vec<TypedResponse>,
    error_responses: Vec<ErrorResponse>,
    success_headers: Vec<ResponseHeaderPlan>,
    error_headers: Vec<ResponseHeaderPlan>,
}

struct ParamBinding<'a> {
    param: &'a IrParameter,
    var_name: String,
}

struct BodyBinding {
    var_name: String,
    type_expr: IrTypeExpr,
    required: bool,
    media_type: String,
    encoding: BodyEncoding,
    multipart_parts: Option<Vec<MultipartPart>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BodyEncoding {
    Json,
    Multipart,
    FormUrlEncoded,
    Xml,
    TextPlain,
    OctetStream,
    Other,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ResponseDecoding {
    Json,
    Text,
    Bytes,
}

struct TypedResponse {
    type_expr: IrTypeExpr,
    decoding: ResponseDecoding,
}

#[derive(Clone)]
struct ErrorResponse {
    status: String,
    class_name: String,
    type_expr: Option<IrTypeExpr>,
    decoding: Option<ResponseDecoding>,
}

fn plan_operation<'a>(
    op: &'a IrOperation,
    ir: &IrSpec,
    request_inputs: &RequestInputPlan,
) -> OpPlan<'a> {
    let op_id = sanitize_operation_id(&op.operation_id, &op.method, &op.path);
    let method_name = op_id.to_snake_case();
    let response_type = format!("{}ApiResponse", op_id.to_pascal_case());
    let error_type = format!("{}Error", op_id.to_pascal_case());

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
        .and_then(|b| plan_body(op, b, ir, request_inputs, &mut used_names));

    let typed_responses = op
        .responses
        .iter()
        .filter(|r| is_success_status(&r.status))
        .filter_map(plan_response)
        .collect();
    let error_responses = op
        .responses
        .iter()
        .filter(|r| !is_success_status(&r.status))
        .map(|r| plan_error_response(&op_id, r))
        .collect();
    let success_headers = collect_response_headers(
        op.responses
            .iter()
            .filter(|response| is_success_status(&response.status)),
        ir,
    );
    let error_headers = collect_response_headers(
        op.responses
            .iter()
            .filter(|response| !is_success_status(&response.status)),
        ir,
    );

    OpPlan {
        op,
        with_http_info_method_name: format!("{method_name}_with_http_info"),
        method_name,
        response_type,
        error_type,
        path_params,
        query_params,
        header_params,
        body,
        typed_responses,
        error_responses,
        success_headers,
        error_headers,
    }
}

fn plan_body(
    op: &IrOperation,
    b: &IrRequestBody,
    ir: &IrSpec,
    request_inputs: &RequestInputPlan,
    used_names: &mut HashSet<String>,
) -> Option<BodyBinding> {
    let (media_type, t) = pick_body_content(b)?;
    let encoding = body_encoding(&media_type);
    let var_name = unique_name("body", used_names);
    let multipart_parts = if media_type_base(&media_type) == "multipart/form-data" {
        multipart_parts_for(b, &media_type, ir)
    } else {
        None
    };
    Some(BodyBinding {
        var_name,
        type_expr: if encoding == BodyEncoding::Multipart {
            request_input_for_operation(request_inputs, op, &media_type)
                .map(|input| IrTypeExpr::Named(input.name.clone()))
                .unwrap_or(t)
        } else {
            t
        },
        required: b.required,
        media_type,
        encoding,
        multipart_parts,
    })
}

fn plan_response(r: &IrResponse) -> Option<TypedResponse> {
    let (media_type, t) = pick_response_content(r)?;
    Some(TypedResponse {
        type_expr: t,
        decoding: response_decoding(&media_type),
    })
}

fn plan_error_response(op_id: &str, r: &IrResponse) -> ErrorResponse {
    match pick_response_content(r) {
        Some((media_type, t)) => ErrorResponse {
            status: r.status.clone(),
            class_name: format!(
                "{}{}",
                op_id.to_pascal_case(),
                response_variant_name(&r.status)
            ),
            type_expr: Some(t),
            decoding: Some(response_decoding(&media_type)),
        },
        None => ErrorResponse {
            status: r.status.clone(),
            class_name: format!(
                "{}{}",
                op_id.to_pascal_case(),
                response_variant_name(&r.status)
            ),
            type_expr: None,
            decoding: None,
        },
    }
}

fn is_success_status(status: &str) -> bool {
    status
        .parse::<u16>()
        .is_ok_and(|code| (200..300).contains(&code))
        || status.eq_ignore_ascii_case("2XX")
}

fn body_encoding(media_type: &str) -> BodyEncoding {
    let base = media_type_base(media_type);
    if base == "multipart/form-data" {
        BodyEncoding::Multipart
    } else if is_json_media_type(media_type) {
        BodyEncoding::Json
    } else if base == "application/x-www-form-urlencoded" {
        BodyEncoding::FormUrlEncoded
    } else if is_xml_media_type(media_type) {
        BodyEncoding::Xml
    } else if base == "text/plain" {
        BodyEncoding::TextPlain
    } else if base == "application/octet-stream" {
        BodyEncoding::OctetStream
    } else {
        BodyEncoding::Other
    }
}

fn response_decoding(media_type: &str) -> ResponseDecoding {
    let base = media_type_base(media_type);
    if is_json_media_type(media_type) {
        ResponseDecoding::Json
    } else if base == "text/plain" || is_xml_media_type(media_type) {
        ResponseDecoding::Text
    } else {
        ResponseDecoding::Bytes
    }
}

fn pick_body_content(body: &IrRequestBody) -> Option<(String, IrTypeExpr)> {
    pick_media_type(&body.content, |media_type| {
        media_type_base(media_type) == "application/json"
    })
    .or_else(|| pick_media_type(&body.content, is_json_media_type))
    .or_else(|| {
        pick_media_type(&body.content, |media_type| {
            media_type_base(media_type) == "multipart/form-data"
        })
    })
    .or_else(|| {
        pick_media_type(&body.content, |media_type| {
            media_type_base(media_type) == "application/x-www-form-urlencoded"
        })
    })
    .or_else(|| pick_media_type(&body.content, is_xml_media_type))
    .or_else(|| {
        pick_media_type(&body.content, |media_type| {
            media_type_base(media_type) == "text/plain"
        })
    })
    .or_else(|| {
        pick_media_type(&body.content, |media_type| {
            media_type_base(media_type) == "application/octet-stream"
        })
    })
    .or_else(|| pick_first_content(&body.content))
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

fn multipart_parts_for(
    body: &IrRequestBody,
    media_type: &str,
    ir: &IrSpec,
) -> Option<Vec<MultipartPart>> {
    multipart_parts_for_request_body(body, media_type, ir).map(|parts| {
        parts
            .into_iter()
            .map(|part| MultipartPart {
                field_name: python_field_name(&part.wire_name),
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

/// Returns true if the type expression is already nullable (wrapped in None),
/// so that the caller can avoid double-wrapping with TypeName::optional.
fn is_already_optional(expr: &IrTypeExpr) -> bool {
    matches!(expr.representation(), IrTypeExpr::Nullable(_))
}
