use heck::ToSnakeCase;
use sigil_stitch::prelude::*;

use crate::generators::response_headers::{
    ResponseHeaderPlan, ResponseHeaderValueKind, unique_response_header_accessor_names,
};

pub(super) fn build_header_accessors(
    headers: &[ResponseHeaderPlan],
    headers_optional: bool,
) -> Vec<FunSpec> {
    let method_names = unique_response_header_accessor_names(headers, |wire_name| {
        let mut base = wire_name.to_snake_case();
        if base.is_empty() || base.starts_with(|character: char| character.is_ascii_digit()) {
            base = format!("header_{base}");
        }
        format!("{base}_header")
    });
    headers
        .iter()
        .zip(method_names)
        .map(|(header, method_name)| {
            FunSpec::builder(&method_name)
                .annotate(AnnotationSpec::new("property"))
                .add_param(ParameterSpec::of("self", TypeName::primitive("")))
                .returns(vec![TypeName::optional(TypeName::primitive(header_type(
                    header.value_kind,
                )))])
                .body(header_accessor_body(header, headers_optional))
                .build()
                .expect("Python response header accessor builds")
        })
        .collect()
}

fn header_accessor_body(header: &ResponseHeaderPlan, headers_optional: bool) -> CodeBlock {
    let wire_name = header.wire_name.as_str();
    let integer_pattern = r"[+-]?[0-9]+";
    let number_pattern = r"[+-]?(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][+-]?[0-9]+)?";
    match header.value_kind {
        ResponseHeaderValueKind::String => sigil_quote!(Python {
            $if(headers_optional) {
                if self.headers is None {
                    return None
                }
            }
            value = self.headers.get($S(wire_name))
            return value
        }),
        ResponseHeaderValueKind::Integer => sigil_quote!(Python {
            $if(headers_optional) {
                if self.headers is None {
                    return None
                }
            }
            value = self.headers.get($S(wire_name))
            if value is None {
                return None
            }
            if re.fullmatch($S(integer_pattern), value) is None {
                return None
            }
            try {
                return int(value)
            } except ValueError {
                return None
            }
        }),
        ResponseHeaderValueKind::Number => sigil_quote!(Python {
            $if(headers_optional) {
                if self.headers is None {
                    return None
                }
            }
            value = self.headers.get($S(wire_name))
            if value is None {
                return None
            }
            if re.fullmatch($S(number_pattern), value) is None {
                return None
            }
            try {
                parsed = float(value)
                return parsed if math.isfinite(parsed) else None
            } except ValueError {
                return None
            }
        }),
        ResponseHeaderValueKind::Boolean => sigil_quote!(Python {
            $if(headers_optional) {
                if self.headers is None {
                    return None
                }
            }
            value = self.headers.get($S(wire_name))
            if value is None {
                return None
            }
            if value == $S("true") {
                return True
            }
            if value == $S("false") {
                return False
            }
            return None
        }),
    }
    .expect("Python response header accessor body builds")
}

fn header_type(kind: ResponseHeaderValueKind) -> &'static str {
    match kind {
        ResponseHeaderValueKind::String => "str",
        ResponseHeaderValueKind::Integer => "int",
        ResponseHeaderValueKind::Number => "float",
        ResponseHeaderValueKind::Boolean => "bool",
    }
}
