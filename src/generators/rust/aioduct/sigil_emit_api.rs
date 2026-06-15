//! Aioduct-specific API method body emission (async, generic runtime).

use sigil_stitch::code_block::CodeBlock;
use sigil_stitch::prelude::sigil_quote;

use crate::generators::rust::common::emit_api::{
    BodyEncoding, MultipartPart, MultipartValueEncoding, OpPlan, RustBackendConfig,
    binary_field_expr, emit_response_match, emit_result_init, optional_binary_field_expr,
    optional_text_field_expr, render_to_string, response_value_expr, rust_field_name,
    rust_string_literal, text_field_expr,
};

/// Backend configuration for aioduct (async, with generic runtime parameter).
pub fn aioduct_backend_config() -> RustBackendConfig {
    RustBackendConfig {
        is_async: true,
        struct_generics: Some("R: aioduct::Runtime".to_string()),
        client_type_args: Some("<R>".to_string()),
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
            if p.is_optional {
                let value_expr = render_to_string("v", &p.param.type_expr, false);
                b.begin_control_flow(&format!("if let Some(v) = &{}", p.var_name), ());
                b.add(
                    &format!("query_parts.push((\"{}\", {value_expr}));\n", p.param.name),
                    (),
                );
                b.end_control_flow();
            } else {
                let value_expr = render_to_string(&p.var_name, &p.param.type_expr, false);
                b.add(
                    &format!("query_parts.push((\"{}\", {value_expr}));\n", p.param.name),
                    (),
                );
            }
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
    let req_let = if needs_mut_req {
        "let mut req"
    } else {
        "let req"
    };
    b.add(&format!("{req_let} = self.client.{method}(&path)?;\n"), ());

    // Headers
    for p in header_params {
        if p.is_optional {
            let value_expr = render_to_string("v", &p.param.type_expr, false);
            b.begin_control_flow(&format!("if let Some(v) = &{}", p.var_name), ());
            b.add(
                &format!(
                    "req = req.header_str(\"{}\", &{value_expr})?;\n",
                    p.param.name
                ),
                (),
            );
            b.end_control_flow();
        } else {
            let value_expr = render_to_string(&p.var_name, &p.param.type_expr, false);
            b.add(
                &format!(
                    "req = req.header_str(\"{}\", &{value_expr})?;\n",
                    p.param.name
                ),
                (),
            );
        }
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
    b.add("let resp = req.send().await?;\n", ());
    b.add("let status_code = resp.status().as_u16();\n", ());

    // Parse response
    if typed_responses.is_empty() {
        b.add(&format!("Ok({response_type} {{ status_code }})\n"), ());
    } else {
        b.add("let body_bytes = resp.bytes().await?;\n", ());
        emit_result_init(&mut b, response_type, typed_responses);
        emit_response_match(&mut b, typed_responses, &|tr| {
            response_value_expr(tr, "&body_bytes")
        });
        b.add("Ok(result)\n", ());
    }

    b.build().unwrap()
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
    b.add(
        "let mut multipart = aioduct::multipart::Multipart::new();\n",
        (),
    );
    for part in parts {
        let wire_name = rust_string_literal(&part.wire_name);
        let content_type = rust_string_literal(&part.content_type);
        if part.value_encoding == MultipartValueEncoding::Unsupported {
            if part.required {
                b.add("return Err(Error::Unsupported(\"unsupported multipart part content type\"));\n", ());
            } else {
                let field_name = rust_field_name(&part.wire_name);
                b.begin_control_flow(&format!("if {body_var}.{field_name}.is_some()"), ());
                b.add("return Err(Error::Unsupported(\"unsupported multipart part content type\"));\n", ());
                b.end_control_flow();
            }
            continue;
        }
        if part.required {
            if part.is_binary {
                b.add(
                    &format!(
                        "multipart = multipart.file({wire_name}, {wire_name}, {content_type}, {});\n",
                        binary_field_expr(body_var, part),
                    ),
                    (),
                );
            } else {
                b.add(
                    &format!(
                        "multipart = multipart.part(aioduct::multipart::Part::text({wire_name}, {}).mime_str({content_type}));\n",
                        text_field_expr(body_var, part),
                    ),
                    (),
                );
            }
        } else {
            let field_name = rust_field_name(&part.wire_name);
            b.begin_control_flow(
                &format!("if let Some(value) = &{body_var}.{field_name}"),
                (),
            );
            if part.is_binary {
                b.add(
                    &format!(
                        "multipart = multipart.file({wire_name}, {wire_name}, {content_type}, {});\n",
                        optional_binary_field_expr("value"),
                    ),
                    (),
                );
            } else {
                b.add(
                    &format!(
                        "multipart = multipart.part(aioduct::multipart::Part::text({wire_name}, {}).mime_str({content_type}));\n",
                        optional_text_field_expr("value", part),
                    ),
                    (),
                );
            }
            b.end_control_flow();
        }
    }
    b.add("req = req.multipart(multipart);\n", ());
}
