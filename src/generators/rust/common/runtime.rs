//! Shared structured runtime emission for Rust generators.

use crate::generators::type_application;
use sigil_stitch::code_block::CodeBlock;
use sigil_stitch::error::SigilStitchError;
use sigil_stitch::prelude::sigil_quote;
use sigil_stitch::spec::annotation_spec::AnnotationSpec;
use sigil_stitch::spec::enum_variant_spec::EnumVariantSpec;
use sigil_stitch::spec::field_spec::FieldSpec;
use sigil_stitch::spec::file_spec::FileSpec;
use sigil_stitch::spec::fun_spec::FunSpec;
use sigil_stitch::spec::modifiers::{TypeKind, Visibility};
use sigil_stitch::spec::parameter_spec::ParameterSpec;
use sigil_stitch::spec::type_spec::TypeSpec;
use sigil_stitch::spec::where_spec::GenericParamSpec;
use sigil_stitch::type_name::TypeName;

pub(crate) fn render_api_call_error(
    response_headers_type: TypeName,
) -> Result<String, SigilStitchError> {
    FileSpec::builder("error.rs")
        .add_type(api_call_http_error(response_headers_type.clone())?)
        .add_type(api_call_error_kind()?)
        .add_type(api_call_error_inner()?)
        .add_type(api_call_error(response_headers_type)?)
        .add_code(api_call_error_display()?)
        .add_code(api_call_error_source()?)
        .build()?
        .render(100)
}

fn api_call_http_error(response_headers_type: TypeName) -> Result<TypeSpec, SigilStitchError> {
    let api_error_of_t = type_application(
        TypeName::primitive("ApiError"),
        vec![TypeName::primitive("T")],
    );
    let from_api_error = FunSpec::builder("from_api_error")
        .add_generic_param(GenericParamSpec::single("T")?)
        .add_param(ParameterSpec::of("error", api_error_of_t))
        .returns(vec![TypeName::primitive("Self")])
        .body(sigil_quote!(RustLang {
            Self {
                status_code: error.status_code,
                headers: error.headers,
                raw_body: error.raw_body,
                body_error: error.body.err(),
            }
        })?)
        .build()?;

    TypeSpec::builder("ApiCallHttpError", TypeKind::Struct)
        .annotate(AnnotationSpec::new("derive").args(["Debug"]))
        .add_field(FieldSpec::builder("status_code", TypeName::primitive("u16")).build()?)
        .add_field(FieldSpec::builder("headers", response_headers_type).build()?)
        .add_field(
            FieldSpec::builder(
                "raw_body",
                type_application(TypeName::primitive("Vec"), vec![TypeName::primitive("u8")]),
            )
            .build()?,
        )
        .add_field(
            FieldSpec::builder(
                "body_error",
                TypeName::optional(TypeName::primitive("Error")),
            )
            .build()?,
        )
        .add_method(from_api_error)
        .build()
}

fn api_call_error_kind() -> Result<TypeSpec, SigilStitchError> {
    let http = EnumVariantSpec::builder("Http")
        .positional_payload(TypeName::primitive("ApiCallHttpError"))
        .build()?;
    let runtime = EnumVariantSpec::builder("Runtime")
        .positional_payload(TypeName::primitive("Error"))
        .build()?;

    TypeSpec::builder("ApiCallErrorKind", TypeKind::Enum)
        .annotate(AnnotationSpec::new("derive").args(["Debug"]))
        .add_variant(http)
        .add_variant(runtime)
        .build()
}

fn api_call_error_inner() -> Result<TypeSpec, SigilStitchError> {
    let api_error_of_t = type_application(
        TypeName::primitive("ApiError"),
        vec![TypeName::primitive("T")],
    );
    let from_api_error = FunSpec::builder("from_api_error")
        .add_generic_param(GenericParamSpec::single("T")?)
        .add_param(ParameterSpec::of("operation_id", operation_id_type()))
        .add_param(ParameterSpec::of("error", api_error_of_t))
        .returns(vec![TypeName::primitive("Self")])
        .body(
            sigil_quote!(RustLang {
                Self {
                    operation_id,
                    kind: ApiCallErrorKind::Http(ApiCallHttpError::from_api_error(error)),
                }
            })
            .map_err(|error| SigilStitchError::Render {
                context: "ApiCallErrorInner::from_api_error body".into(),
                message: error.to_string(),
            })?,
        )
        .build()?;
    let from_runtime_error = FunSpec::builder("from_runtime_error")
        .add_param(ParameterSpec::of("operation_id", operation_id_type()))
        .add_param(ParameterSpec::of("error", TypeName::primitive("Error")))
        .returns(vec![TypeName::primitive("Self")])
        .body(
            sigil_quote!(RustLang {
                Self {
                    operation_id,
                    kind: ApiCallErrorKind::Runtime(error),
                }
            })
            .map_err(|error| SigilStitchError::Render {
                context: "ApiCallErrorInner::from_runtime_error body".into(),
                message: error.to_string(),
            })?,
        )
        .build()?;

    TypeSpec::builder("ApiCallErrorInner", TypeKind::Struct)
        .annotate(AnnotationSpec::new("derive").args(["Debug"]))
        .add_field(FieldSpec::builder("operation_id", operation_id_type()).build()?)
        .add_field(FieldSpec::builder("kind", TypeName::primitive("ApiCallErrorKind")).build()?)
        .add_method(from_api_error)
        .add_method(from_runtime_error)
        .build()
}

fn api_call_error(response_headers_type: TypeName) -> Result<TypeSpec, SigilStitchError> {
    let static_str = operation_id_type();
    let api_error_of_t = type_application(
        TypeName::primitive("ApiError"),
        vec![TypeName::primitive("T")],
    );
    let optional_status = TypeName::optional(TypeName::primitive("u16"));
    let optional_headers = api_call_headers_type(response_headers_type.clone());
    let optional_body = api_call_raw_body_type();

    let from_api_error = FunSpec::builder("from_api_error")
        .visibility(Visibility::PublicCrate)
        .add_generic_param(GenericParamSpec::single("T")?)
        .add_param(ParameterSpec::of("operation_id", static_str.clone()))
        .add_param(ParameterSpec::of("error", api_error_of_t))
        .returns(vec![TypeName::primitive("Self")])
        .body(
            sigil_quote!(RustLang {
                Self {
                    inner: Box::new(ApiCallErrorInner::from_api_error(operation_id, error)),
                }
            })
            .map_err(|error| SigilStitchError::Render {
                context: "ApiCallError::from_api_error body".into(),
                message: error.to_string(),
            })?,
        )
        .build()?;
    let from_runtime_error = FunSpec::builder("from_runtime_error")
        .visibility(Visibility::PublicCrate)
        .add_param(ParameterSpec::of("operation_id", static_str.clone()))
        .add_param(ParameterSpec::of("error", TypeName::primitive("Error")))
        .returns(vec![TypeName::primitive("Self")])
        .body(
            sigil_quote!(RustLang {
                Self {
                    inner: Box::new(ApiCallErrorInner::from_runtime_error(operation_id, error)),
                }
            })
            .map_err(|error| SigilStitchError::Render {
                context: "ApiCallError::from_runtime_error body".into(),
                message: error.to_string(),
            })?,
        )
        .build()?;
    let operation_id = FunSpec::builder("operation_id")
        .visibility(Visibility::Public)
        .doc(
            "Operation identifier, generated from the HTTP method and path when OpenAPI omits one.",
        )
        .add_param(ParameterSpec::of("&self", TypeName::primitive("")))
        .returns(vec![static_str])
        .body(sigil_quote!(RustLang { self.inner.operation_id })?)
        .build()?;
    let status_code = FunSpec::builder("status_code")
        .visibility(Visibility::Public)
        .doc("HTTP response status, if the operation reached the server and received an error response.")
        .add_param(ParameterSpec::of("&self", TypeName::primitive("")))
        .returns(vec![optional_status])
        .body(
            sigil_quote!(RustLang {
                match &self.inner.kind {
                    ApiCallErrorKind::Http(error) => Some(error.status_code),
                    ApiCallErrorKind::Runtime(_) => None,
                }
            })?,
        )
        .build()?;
    let headers = FunSpec::builder("headers")
        .visibility(Visibility::Public)
        .doc("Native HTTP response headers, if an error response was received.")
        .add_param(ParameterSpec::of("&self", TypeName::primitive("")))
        .returns(vec![optional_headers])
        .body(sigil_quote!(RustLang {
            match &self.inner.kind {
                ApiCallErrorKind::Http(error) => Some(&error.headers),
                ApiCallErrorKind::Runtime(_) => None,
            }
        })?)
        .build()?;
    let raw_body = FunSpec::builder("raw_body")
        .visibility(Visibility::Public)
        .doc("Raw HTTP response body, if an error response was received.")
        .add_param(ParameterSpec::of("&self", TypeName::primitive("")))
        .returns(vec![optional_body])
        .body(sigil_quote!(RustLang {
            match &self.inner.kind {
                ApiCallErrorKind::Http(error) => Some(&error.raw_body),
                ApiCallErrorKind::Runtime(_) => None,
            }
        })?)
        .build()?;

    TypeSpec::builder("ApiCallError", TypeKind::Struct)
        .visibility(Visibility::Public)
        .doc("Type-erased error from any generated API operation.")
        .annotate(AnnotationSpec::new("derive").args(["Debug"]))
        .add_field(FieldSpec::builder("inner", api_call_error_inner_type()).build()?)
        .add_method(from_api_error)
        .add_method(from_runtime_error)
        .add_method(operation_id)
        .add_method(status_code)
        .add_method(headers)
        .add_method(raw_body)
        .build()
}

fn operation_id_type() -> TypeName {
    TypeName::reference_with_lifetime(TypeName::primitive("str"), "'static")
}

fn api_call_error_inner_type() -> TypeName {
    type_application(
        TypeName::primitive("Box"),
        vec![TypeName::primitive("ApiCallErrorInner")],
    )
}

fn api_call_headers_type(response_headers_type: TypeName) -> TypeName {
    TypeName::optional(TypeName::reference(response_headers_type))
}

fn api_call_raw_body_type() -> TypeName {
    TypeName::optional(TypeName::slice(TypeName::primitive("u8")))
}

fn api_call_error_display() -> Result<CodeBlock, SigilStitchError> {
    sigil_quote!(RustLang {
        impl std::fmt::Display for ApiCallError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match &self.inner.kind {
                    ApiCallErrorKind::Http(error) => {
                        write!(f, "operation {} failed with HTTP status {}", self.inner.operation_id, error.status_code)
                    }
                    ApiCallErrorKind::Runtime(error) => {
                        write!(f, "operation {} failed: {error}", self.inner.operation_id)
                    }
                }
            }
        }
    })
}

fn api_call_error_source() -> Result<CodeBlock, SigilStitchError> {
    sigil_quote!(RustLang {
        impl std::error::Error for ApiCallError {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                if let ApiCallErrorKind::Http(error) = &self.inner.kind {
                    if let Some(error) = &error.body_error {
                        return Some(error);
                    }
                }
                if let ApiCallErrorKind::Runtime(error) = &self.inner.kind {
                    return Some(error);
                }
                None
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use sigil_stitch::assert_rendered;
    use sigil_stitch::lang::rust::Rust;

    use super::*;

    #[test]
    fn api_call_error_borrowed_types_render_exactly() {
        let operation_id = operation_id_type();
        let accessors = TypeName::tuple(vec![
            api_call_headers_type(TypeName::qualified("reqwest::header", "HeaderMap")),
            api_call_raw_body_type(),
        ]);
        let block = sigil_quote!(RustLang {
            fn inspect(operation_id: $T(operation_id)) -> $T(accessors) {
                todo!()
            }
        })
        .expect("ApiCallError borrowed type probe builds");

        assert_rendered!(
            Rust::new(),
            width = 100,
            block,
            r#"fn inspect(operation_id: &'static str) -> (Option<&reqwest::header::HeaderMap>, Option<&[u8]>) {
    todo!()
}
"#,
        );
    }

    #[test]
    fn api_call_error_owned_type_renders_exactly() {
        let inner = api_call_error_inner_type();
        let block = sigil_quote!(RustLang {
            struct ApiCallError {
                inner: $T(inner),
            }
        })
        .expect("ApiCallError owned type probe builds");

        assert_rendered!(
            Rust::new(),
            width = 100,
            block,
            r#"struct ApiCallError {
    inner: Box<ApiCallErrorInner>,
}
"#,
        );
    }
}
