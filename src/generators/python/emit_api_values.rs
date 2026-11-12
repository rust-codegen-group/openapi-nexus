//! Transport-independent Python multipart and lazy union-error conversion.

use sigil_stitch::lang::python::Python;
use sigil_stitch::prelude::*;

use super::emit_models::is_object_schema;
use super::value::{Direction, ValuePlan};
use crate::generators::multipart::MultipartValueEncoding;
use crate::ir::types::{IrSpec, IrTypeExpr};

pub(super) struct MultipartPart {
    pub wire_name: String,
    pub field_name: String,
    pub type_expr: IrTypeExpr,
    pub is_binary: bool,
    pub required: bool,
    pub content_type: String,
    pub value_encoding: MultipartValueEncoding,
}

pub(super) fn multipart_data(
    body_name: &str,
    required: bool,
    parts: &[MultipartPart],
    ir: &IrSpec,
    values: &ValuePlan<'_>,
) -> CodeBlock {
    if parts
        .iter()
        .any(|part| values.needs_conversion(&part.type_expr))
    {
        let assignments = parts.iter().map(|part| {
            let source = sigil_quote!(Python { $N(body_name).$N(part.field_name.as_str()) }).expect("Python multipart input access builds");
            let assignment = if part.value_encoding == MultipartValueEncoding::Json && values.needs_conversion(&part.type_expr) {
                let converted = values.call(&part.type_expr, source.clone(), Direction::To);
                sigil_quote!(Python {
                    files[$S(part.wire_name.as_str())] = (None, json.dumps($C(converted)), $S(part.content_type.as_str()))
                }).expect("Python union multipart assignment builds")
            } else {
                multipart_part_assignment(part, &format!("{body_name}.{}", part.field_name), ir)
            };
            sigil_quote!(Python {
                $if(part.required) { $C(assignment) } $else {
                    if $C(source) is not None { $C(assignment) }
                }
            }).expect("Python optional multipart field builds")
        }).collect::<Vec<_>>();
        return sigil_quote!(Python {
            files: dict[str, object] = {}
            $if(required) { $C_each(assignments) } $else {
                if $N(body_name) is not None { $C_each(assignments) }
            }
        })
        .expect("Python multipart body with union parts builds");
    }

    // Keep the existing non-union rendering byte-identical while both transports
    // share it; union-aware source above uses structured fragments throughout.
    let mut cb = CodeBlock::builder();
    cb.add_statement("files: dict[str, object] = {}", ());
    if !required {
        cb.add_statement(&format!("if {body_name} is not None:%>"), ());
    }
    for part in parts {
        let access = format!("{body_name}.{}", part.field_name);
        if part.required {
            cb.add_code(multipart_part_assignment(part, &access, ir));
        } else {
            cb.add_statement(&format!("if {access} is not None:%>"), ());
            cb.add_code(multipart_part_assignment(part, &access, ir));
            cb.add_statement("%<", ());
        }
    }
    if !required {
        cb.add_statement("%<", ());
    }
    cb.build().expect("Python multipart body builds")
}

fn multipart_part_assignment(part: &MultipartPart, access: &str, ir: &IrSpec) -> CodeBlock {
    let binary_stmt = format!(
        "files[\"{}\"] = ({}.filename_or_default(\"{}\"), {}.data, \"{}\")",
        part.wire_name, access, part.wire_name, access, part.content_type
    );
    let json_value = render_multipart_json_value(access, &part.type_expr, ir);
    let json_stmt = format!(
        "files[\"{}\"] = (None, json.dumps({json_value}), \"{}\")",
        part.wire_name, part.content_type
    );
    let unsupported_stmt = "raise ValueError(\"unsupported multipart part content type\")";
    let scalar_stmt = format!(
        "files[\"{}\"] = (None, str({access}), \"{}\")",
        part.wire_name, part.content_type
    );

    sigil_quote!(Python {
        $if(part.is_binary) {
            $L(binary_stmt.as_str())
        } $else_if(part.value_encoding == MultipartValueEncoding::Json) {
            $L(json_stmt.as_str())
        } $else_if(part.value_encoding == MultipartValueEncoding::Unsupported) {
            $L(unsupported_stmt)
        } $else {
            $L(scalar_stmt.as_str())
        }
    })
    .expect("multipart part assignment builds")
}

fn render_multipart_json_value(access: &str, expr: &IrTypeExpr, ir: &IrSpec) -> String {
    match expr {
        IrTypeExpr::Named(name) if is_object_schema(name, ir) => format!("{access}.to_dict()"),
        IrTypeExpr::Nullable(inner) => render_multipart_json_value(access, inner, ir),
        IrTypeExpr::Array(inner) => {
            if let IrTypeExpr::Named(name) = inner.as_ref()
                && is_object_schema(name, ir)
            {
                format!("[item.to_dict() for item in {access}]")
            } else {
                access.to_string()
            }
        }
        _ => access.to_string(),
    }
}

pub(super) fn union_error_body_property(
    type_expr: &IrTypeExpr,
    ir: &IrSpec,
    values: &ValuePlan<'_>,
) -> CodeBlock {
    let converted = values.call(
        type_expr,
        sigil_quote!(Python { json.loads(self.raw_body.decode("utf-8")) })
            .expect("Python union error JSON input builds"),
        Direction::From,
    );
    let ty = values.api_type(type_expr);
    let converted = values.coerce(type_expr, ty.clone(), converted);
    let cached =
        sigil_quote!(Python { self._body_value }).expect("Python cached union error value builds");
    let cached = if crate::generators::union_match::accepts_null(type_expr, ir)
        || values.is_dynamic(type_expr)
    {
        cached
    } else {
        values.coerce(type_expr, ty.clone(), cached)
    };
    let body = sigil_quote!(Python {
        if not self._body_loaded {
            try {
                self._body_value = $C(converted)
            } except Exception as exc {
                self._body_error = exc
            }
            self._body_loaded = True
        }
        if self._body_error is not None {
            raise self._body_error
        }
        return $C(cached)
    })
    .expect("Python union error body builds");
    FunSpec::builder("body")
        .annotate(AnnotationSpec::new("property"))
        .add_param(ParameterSpec::of("self", TypeName::primitive("")))
        .returns(vec![ty])
        .body(body)
        .build()
        .expect("Python union error body property builds")
        .emit(&Python::new(), DeclarationContext::Member)
        .expect("Python union error body property emits")
}
