//! Ureq-specific API method body emission (synchronous, ureq 3.x).

use sigil_stitch::code_block::CodeBlock;
use sigil_stitch::prelude::sigil_quote;

use crate::generators::rust::common::emit_api::{
    BodyEncoding, MultipartPart, MultipartValueEncoding, OpPlan, RustBackendConfig,
    binary_field_expr, binary_filename_expr, emit_response_match, emit_result_init,
    optional_binary_field_expr, optional_binary_filename_expr, optional_text_field_expr,
    render_to_string, response_value_expr, rust_field_name, rust_string_literal, text_field_expr,
};
use crate::ir::types::IrTypeExpr;

/// Backend configuration for ureq (synchronous, no extra generics).
pub fn ureq_backend_config() -> RustBackendConfig {
    RustBackendConfig {
        is_async: false,
        struct_generics: None,
        client_type_args: None,
    }
}

/// Emit the ureq-specific method body for an operation.
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

    let method = op.method.to_lowercase();
    let is_body_method = matches!(method.as_str(), "post" | "put" | "patch");

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
    if path_params.is_empty() {
        b.add(&format!("let path = \"{}\".to_string();\n", op.path), ());
    } else {
        b.add(
            &format!("let mut path = \"{}\".to_string();\n", op.path),
            (),
        );
        for p in path_params {
            let placeholder = format!("{{{}}}", p.param.name);
            let value_expr = render_to_string(&p.var_name, &p.param.type_expr, p.is_optional);
            b.add(
                &format!("path = path.replace(\"{placeholder}\", &{value_expr});\n"),
                (),
            );
        }
    }

    // Create request via typed client method
    let body_requires_mut_req = body.as_ref().is_some_and(|body| {
        matches!(
            &body.encoding,
            BodyEncoding::Xml
                | BodyEncoding::TextPlain
                | BodyEncoding::OctetStream
                | BodyEncoding::Multipart
        )
    });
    let needs_mut_req =
        !query_params.is_empty() || !header_params.is_empty() || body_requires_mut_req;
    b.add_code(ureq_request(needs_mut_req, &method));

    // Query params via ureq's built-in .query()
    for p in query_params {
        let is_array = matches!(&p.param.type_expr, IrTypeExpr::Array(_));
        if p.is_optional {
            if is_array {
                b.begin_control_flow(&format!("if let Some(vals) = &{}", p.var_name), ());
                b.begin_control_flow("for v in vals", ());
                b.add(
                    &format!("req = req.query(\"{}\", &v.to_string());\n", p.param.name),
                    (),
                );
                b.end_control_flow();
                b.end_control_flow();
            } else {
                let value_expr = render_to_string("v", &p.param.type_expr, false);
                b.begin_control_flow(&format!("if let Some(v) = &{}", p.var_name), ());
                b.add(
                    &format!("req = req.query(\"{}\", &{value_expr});\n", p.param.name),
                    (),
                );
                b.end_control_flow();
            }
        } else if is_array {
            b.begin_control_flow(&format!("for v in {}", p.var_name), ());
            b.add(
                &format!("req = req.query(\"{}\", &v.to_string());\n", p.param.name),
                (),
            );
            b.end_control_flow();
        } else {
            let value_expr = render_to_string(&p.var_name, &p.param.type_expr, false);
            b.add(
                &format!("req = req.query(\"{}\", &{value_expr});\n", p.param.name),
                (),
            );
        }
    }

    // Headers
    for p in header_params {
        if p.is_optional {
            let value_expr = render_to_string("v", &p.param.type_expr, false);
            b.begin_control_flow(&format!("if let Some(v) = &{}", p.var_name), ());
            b.add(
                &format!("req = req.header(\"{}\", &{value_expr});\n", p.param.name),
                (),
            );
            b.end_control_flow();
        } else {
            let value_expr = render_to_string(&p.var_name, &p.param.type_expr, false);
            b.add(
                &format!("req = req.header(\"{}\", &{value_expr});\n", p.param.name),
                (),
            );
        }
    }

    // Send
    if is_body_method {
        if let Some(body) = body {
            let can_emit_body =
                !matches!(&body.encoding, BodyEncoding::Multipart) || body.multipart_supported;
            if can_emit_body {
                if body.required {
                    emit_body_send(&mut b, body, "let resp =");
                } else {
                    b.add_code(resp_decl());
                    b.begin_control_flow(
                        &format!("if let Some({}) = {}", body.var_name, body.var_name),
                        (),
                    );
                    emit_body_send(&mut b, body, "resp =");
                    b.end_control_flow();
                    b.begin_control_flow("else", ());
                    b.add_code(assign_send_empty());
                    b.end_control_flow();
                }
            } else {
                b.add_code(let_send_empty());
            }
        } else {
            b.add_code(let_send_empty());
        }
    } else {
        b.add_code(let_call());
    }
    b.add_code(status_code_init());

    // Parse response
    if typed_responses.is_empty() {
        b.add_code(empty_response(response_type));
    } else {
        b.add_code(body_bytes_init());
        emit_result_init(&mut b, response_type, typed_responses);
        emit_response_match(&mut b, typed_responses, &|tr| {
            response_value_expr(tr, "body_bytes.as_slice()")
        });
        b.add_code(ok_result());
    }

    b.build().unwrap()
}

fn emit_body_send(
    b: &mut sigil_stitch::code_block::CodeBlockBuilder,
    body: &crate::generators::rust::common::emit_api::BodyBinding,
    resp_prefix: &str,
) {
    match &body.encoding {
        BodyEncoding::Json => {
            b.add_code(json_send(&body.var_name, resp_prefix));
        }
        BodyEncoding::FormUrlEncoded => {
            emit_form_body(b, &body.var_name);
            b.add_code(form_send(resp_prefix));
        }
        BodyEncoding::Xml => {
            b.add_code(xml_send(&body.var_name, &body.media_type, resp_prefix));
        }
        BodyEncoding::TextPlain => {
            b.add_code(text_send(&body.var_name, &body.media_type, resp_prefix));
        }
        BodyEncoding::OctetStream => {
            b.add_code(octet_stream_send(
                &body.var_name,
                &body.media_type,
                resp_prefix,
            ));
        }
        BodyEncoding::Multipart => {
            emit_multipart_body(b, &body.var_name, &body.multipart_parts);
            b.add_code(multipart_send(resp_prefix));
        }
        BodyEncoding::Other(media_type) => {
            b.add_code(unsupported_media_type_body(media_type));
        }
    }
}

fn ureq_request(needs_mut_req: bool, method: &str) -> CodeBlock {
    let mutable_request_stmt = format!("let mut req = self.client.{method}(&path);");
    let request_stmt = format!("let req = self.client.{method}(&path);");
    sigil_quote!(RustLang {
        $if(needs_mut_req) {
            $L(mutable_request_stmt.as_str())
        } $else {
            $L(request_stmt.as_str())
        }
    })
    .expect("ureq request builds")
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

fn json_send(body_var: &str, resp_prefix: &str) -> CodeBlock {
    sigil_quote!(RustLang {
        $L(resp_prefix) req.send_json(&$L(body_var))?;
    })
    .expect("json send builds")
}

fn form_send(resp_prefix: &str) -> CodeBlock {
    let pairs_iter = "form_pairs.iter().map(|(key, value)| (key.as_str(), value.as_str()))";
    sigil_quote!(RustLang {
        $L(resp_prefix) req.send_form($L(pairs_iter))?;
    })
    .expect("form send builds")
}

fn xml_send(body_var: &str, media_type: &str, resp_prefix: &str) -> CodeBlock {
    sigil_quote!(RustLang {
        let body_xml = serde_xml_rs::to_string($L(body_var))?;
        req = req.header("Content-Type", $L(rust_string_literal(media_type)));
        $L(resp_prefix) req.send(body_xml)?;
    })
    .expect("xml send builds")
}

fn text_send(body_var: &str, media_type: &str, resp_prefix: &str) -> CodeBlock {
    sigil_quote!(RustLang {
        req = req.header("Content-Type", $L(rust_string_literal(media_type)));
        $L(resp_prefix) req.send($L(body_var).as_str())?;
    })
    .expect("text send builds")
}

fn octet_stream_send(body_var: &str, media_type: &str, resp_prefix: &str) -> CodeBlock {
    sigil_quote!(RustLang {
        req = req.header("Content-Type", $L(rust_string_literal(media_type)));
        $L(resp_prefix) req.send($L(body_var).clone())?;
    })
    .expect("octet-stream send builds")
}

fn multipart_send(resp_prefix: &str) -> CodeBlock {
    sigil_quote!(RustLang {
        $L(resp_prefix) req.send(multipart_body)?;
    })
    .expect("multipart send builds")
}

fn emit_multipart_body(
    b: &mut sigil_stitch::code_block::CodeBlockBuilder,
    body_var: &str,
    parts: &[MultipartPart],
) {
    b.add_code(multipart_boundary_init());
    b.add_code(multipart_body_init());
    for part in parts {
        emit_multipart_part(b, body_var, part);
    }
    b.add_code(multipart_epilogue());
}

fn emit_form_body(b: &mut sigil_stitch::code_block::CodeBlockBuilder, body_var: &str) {
    b.add_code(form_pairs_init(body_var));
    b.begin_control_flow("if let serde_json::Value::Object(fields) = form_value", ());
    b.begin_control_flow("for (key, value) in fields", ());
    b.begin_control_flow("if !value.is_null()", ());
    b.add_code(form_value_to_string());
    b.add_code(form_pairs_push());
    b.end_control_flow();
    b.end_control_flow();
    b.end_control_flow();
}

fn resp_decl() -> CodeBlock {
    sigil_quote!(RustLang {
        let resp;
    })
    .expect("response declaration builds")
}

fn assign_send_empty() -> CodeBlock {
    sigil_quote!(RustLang {
        resp = req.send_empty()?;
    })
    .expect("assign send_empty builds")
}

fn let_send_empty() -> CodeBlock {
    sigil_quote!(RustLang {
        let resp = req.send_empty()?;
    })
    .expect("let send_empty builds")
}

fn let_call() -> CodeBlock {
    sigil_quote!(RustLang {
        let resp = req.call()?;
    })
    .expect("let call builds")
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

fn body_bytes_init() -> CodeBlock {
    sigil_quote!(RustLang {
        let body_bytes = resp.into_body().read_to_vec()?;
    })
    .expect("body bytes init builds")
}

fn ok_result() -> CodeBlock {
    sigil_quote!(RustLang {
        Ok(result)
    })
    .expect("ok result builds")
}

fn form_value_to_string() -> CodeBlock {
    let value_expr = "let value = match value { serde_json::Value::String(s) => s, other => other.to_string() };";
    sigil_quote!(RustLang {
        $L(value_expr)
    })
    .expect("form value conversion builds")
}

fn form_pairs_push() -> CodeBlock {
    sigil_quote!(RustLang {
        form_pairs.push((key, value));
    })
    .expect("form pairs push builds")
}

fn multipart_boundary_init() -> CodeBlock {
    let timestamp_expr = "std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|duration| duration.as_nanos()).unwrap_or(0)";
    sigil_quote!(RustLang {
        let boundary = format!("openapi-nexus-{}", $L(timestamp_expr));
    })
    .expect("multipart boundary init builds")
}

fn multipart_body_init() -> CodeBlock {
    sigil_quote!(RustLang {
        let mut multipart_body: Vec<u8> = Vec::new();
    })
    .expect("multipart body init builds")
}

fn multipart_epilogue() -> CodeBlock {
    sigil_quote!(RustLang {
        multipart_body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());
        req = req.header("Content-Type", &format!("multipart/form-data; boundary={}", boundary));
    })
    .expect("multipart epilogue builds")
}

fn form_pairs_init(body_var: &str) -> CodeBlock {
    sigil_quote!(RustLang {
        let form_value = serde_json::to_value($L(body_var))?;
        let mut form_pairs: Vec<(String, String)> = Vec::new();
    })
    .expect("form pairs init builds")
}

fn emit_multipart_part(
    b: &mut sigil_stitch::code_block::CodeBlockBuilder,
    body_var: &str,
    part: &MultipartPart,
) {
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
        return;
    }
    if part.required {
        let filename_expr = part.is_binary.then(|| binary_filename_expr(body_var, part));
        emit_part_prefix(
            b,
            &wire_name,
            part.is_binary,
            &content_type,
            filename_expr.as_deref(),
        );
        let binary_value_expr = if part.is_binary {
            binary_field_expr(body_var, part)
        } else {
            String::new()
        };
        let text_value_expr = if part.is_binary {
            String::new()
        } else {
            text_field_expr(body_var, part)
        };
        b.add_code(multipart_part_value(
            part.is_binary,
            &binary_value_expr,
            &text_value_expr,
        ));
        b.add_code(multipart_part_crlf());
    } else {
        let field_name = rust_field_name(&part.wire_name);
        b.begin_control_flow(
            &format!("if let Some(value) = &{body_var}.{field_name}"),
            (),
        );
        let filename_expr = part
            .is_binary
            .then(|| optional_binary_filename_expr("value", part));
        emit_part_prefix(
            b,
            &wire_name,
            part.is_binary,
            &content_type,
            filename_expr.as_deref(),
        );
        let binary_value_expr = if part.is_binary {
            optional_binary_field_expr("value")
        } else {
            String::new()
        };
        let text_value_expr = if part.is_binary {
            String::new()
        } else {
            optional_text_field_expr("value", part)
        };
        b.add_code(multipart_part_value(
            part.is_binary,
            &binary_value_expr,
            &text_value_expr,
        ));
        b.add_code(multipart_part_crlf());
        b.end_control_flow();
    }
}

fn unsupported_multipart_part() -> CodeBlock {
    sigil_quote!(RustLang {
        return Err(Error::Unsupported($S("unsupported multipart part content type")));
    })
    .expect("unsupported multipart part builds")
}

fn multipart_part_value(
    is_binary: bool,
    binary_value_expr: &str,
    text_value_expr: &str,
) -> CodeBlock {
    sigil_quote!(RustLang {
        $if(is_binary) {
            multipart_body.extend_from_slice(&$L(binary_value_expr));
        } $else {
            multipart_body.extend_from_slice($L(text_value_expr).as_bytes());
        }
    })
    .expect("multipart part value builds")
}

fn multipart_part_crlf() -> CodeBlock {
    sigil_quote!(RustLang {
        multipart_body.extend_from_slice(b"\r\n");
    })
    .expect("multipart part CRLF builds")
}

fn emit_part_prefix(
    b: &mut sigil_stitch::code_block::CodeBlockBuilder,
    wire_name: &str,
    is_binary: bool,
    content_type: &str,
    filename_expr: Option<&str>,
) {
    b.add_code(multipart_part_boundary());
    let filename_expr = filename_expr.unwrap_or_default();
    b.add_code(multipart_part_headers(
        is_binary,
        wire_name,
        filename_expr,
        content_type,
    ));
}

fn multipart_part_boundary() -> CodeBlock {
    sigil_quote!(RustLang {
        multipart_body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    })
    .expect("multipart part boundary builds")
}

fn multipart_part_headers(
    is_binary: bool,
    wire_name: &str,
    filename_expr: &str,
    content_type: &str,
) -> CodeBlock {
    sigil_quote!(RustLang {
        $if(is_binary) {
            multipart_body.extend_from_slice(format!("Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\nContent-Type: {}\r\n\r\n", crate::runtime::multipart_header_value($L(wire_name)), crate::runtime::multipart_header_value(&$L(filename_expr)), $L(content_type)).as_bytes());
        } $else {
            multipart_body.extend_from_slice(format!("Content-Disposition: form-data; name=\"{}\"\r\nContent-Type: {}\r\n\r\n", crate::runtime::multipart_header_value($L(wire_name)), $L(content_type)).as_bytes());
        }
    })
    .expect("multipart part headers build")
}
