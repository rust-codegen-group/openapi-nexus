//! Transport-independent Python multipart and lazy union-error conversion.

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
    let assignments = parts
        .iter()
        .map(|part| {
            let source = sigil_quote!(Python { $N(body_name).$N(part.field_name.as_str()) })
                .expect("Python multipart input access builds");
            let assignment = if part.value_encoding == MultipartValueEncoding::Json
                && values.needs_conversion(&part.type_expr)
            {
                let converted = values.call(&part.type_expr, source.clone(), Direction::To);
                sigil_quote!(Python {
                    files[$S(part.wire_name.as_str())] = (None, json.dumps($C(converted)), $S(part.content_type.as_str()))
                })
                .expect("Python union multipart assignment builds")
            } else {
                multipart_part_assignment(part, source.clone(), ir)
            };
            if part.required {
                Ok(assignment)
            } else {
                sigil_quote!(Python {
                    if $C(source) is not None {
                        $C(assignment)
                    }
                })
            }
        })
        .collect::<Result<Vec<_>, SigilStitchError>>()
        .expect("Python multipart assignments build");
    sigil_quote!(Python {
        files: dict[str, object] = {}
        $if(required) { $C_each(assignments) } $else {
            if $N(body_name) is not None {
                $C_each(assignments)
            }
        }
    })
    .expect("Python multipart body builds")
}

fn multipart_part_assignment(part: &MultipartPart, access: CodeBlock, ir: &IrSpec) -> CodeBlock {
    let json_value = render_multipart_json_value(access.clone(), &part.type_expr, ir);
    if part.is_binary {
        sigil_quote!(Python {
            files[$S(part.wire_name.as_str())] = ($C(access.clone()).filename_or_default($S(part.wire_name.as_str())), $C(access.clone()).data, $S(part.content_type.as_str()))
        })
        .expect("Python binary multipart assignment builds")
    } else if part.value_encoding == MultipartValueEncoding::Json {
        sigil_quote!(Python {
            files[$S(part.wire_name.as_str())] = (None, json.dumps($C(json_value)), $S(part.content_type.as_str()))
        })
        .expect("Python JSON multipart assignment builds")
    } else if part.value_encoding == MultipartValueEncoding::Unsupported {
        sigil_quote!(Python { raise ValueError("unsupported multipart part content type") })
            .expect("Python unsupported multipart assignment builds")
    } else {
        sigil_quote!(Python {
            files[$S(part.wire_name.as_str())] = (None, str($C(access)), $S(part.content_type.as_str()))
        })
        .expect("Python scalar multipart assignment builds")
    }
}

fn render_multipart_json_value(access: CodeBlock, expr: &IrTypeExpr, ir: &IrSpec) -> CodeBlock {
    match expr {
        IrTypeExpr::Named(name) if is_object_schema(name, ir) => {
            sigil_quote!(Python { $C(access).to_dict() })
                .expect("Python multipart object value builds")
        }
        IrTypeExpr::Nullable(inner) => render_multipart_json_value(access, inner, ir),
        IrTypeExpr::Array(inner) => {
            if let IrTypeExpr::Named(name) = inner.as_ref()
                && is_object_schema(name, ir)
            {
                sigil_quote!(Python { [item.to_dict() for item in $C(access)] })
                    .expect("Python multipart object array value builds")
            } else {
                access
            }
        }
        _ => access,
    }
}

pub(super) fn union_error_body_property(
    type_expr: &IrTypeExpr,
    ir: &IrSpec,
    values: &ValuePlan<'_>,
) -> FunSpec {
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
}
