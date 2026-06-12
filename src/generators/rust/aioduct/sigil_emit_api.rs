//! Aioduct-specific API method body emission (async, generic runtime).

use sigil_stitch::code_block::CodeBlock;

use crate::generators::rust::common::emit_api::{
    BodyEncoding, MultipartPart, OpPlan, RustBackendConfig, binary_field_expr, emit_response_match,
    emit_result_init, optional_binary_field_expr, optional_text_field_expr, render_to_string,
    response_value_expr, rust_string_literal, text_field_expr,
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
        b.add("let _ = self.client;\n", ());
        b.add(&format!("let _ = {};\n", body.var_name), ());
        b.add(
            "return Err(Error::Unsupported(\"multipart/form-data request bodies must be object schemas\"));\n",
            (),
        );
        return b.build().unwrap();
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
        match &body.encoding {
            BodyEncoding::Json => {
                b.add(&format!("req = req.json(&{})?;\n", body.var_name), ());
            }
            BodyEncoding::FormUrlEncoded => {
                b.add(&format!("req = req.form_serde(&{})?;\n", body.var_name), ());
            }
            BodyEncoding::Xml => {
                let media_type = rust_string_literal(&body.media_type);
                b.add(
                    &format!("req = req.header_str(\"Content-Type\", {media_type})?;\n"),
                    (),
                );
                b.add(
                    &format!(
                        "let body_xml = serde_xml_rs::to_string({})?;\n",
                        body.var_name
                    ),
                    (),
                );
                b.add("req = req.body(body_xml.into_bytes());\n", ());
            }
            BodyEncoding::TextPlain => {
                let media_type = rust_string_literal(&body.media_type);
                b.add(
                    &format!("req = req.header_str(\"Content-Type\", {media_type})?;\n"),
                    (),
                );
                b.add(
                    &format!("req = req.body({}.clone().into_bytes());\n", body.var_name),
                    (),
                );
            }
            BodyEncoding::OctetStream => {
                let media_type = rust_string_literal(&body.media_type);
                b.add(
                    &format!("req = req.header_str(\"Content-Type\", {media_type})?;\n"),
                    (),
                );
                b.add(&format!("req = req.body({}.clone());\n", body.var_name), ());
            }
            BodyEncoding::Multipart => {
                emit_multipart_body(&mut b, &body.var_name, &body.multipart_parts);
            }
            BodyEncoding::Other(media_type) => {
                b.add(
                    &format!(
                        "return Err(Error::Unsupported(\"unsupported request body media type: {media_type}\"));\n"
                    ),
                    (),
                );
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
        if part.required {
            if part.is_binary {
                b.add(
                    &format!(
                        "multipart = multipart.file({wire_name}, {wire_name}, \"application/octet-stream\", {});\n",
                        binary_field_expr(body_var, part),
                    ),
                    (),
                );
            } else {
                b.add(
                    &format!(
                        "multipart = multipart.text({wire_name}, {});\n",
                        text_field_expr(body_var, part),
                    ),
                    (),
                );
            }
        } else {
            b.begin_control_flow(
                &format!("if let Some(value) = &{body_var}.{}", part.field_name),
                (),
            );
            if part.is_binary {
                b.add(
                    &format!(
                        "multipart = multipart.file({wire_name}, {wire_name}, \"application/octet-stream\", {});\n",
                        optional_binary_field_expr("value"),
                    ),
                    (),
                );
            } else {
                b.add(
                    &format!(
                        "multipart = multipart.text({wire_name}, {});\n",
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
