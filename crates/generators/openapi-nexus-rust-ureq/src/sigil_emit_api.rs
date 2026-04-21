//! Ureq-specific API method body emission (synchronous).

use std::collections::HashSet;

use openapi_nexus_rust_common::emit_api::{OpPlan, RustBackendConfig, render_to_string};

/// Backend configuration for ureq (synchronous, no extra generics).
pub fn ureq_backend_config() -> RustBackendConfig {
    RustBackendConfig {
        is_async: false,
        struct_generics: None,
        client_type_args: None,
    }
}

/// Emit the ureq-specific method body for an operation.
pub fn emit_method_body(plan: &OpPlan<'_>) -> String {
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

    let mut out = String::new();

    // Build path
    if path_params.is_empty() {
        out.push_str(&format!(
            "        let path = \"{}\".to_string();\n",
            op.path
        ));
    } else {
        out.push_str(&format!(
            "        let mut path = \"{}\".to_string();\n",
            op.path
        ));
        for p in path_params {
            let placeholder = format!("{{{}}}", p.param.name);
            let value_expr = render_to_string(&p.var_name, &p.param.type_expr, p.is_optional);
            out.push_str(&format!(
                "        path = path.replace(\"{placeholder}\", &{value_expr});\n"
            ));
        }
    }

    // Build query string
    if !query_params.is_empty() {
        out.push_str("        let mut query_parts: Vec<(&str, String)> = Vec::new();\n");
        for p in query_params {
            if p.is_optional {
                let value_expr = render_to_string("v", &p.param.type_expr, false);
                out.push_str(&format!(
                    "        if let Some(v) = &{} {{\n            query_parts.push((\"{}\", {value_expr}));\n        }}\n",
                    p.var_name, p.param.name,
                ));
            } else {
                let value_expr = render_to_string(&p.var_name, &p.param.type_expr, false);
                out.push_str(&format!(
                    "        query_parts.push((\"{}\", {value_expr}));\n",
                    p.param.name,
                ));
            }
        }
        out.push_str("        if !query_parts.is_empty() {\n");
        out.push_str("            let qs: Vec<String> = query_parts.iter().map(|(k, v)| format!(\"{}={}\", k, v)).collect();\n");
        out.push_str("            path = format!(\"{}?{}\", path, qs.join(\"&\"));\n");
        out.push_str("        }\n");
    }

    // Build request
    let method = op.method.to_uppercase();
    out.push_str(&format!(
        "        let mut req = self.client.request(\"{method}\", &path)?;\n"
    ));

    // Headers
    for p in header_params {
        if p.is_optional {
            let value_expr = render_to_string("v", &p.param.type_expr, false);
            out.push_str(&format!(
                "        if let Some(v) = &{} {{\n            req = req.header(\"{}\", &{value_expr});\n        }}\n",
                p.var_name, p.param.name,
            ));
        } else {
            let value_expr = render_to_string(&p.var_name, &p.param.type_expr, false);
            out.push_str(&format!(
                "        req = req.header(\"{}\", &{value_expr});\n",
                p.param.name,
            ));
        }
    }

    // Send (with or without body)
    if let Some(body) = body {
        out.push_str(&format!(
            "        let resp = req.send_json(&{})?;\n",
            body.var_name
        ));
    } else {
        out.push_str("        let resp = req.send()?;\n");
    }
    out.push_str("        let status_code = resp.status();\n");

    // Parse response
    if typed_responses.is_empty() {
        out.push_str(&format!("        Ok({response_type} {{ status_code }})\n"));
    } else {
        out.push_str(
            "        let body_str = resp.into_body().read_to_string().map_err(Error::Io)?;\n",
        );
        out.push_str(&format!("        let mut result = {response_type} {{\n"));
        out.push_str("            status_code,\n");
        let mut seen: HashSet<String> = HashSet::new();
        for tr in typed_responses {
            if !seen.insert(tr.field_name.clone()) {
                continue;
            }
            out.push_str(&format!("            {}: None,\n", tr.field_name));
        }
        out.push_str("        };\n");

        out.push_str("        match status_code {\n");
        seen.clear();
        for tr in typed_responses {
            if !seen.insert(format!("{}-{}", tr.status, tr.field_name)) {
                continue;
            }
            let status_pattern = if tr.status == "default" {
                "_".to_string()
            } else {
                tr.status.clone()
            };
            out.push_str(&format!("            {status_pattern} => {{\n"));
            out.push_str(&format!(
                "                result.{} = Some(serde_json::from_str(&body_str).map_err(Error::Deserialize)?);\n",
                tr.field_name
            ));
            out.push_str("            }\n");
        }
        if !typed_responses.iter().any(|tr| tr.status == "default") {
            out.push_str("            _ => {}\n");
        }
        out.push_str("        }\n");
        out.push_str("        Ok(result)\n");
    }

    out
}
