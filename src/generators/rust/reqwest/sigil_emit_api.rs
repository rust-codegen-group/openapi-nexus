//! Reqwest-specific API method body emission.

use sigil_stitch::code_block::CodeBlock;

use crate::generators::rust::common::emit_api::{
    OpPlan, RustBackendConfig, emit_response_match, emit_result_init, render_to_string,
};

/// Backend configuration for reqwest (async, no extra generics).
pub fn reqwest_backend_config() -> RustBackendConfig {
    RustBackendConfig {
        is_async: true,
        struct_generics: None,
        client_type_args: None,
    }
}

/// Emit the reqwest-specific method body for an operation.
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

    // Build request
    let method = op.method.to_uppercase();
    let needs_mut_req = !header_params.is_empty() || body.is_some();
    let req_let = if needs_mut_req {
        "let mut req"
    } else {
        "let req"
    };
    b.add(
        &format!("{req_let} = self.client.request(reqwest::Method::{method}, &path).await?;\n"),
        (),
    );

    // Headers
    for p in header_params {
        if p.is_optional {
            let value_expr = render_to_string("v", &p.param.type_expr, false);
            b.begin_control_flow(&format!("if let Some(v) = &{}", p.var_name), ());
            b.add(
                &format!("req = req.header(\"{}\", {value_expr});\n", p.param.name),
                (),
            );
            b.end_control_flow();
        } else {
            let value_expr = render_to_string(&p.var_name, &p.param.type_expr, false);
            b.add(
                &format!("req = req.header(\"{}\", {value_expr});\n", p.param.name),
                (),
            );
        }
    }

    // Body
    if let Some(body) = body {
        b.add(&format!("req = req.json(&{});\n", body.var_name), ());
    }

    // Send
    b.add("let resp = self.client.send(req).await?;\n", ());
    b.add("let status_code = resp.status().as_u16();\n", ());

    // Parse response
    if typed_responses.is_empty() {
        b.add(&format!("Ok({response_type} {{ status_code }})\n"), ());
    } else {
        b.add(
            "let body_bytes = resp.bytes().await.map_err(Error::Network)?;\n",
            (),
        );
        emit_result_init(&mut b, response_type, typed_responses);
        emit_response_match(
            &mut b,
            typed_responses,
            "serde_json::from_slice(&body_bytes)",
        );
        b.add("Ok(result)\n", ());
    }

    b.build().unwrap()
}
