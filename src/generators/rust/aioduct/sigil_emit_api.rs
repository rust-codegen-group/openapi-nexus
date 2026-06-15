//! Aioduct-specific API method body emission (async, generic runtime).

use sigil_stitch::code_block::CodeBlock;
use sigil_stitch::prelude::sigil_quote;

use crate::generators::rust::common::emit_api::{
    BodyEncoding, MultipartPart, MultipartValueEncoding, OpPlan, RustBackendConfig,
    binary_field_expr, binary_filename_expr, emit_response_match, emit_result_init,
    optional_binary_field_expr, optional_binary_filename_expr, optional_text_field_expr,
    render_to_string, response_value_expr, rust_field_name, rust_string_literal, text_field_expr,
};

/// Backend configuration for aioduct (async, with generic runtime parameter).
pub fn aioduct_backend_config() -> RustBackendConfig {
    RustBackendConfig {
        is_async: true,
        struct_generics: Some("R: aioduct::RuntimePoll, C: aioduct::ConnectorSend".to_string()),
        client_type_args: Some("<R, C>".to_string()),
    }
}

/// Emit the aioduct-specific method body for an operation.
pub fn emit_method_body(plan: &OpPlan<'_>) -> CodeBlock {
    let OpPlan {
        op,
        response_type,
        path_params,
        query_params,
        header_params,
        body,
        typed_responses,
        ..
    } = plan;

    let mut b = CodeBlock::builder();

    if let Some(body) = body
        && matches!(&body.encoding, BodyEncoding::Multipart)
        && !body.multipart_supported
    {
        if body.required {
            b.add_code(unsupported_multipart_body(&body.var_name));
            return b.build().unwrap();
        }
        b.begin_control_flow(&format!("if {}.is_some()", body.var_name), ());
        b.add_code(unsupported_multipart_body(&body.var_name));
        b.end_control_flow();
    }

    // Build path
    let needs_mut_path = !path_params.is_empty() || !query_params.is_empty();
    if needs_mut_path {
        b.add(
            &format!("let mut path = \"{}\".to_string();\n", op.path),
            (),
        );
    } else {
        b.add(&format!("let path = \"{}\".to_string();\n", op.path), ());
    }
    for p in path_params {
        let placeholder = format!("{{{}}}", p.param.name);
        let value_expr = render_to_string(&p.var_name, &p.param.type_expr, p.is_optional);
        b.add(
            &format!("path = path.replace(\"{placeholder}\", &{value_expr});\n"),
            (),
        );
    }

    // Build query string
    if !query_params.is_empty() {
        b.add(
            "let mut query_parts: Vec<(&str, String)> = Vec::new();\n",
            (),
        );
        for p in query_params {
            let value_expr = if p.is_optional {
                render_to_string("v", &p.param.type_expr, false)
            } else {
                render_to_string(&p.var_name, &p.param.type_expr, false)
            };
            b.add_code(rust_query_part(
                p.is_optional,
                &p.var_name,
                &p.param.name,
                &value_expr,
            ));
        }
        b.begin_control_flow("if !query_parts.is_empty()", ());
        b.add(
            "let qs = url::form_urlencoded::Serializer::new(String::new()).extend_pairs(query_parts.iter().map(|(k, v)| (*k, v.as_str()))).finish();\n",
            (),
        );
        b.add("path = format!(\"{}?{}\", path, qs);\n", ());
        b.end_control_flow();
    }

    // Build request (aioduct uses method helpers that return Result)
    let method = op.method.to_lowercase();
    let needs_mut_req = !header_params.is_empty() || body.is_some();
    b.add_code(aioduct_request(needs_mut_req, &method));

    // Headers
    for p in header_params {
        let header_name = rust_string_literal(&p.param.name);
        let value_expr = if p.is_optional {
            render_to_string("v", &p.param.type_expr, false)
        } else {
            render_to_string(&p.var_name, &p.param.type_expr, false)
        };
        b.add_code(aioduct_header_guarded(
            p.is_optional,
            &p.var_name,
            &header_name,
            &value_expr,
        ));
    }

    // Body
    if let Some(body) = body {
        let can_emit_body =
            !matches!(&body.encoding, BodyEncoding::Multipart) || body.multipart_supported;
        if can_emit_body {
            if !body.required {
                b.begin_control_flow(
                    &format!("if let Some({}) = {}", body.var_name, body.var_name),
                    (),
                );
            }
            emit_body(&mut b, body);
            if !body.required {
                b.end_control_flow();
            }
        }
    }

    // Send
    b.add_code(aioduct_send());
    b.add_code(status_code_init());

    // Parse response
    if typed_responses.is_empty() {
        b.add_code(empty_response(response_type));
    } else {
        b.add_code(aioduct_body_bytes_init());
        emit_result_init(&mut b, response_type, typed_responses);
        emit_response_match(&mut b, typed_responses, &|tr| {
            response_value_expr(tr, "&body_bytes")
        });
        b.add_code(ok_result());
    }

    b.build().unwrap()
}

fn aioduct_request(needs_mut_req: bool, method: &str) -> CodeBlock {
    let mutable_request_stmt = format!("let mut req = self.client.{method}(&path)?;");
    let request_stmt = format!("let req = self.client.{method}(&path)?;");
    sigil_quote!(RustLang {
        $if(needs_mut_req) {
            $L(mutable_request_stmt.as_str())
        } $else {
            $L(request_stmt.as_str())
        }
    })
    .expect("aioduct request builds")
}

fn rust_query_part(
    is_optional: bool,
    var_name: &str,
    param_name: &str,
    value_expr: &str,
) -> CodeBlock {
    let param_name = rust_string_literal(param_name);
    sigil_quote!(RustLang {
        $if(is_optional) {
            if let Some(v) = &$L(var_name) {
                query_parts.push(($L(param_name.as_str()), $L(value_expr)));
            }
        } $else {
            query_parts.push(($L(param_name.as_str()), $L(value_expr)));
        }
    })
    .expect("Rust query part builds")
}

fn aioduct_header_guarded(
    is_optional: bool,
    var_name: &str,
    header_name: &str,
    value_expr: &str,
) -> CodeBlock {
    sigil_quote!(RustLang {
        $if(is_optional) {
            if let Some(v) = &$L(var_name) {
                req = req.header_str($L(header_name), &$L(value_expr))?;
            }
        } $else {
            req = req.header_str($L(header_name), &$L(value_expr))?;
        }
    })
    .expect("guarded aioduct header builds")
}

fn aioduct_send() -> CodeBlock {
    let send_stmt = "let resp = req.send().await?;";
    sigil_quote!(RustLang {
        $L(send_stmt)
    })
    .expect("aioduct send builds")
}

fn status_code_init() -> CodeBlock {
    sigil_quote!(RustLang {
        let status_code = resp.status().as_u16();
    })
    .expect("status code init builds")
}

fn empty_response(response_type: &str) -> CodeBlock {
    let response_expr = format!("{response_type} {{ status_code }}");
    sigil_quote!(RustLang {
        Ok($L(response_expr.as_str()))
    })
    .expect("empty response builds")
}

fn aioduct_body_bytes_init() -> CodeBlock {
    let body_bytes_stmt = "let body_bytes = resp.bytes().await?;";
    sigil_quote!(RustLang {
        $L(body_bytes_stmt)
    })
    .expect("aioduct body bytes init builds")
}

fn ok_result() -> CodeBlock {
    sigil_quote!(RustLang {
        Ok(result)
    })
    .expect("ok result builds")
}

fn emit_body(
    b: &mut sigil_stitch::code_block::CodeBlockBuilder,
    body: &crate::generators::rust::common::emit_api::BodyBinding,
) {
    match &body.encoding {
        BodyEncoding::Json => {
            b.add_code(json_body(&body.var_name));
        }
        BodyEncoding::FormUrlEncoded => {
            b.add_code(form_body(&body.var_name));
        }
        BodyEncoding::Xml => {
            b.add_code(xml_body(&body.var_name, &body.media_type));
        }
        BodyEncoding::TextPlain => {
            b.add_code(text_body(&body.var_name, &body.media_type));
        }
        BodyEncoding::OctetStream => {
            b.add_code(octet_stream_body(&body.var_name, &body.media_type));
        }
        BodyEncoding::Multipart => {
            emit_multipart_body(b, &body.var_name, &body.multipart_parts);
        }
        BodyEncoding::Other(media_type) => {
            b.add_code(unsupported_media_type_body(media_type));
        }
    }
}

fn unsupported_multipart_body(body_var: &str) -> CodeBlock {
    sigil_quote!(RustLang {
        let _ = self.client;
        let _ = $L(body_var);
        return Err(Error::Unsupported($S("multipart/form-data request bodies must be object schemas")));
    })
    .expect("unsupported multipart body builds")
}

fn unsupported_media_type_body(media_type: &str) -> CodeBlock {
    let message = format!("unsupported request body media type: {media_type}");
    sigil_quote!(RustLang {
        return Err(Error::Unsupported($S(&message)));
    })
    .expect("unsupported media type body builds")
}

fn json_body(body_var: &str) -> CodeBlock {
    sigil_quote!(RustLang {
        req = req.json(&$L(body_var))?;
    })
    .expect("json body builds")
}

fn form_body(body_var: &str) -> CodeBlock {
    sigil_quote!(RustLang {
        req = req.form_serde(&$L(body_var))?;
    })
    .expect("form body builds")
}

fn xml_body(body_var: &str, media_type: &str) -> CodeBlock {
    sigil_quote!(RustLang {
        req = req.header_str("Content-Type", $L(rust_string_literal(media_type)))?;
        let body_xml = serde_xml_rs::to_string($L(body_var))?;
        req = req.body(body_xml.into_bytes());
    })
    .expect("xml body builds")
}

fn text_body(body_var: &str, media_type: &str) -> CodeBlock {
    sigil_quote!(RustLang {
        req = req.header_str("Content-Type", $L(rust_string_literal(media_type)))?;
        req = req.body($L(body_var).clone().into_bytes());
    })
    .expect("text body builds")
}

fn octet_stream_body(body_var: &str, media_type: &str) -> CodeBlock {
    sigil_quote!(RustLang {
        req = req.header_str("Content-Type", $L(rust_string_literal(media_type)))?;
        req = req.body($L(body_var).clone());
    })
    .expect("octet-stream body builds")
}

fn emit_multipart_body(
    b: &mut sigil_stitch::code_block::CodeBlockBuilder,
    body_var: &str,
    parts: &[MultipartPart],
) {
    b.add_code(multipart_init());
    for part in parts {
        let wire_name = rust_string_literal(&part.wire_name);
        let content_type = rust_string_literal(&part.content_type);
        if part.value_encoding == MultipartValueEncoding::Unsupported {
            if part.required {
                b.add_code(unsupported_multipart_part());
            } else {
                let field_name = rust_field_name(&part.wire_name);
                b.begin_control_flow(&format!("if {body_var}.{field_name}.is_some()"), ());
                b.add_code(unsupported_multipart_part());
                b.end_control_flow();
            }
            continue;
        }
        if part.required {
            let binary_value_expr = if part.is_binary {
                binary_field_expr(body_var, part)
            } else {
                String::new()
            };
            let filename_expr = if part.is_binary {
                binary_filename_expr(body_var, part)
            } else {
                String::new()
            };
            let text_value_expr = if part.is_binary {
                String::new()
            } else {
                text_field_expr(body_var, part)
            };
            b.add_code(aioduct_multipart_part(
                part.is_binary,
                &wire_name,
                &filename_expr,
                &content_type,
                &binary_value_expr,
                &text_value_expr,
            ));
        } else {
            let field_name = rust_field_name(&part.wire_name);
            b.begin_control_flow(
                &format!("if let Some(value) = &{body_var}.{field_name}"),
                (),
            );
            let binary_value_expr = if part.is_binary {
                optional_binary_field_expr("value")
            } else {
                String::new()
            };
            let filename_expr = if part.is_binary {
                optional_binary_filename_expr("value", part)
            } else {
                String::new()
            };
            let text_value_expr = if part.is_binary {
                String::new()
            } else {
                optional_text_field_expr("value", part)
            };
            b.add_code(aioduct_multipart_part(
                part.is_binary,
                &wire_name,
                &filename_expr,
                &content_type,
                &binary_value_expr,
                &text_value_expr,
            ));
            b.end_control_flow();
        }
    }
    b.add_code(multipart_finish());
}

fn multipart_init() -> CodeBlock {
    sigil_quote!(RustLang {
        let mut multipart = aioduct::multipart::Multipart::new();
    })
    .expect("multipart init builds")
}

fn unsupported_multipart_part() -> CodeBlock {
    sigil_quote!(RustLang {
        return Err(Error::Unsupported($S("unsupported multipart part content type")));
    })
    .expect("unsupported multipart part builds")
}

fn aioduct_multipart_part(
    is_binary: bool,
    wire_name: &str,
    filename_expr: &str,
    content_type: &str,
    binary_value_expr: &str,
    text_value_expr: &str,
) -> CodeBlock {
    sigil_quote!(RustLang {
        $if(is_binary) {
            multipart = multipart.file($L(wire_name), $L(filename_expr), $L(content_type), $L(binary_value_expr));
        } $else {
            multipart = multipart.part(aioduct::multipart::Part::text($L(wire_name), $L(text_value_expr)).mime_str($L(content_type)));
        }
    })
    .expect("multipart part builds")
}

fn multipart_finish() -> CodeBlock {
    sigil_quote!(RustLang {
        req = req.multipart(multipart);
    })
    .expect("multipart finish builds")
}
