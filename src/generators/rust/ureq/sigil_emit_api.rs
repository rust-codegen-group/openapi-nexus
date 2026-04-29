//! Ureq-specific API method body emission (synchronous, ureq 3.x).

use sigil_stitch::code_block::CodeBlock;

use crate::generators::rust::common::emit_api::{
    OpPlan, RustBackendConfig, emit_response_match, emit_result_init, render_to_string,
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
    let needs_mut_req = !query_params.is_empty() || !header_params.is_empty();
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
            b.add(
                &format!("let resp = req.send_json(&{})?;\n", body.var_name),
                (),
            );
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
        b.add("let body_str = resp.into_body().read_to_string()?;\n", ());
        emit_result_init(&mut b, response_type, typed_responses);
        emit_response_match(&mut b, typed_responses, "serde_json::from_str(&body_str)");
        b.add("Ok(result)\n", ());
    }

    b.build().unwrap()
}
