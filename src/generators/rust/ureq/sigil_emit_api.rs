//! Ureq-specific API method body emission (synchronous, ureq 3.x).

use sigil_stitch::code_block::CodeBlock;
use sigil_stitch::prelude::sigil_quote;

use crate::generators::rust::common::emit_api::{
    BodyEncoding, MultipartPart, MultipartValueEncoding, OpPlan, RustBackendConfig,
    binary_field_expr, emit_response_match, emit_result_init, optional_binary_field_expr,
    optional_text_field_expr, render_to_string, response_value_expr, rust_field_name,
    rust_string_literal, text_field_expr,
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
        b.add_code(unsupported_multipart_body(&body.var_name));
        return b.build().unwrap();
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
    let req_let = if needs_mut_req {
        "let mut req"
    } else {
        "let req"
    };
    b.add(&format!("{req_let} = self.client.{method}(&path);\n"), ());

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
            match &body.encoding {
                BodyEncoding::Json => {
                    b.add_code(json_send(&body.var_name));
                }
                BodyEncoding::FormUrlEncoded => {
                    emit_form_body(&mut b, &body.var_name);
                    b.add_code(form_send());
                }
                BodyEncoding::Xml => {
                    b.add_code(xml_send(&body.var_name, &body.media_type));
                }
                BodyEncoding::TextPlain => {
                    b.add_code(text_send(&body.var_name, &body.media_type));
                }
                BodyEncoding::OctetStream => {
                    b.add_code(octet_stream_send(&body.var_name, &body.media_type));
                }
                BodyEncoding::Multipart => {
                    emit_multipart_body(&mut b, &body.var_name, &body.multipart_parts);
                    b.add_code(multipart_send());
                }
                BodyEncoding::Other(media_type) => {
                    b.add_code(unsupported_media_type_body(media_type));
                }
            }
        } else {
            b.add("let resp = req.send_empty()?;\n", ());
        }
    } else {
        b.add("let resp = req.call()?;\n", ());
    }
    b.add("let status_code = resp.status().as_u16();\n", ());

    // Parse response
    if typed_responses.is_empty() {
        b.add(&format!("Ok({response_type} {{ status_code }})\n"), ());
    } else {
        b.add("let body_bytes = resp.into_body().read_to_vec()?;\n", ());
        emit_result_init(&mut b, response_type, typed_responses);
        emit_response_match(&mut b, typed_responses, &|tr| {
            response_value_expr(tr, "body_bytes.as_slice()")
        });
        b.add("Ok(result)\n", ());
    }

    b.build().unwrap()
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

fn json_send(body_var: &str) -> CodeBlock {
    sigil_quote!(RustLang {
        let resp = req.send_json(&$L(body_var))?;
    })
    .expect("json send builds")
}

fn form_send() -> CodeBlock {
    let pairs_iter = "form_pairs.iter().map(|(key, value)| (key.as_str(), value.as_str()))";
    sigil_quote!(RustLang {
        let resp = req.send_form($L(pairs_iter))?;
    })
    .expect("form send builds")
}

fn xml_send(body_var: &str, media_type: &str) -> CodeBlock {
    sigil_quote!(RustLang {
        let body_xml = serde_xml_rs::to_string($L(body_var))?;
        req = req.header("Content-Type", $L(rust_string_literal(media_type)));
        let resp = req.send(body_xml)?;
    })
    .expect("xml send builds")
}

fn text_send(body_var: &str, media_type: &str) -> CodeBlock {
    sigil_quote!(RustLang {
        req = req.header("Content-Type", $L(rust_string_literal(media_type)));
        let resp = req.send($L(body_var).as_str())?;
    })
    .expect("text send builds")
}

fn octet_stream_send(body_var: &str, media_type: &str) -> CodeBlock {
    sigil_quote!(RustLang {
        req = req.header("Content-Type", $L(rust_string_literal(media_type)));
        let resp = req.send($L(body_var).clone())?;
    })
    .expect("octet-stream send builds")
}

fn multipart_send() -> CodeBlock {
    sigil_quote!(RustLang {
        let resp = req.send(multipart_body)?;
    })
    .expect("multipart send builds")
}

fn emit_multipart_body(
    b: &mut sigil_stitch::code_block::CodeBlockBuilder,
    body_var: &str,
    parts: &[MultipartPart],
) {
    b.add(
        "let boundary = format!(\"openapi-nexus-{}\", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|duration| duration.as_nanos()).unwrap_or(0));\n",
        (),
    );
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
    b.add(
        "let value = match value { serde_json::Value::String(s) => s, other => other.to_string() };\n",
        (),
    );
    b.add("form_pairs.push((key, value));\n", ());
    b.end_control_flow();
    b.end_control_flow();
    b.end_control_flow();
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
            b.add(
                "return Err(Error::Unsupported(\"unsupported multipart part content type\"));\n",
                (),
            );
        } else {
            let field_name = rust_field_name(&part.wire_name);
            b.begin_control_flow(&format!("if {body_var}.{field_name}.is_some()"), ());
            b.add(
                "return Err(Error::Unsupported(\"unsupported multipart part content type\"));\n",
                (),
            );
            b.end_control_flow();
        }
        return;
    }
    if part.required {
        emit_part_prefix(b, &wire_name, part.is_binary, &content_type);
        if part.is_binary {
            b.add(
                &format!(
                    "multipart_body.extend_from_slice(&{});\n",
                    binary_field_expr(body_var, part),
                ),
                (),
            );
        } else {
            b.add(
                &format!(
                    "multipart_body.extend_from_slice({}.as_bytes());\n",
                    text_field_expr(body_var, part),
                ),
                (),
            );
        }
        b.add("multipart_body.extend_from_slice(b\"\\r\\n\");\n", ());
    } else {
        let field_name = rust_field_name(&part.wire_name);
        b.begin_control_flow(
            &format!("if let Some(value) = &{body_var}.{field_name}"),
            (),
        );
        emit_part_prefix(b, &wire_name, part.is_binary, &content_type);
        if part.is_binary {
            b.add(
                &format!(
                    "multipart_body.extend_from_slice(&{});\n",
                    optional_binary_field_expr("value"),
                ),
                (),
            );
        } else {
            b.add(
                &format!(
                    "multipart_body.extend_from_slice({}.as_bytes());\n",
                    optional_text_field_expr("value", part),
                ),
                (),
            );
        }
        b.add("multipart_body.extend_from_slice(b\"\\r\\n\");\n", ());
        b.end_control_flow();
    }
}

fn emit_part_prefix(
    b: &mut sigil_stitch::code_block::CodeBlockBuilder,
    wire_name: &str,
    is_binary: bool,
    content_type: &str,
) {
    b.add(
        "multipart_body.extend_from_slice(format!(\"--{}\\r\\n\", boundary).as_bytes());\n",
        (),
    );
    if is_binary {
        b.add(
            &format!(
                "multipart_body.extend_from_slice(format!(\"Content-Disposition: form-data; name=\\\"{{}}\\\"; filename=\\\"{{}}\\\"\\r\\nContent-Type: {{}}\\r\\n\\r\\n\", {wire_name}, {wire_name}, {content_type}).as_bytes());\n",
            ),
            (),
        );
    } else {
        b.add(
            &format!(
                "multipart_body.extend_from_slice(format!(\"Content-Disposition: form-data; name=\\\"{{}}\\\"\\r\\nContent-Type: {{}}\\r\\n\\r\\n\", {wire_name}, {content_type}).as_bytes());\n",
            ),
            (),
        );
    }
}
