use std::collections::HashMap;

use openapi_nexus::generators::go::http::GoHttpCodeGenerator;
use openapi_nexus::generators::java::okhttp::JavaOkhttpCodeGenerator;
use openapi_nexus::generators::kotlin::okhttp::KotlinOkhttpCodeGenerator;
use openapi_nexus::generators::python::httpx::PythonHttpxCodeGenerator;
use openapi_nexus::generators::python::requests::PythonRequestsCodeGenerator;
use openapi_nexus::generators::typescript::fetch::TypeScriptFetchCodeGenerator;
use openapi_nexus::test_utils::{generate_files, read_fixture};

fn empty_config() -> toml::value::Table {
    toml::value::Table::new()
}

fn generated_file<'a>(files: &'a HashMap<String, String>, suffix: &str) -> &'a str {
    files
        .iter()
        .find(|(name, _)| name.ends_with(suffix))
        .map(|(_, content)| content.as_str())
        .unwrap_or_else(|| {
            panic!(
                "expected generated file ending in {suffix}; got {:?}",
                files.keys().collect::<Vec<_>>()
            )
        })
}

#[test]
fn multipart_wire_construction_is_pinned_across_non_rust_clients() {
    let fixture = read_fixture("valid/multipart-edge-cases.yaml");

    let go_files = generate_files(&GoHttpCodeGenerator::new(empty_config()), &fixture).unwrap();
    let go_api = generated_file(&go_files, "apis/multipart.go");
    assert!(go_api.contains("multipart.NewWriter(buf)"));
    assert!(go_api.contains("strconv.FormatInt(int64(*body.RetryCount), 10)"));
    assert!(go_api.contains("strconv.FormatBool(*body.Enabled)"));

    let java_files =
        generate_files(&JavaOkhttpCodeGenerator::new(empty_config()), &fixture).unwrap();
    let java_api = generated_file(&java_files, "apis/MultipartApi.java");
    assert!(java_api.contains("client.newRequestWithBody(\"POST\", path, null, multipartBody)"));
    assert!(java_api.contains("RequestBody multipartBody = RequestBody.create(new byte[0], null)"));
    assert!(java_api.contains("if (body != null)"));
    assert!(java_api.contains("if (body.getFile() != null)"));

    let kotlin_files =
        generate_files(&KotlinOkhttpCodeGenerator::new(empty_config()), &fixture).unwrap();
    let kotlin_api = generated_file(&kotlin_files, "apis/MultipartApi.kt");
    assert!(kotlin_api.contains("client.newRequestWithBody(\"POST\", path, null, multipartBody)"));
    assert!(kotlin_api.contains("body: OptionalUpload?"));
    assert!(kotlin_api.contains("var multipartBody = ByteArray(0).toRequestBody(null)"));
    assert!(kotlin_api.contains("if (body != null)"));
    assert!(kotlin_api.contains("if (body.file != null)"));

    let httpx_files =
        generate_files(&PythonHttpxCodeGenerator::new(empty_config()), &fixture).unwrap();
    let httpx_api = generated_file(&httpx_files, "apis/multipart_api.py");
    assert!(httpx_api.contains("files: dict[str, object] = {}"));
    assert!(httpx_api.contains("files[\"note\"] = (None, str(body.note), \"text/plain\")"));
    assert!(httpx_api.contains("files=files if files else None"));
    assert!(!httpx_api.contains("data=data"));

    let requests_files =
        generate_files(&PythonRequestsCodeGenerator::new(empty_config()), &fixture).unwrap();
    let requests_api = generated_file(&requests_files, "apis/multipart_api.py");
    assert!(requests_api.contains("files: dict[str, object] = {}"));
    assert!(requests_api.contains("files[\"note\"] = (None, str(body.note), \"text/plain\")"));
    assert!(requests_api.contains("files=files if files else None"));
    assert!(!requests_api.contains("data=data"));

    let ts_files =
        generate_files(&TypeScriptFetchCodeGenerator::new(empty_config()), &fixture).unwrap();
    let ts_api = generated_file(&ts_files, "apis/MultipartApi.ts");
    assert!(ts_api.contains("let requestBody: Blob | undefined = undefined;"));
    assert!(
        ts_api.contains(
            "if (requestParameters.body !== undefined && requestParameters.body !== null)"
        )
    );
    assert!(ts_api.contains("const multipartChunks: Array<string | Blob> = [];"));
    assert!(
        ts_api.contains(
            "Content-Disposition: form-data; name=\"note\"\\r\\nContent-Type: text/plain"
        )
    );
    assert!(ts_api.contains("multipartChunks.push(String(requestParameters.body.note));"));
}

#[test]
fn selected_media_types_drive_request_and_response_wire_code() {
    let media_fixture = read_fixture("valid/media-type-selection.yaml");

    let go_files =
        generate_files(&GoHttpCodeGenerator::new(empty_config()), &media_fixture).unwrap();
    let go_api = generated_file(&go_files, "apis/media.go");
    assert!(go_api.contains(
        "req.Header.Set(\"Content-Type\", \"application/vnd.example+json; charset=utf-8\")"
    ));
    assert!(go_api.contains("payload, err := io.ReadAll(httpResp.Body)"));
    assert!(go_api.contains("bodyBytes, err := io.ReadAll(httpResp.Body)"));

    let java_files = generate_files(
        &JavaOkhttpCodeGenerator::new(empty_config()),
        &media_fixture,
    )
    .unwrap();
    let java_api = generated_file(&java_files, "apis/MediaApi.java");
    assert!(java_api.contains("RequestBody.create(jsonBody, MediaType.get(\"application/vnd.example+json; charset=utf-8\"))"));
    assert!(java_api.contains(
        "byte[] responseBytes = response.body() != null ? response.body().bytes() : new byte[0]"
    ));
    assert!(java_api.contains("status200 = responseBytes"));
    assert!(java_api.contains("status200 = responseText"));

    let kotlin_files = generate_files(
        &KotlinOkhttpCodeGenerator::new(empty_config()),
        &media_fixture,
    )
    .unwrap();
    let kotlin_api = generated_file(&kotlin_files, "apis/MediaApi.kt");
    assert!(kotlin_api.contains(
        "jsonBody.toRequestBody(\"application/vnd.example+json; charset=utf-8\".toMediaType())"
    ));
    assert!(kotlin_api.contains("val responseBytes = response.body?.bytes() ?: ByteArray(0)"));
    assert!(kotlin_api.contains("if (response.code == 200) responseBytes else null"));
    assert!(kotlin_api.contains("if (response.code == 200) responseText else null"));

    let httpx_files = generate_files(
        &PythonHttpxCodeGenerator::new(empty_config()),
        &media_fixture,
    )
    .unwrap();
    let httpx_api = generated_file(&httpx_files, "apis/media_api.py");
    assert!(
        httpx_api.contains(
            "headers[\"Content-Type\"] = \"application/vnd.example+json; charset=utf-8\""
        )
    );
    assert!(httpx_api.contains("return response.content"));
    assert!(httpx_api.contains("return response.text"));

    let requests_files = generate_files(
        &PythonRequestsCodeGenerator::new(empty_config()),
        &media_fixture,
    )
    .unwrap();
    let requests_api = generated_file(&requests_files, "apis/media_api.py");
    assert!(
        requests_api.contains(
            "headers[\"Content-Type\"] = \"application/vnd.example+json; charset=utf-8\""
        )
    );
    assert!(requests_api.contains("return response.content"));
    assert!(requests_api.contains("return response.text"));
}

#[test]
fn non_json_request_bodies_do_not_use_json_serialization() {
    let fixture = read_fixture("valid/request-body-content-types.yaml");

    let go_files = generate_files(&GoHttpCodeGenerator::new(empty_config()), &fixture).unwrap();
    let go_api = generated_file(&go_files, "apis/default.go");
    assert!(go_api.contains("bodyReader = strings.NewReader(*body)"));
    assert!(go_api.contains("bodyReader = bytes.NewReader(body)"));
    assert!(
        go_api.contains("unsupported request body media type: application/x-www-form-urlencoded")
    );
    assert!(go_api.contains("unsupported request body media type: application/xml"));

    let java_files =
        generate_files(&JavaOkhttpCodeGenerator::new(empty_config()), &fixture).unwrap();
    let java_api = generated_file(&java_files, "apis/DefaultApi.java");
    assert!(java_api.contains("RequestBody.create(body, MediaType.get(\"text/plain\"))"));
    assert!(
        java_api.contains("RequestBody.create(body, MediaType.get(\"application/octet-stream\"))")
    );
    assert!(
        java_api.contains("unsupported request body media type: application/x-www-form-urlencoded")
    );
    assert!(java_api.contains("unsupported request body media type: application/xml"));

    let kotlin_files =
        generate_files(&KotlinOkhttpCodeGenerator::new(empty_config()), &fixture).unwrap();
    let kotlin_api = generated_file(&kotlin_files, "apis/DefaultApi.kt");
    assert!(kotlin_api.contains("body.toRequestBody(\"text/plain\".toMediaType())"));
    assert!(kotlin_api.contains("body.toRequestBody(\"application/octet-stream\".toMediaType())"));
    assert!(
        kotlin_api
            .contains("unsupported request body media type: application/x-www-form-urlencoded")
    );
    assert!(kotlin_api.contains("unsupported request body media type: application/xml"));

    let httpx_files =
        generate_files(&PythonHttpxCodeGenerator::new(empty_config()), &fixture).unwrap();
    let httpx_api = generated_file(&httpx_files, "apis/default_api.py");
    assert!(httpx_api.contains("data=body.to_dict()"));
    assert!(httpx_api.contains("content=body"));
    assert!(httpx_api.contains("unsupported request body media type: application/xml"));

    let requests_files =
        generate_files(&PythonRequestsCodeGenerator::new(empty_config()), &fixture).unwrap();
    let requests_api = generated_file(&requests_files, "apis/default_api.py");
    assert!(requests_api.contains("data=body.to_dict()"));
    assert!(requests_api.contains("data=body"));
    assert!(requests_api.contains("unsupported request body media type: application/xml"));

    let ts_files =
        generate_files(&TypeScriptFetchCodeGenerator::new(empty_config()), &fixture).unwrap();
    let ts_api = generated_file(&ts_files, "apis/DefaultApi.ts");
    assert!(
        ts_api.contains("unsupported request body media type: application/x-www-form-urlencoded")
    );
    assert!(ts_api.contains("unsupported request body media type: application/xml"));
}

#[test]
fn unsupported_multipart_schema_fails_explicitly_in_non_rust_clients() {
    let fixture = read_fixture("valid/multipart-unsupported-schema.yaml");
    let expected = "unsupported multipart request body: schema must be object-shaped";

    let go_files = generate_files(&GoHttpCodeGenerator::new(empty_config()), &fixture).unwrap();
    assert!(generated_file(&go_files, "apis/transfer.go").contains(expected));

    let java_files =
        generate_files(&JavaOkhttpCodeGenerator::new(empty_config()), &fixture).unwrap();
    assert!(generated_file(&java_files, "apis/TransferApi.java").contains(expected));

    let kotlin_files =
        generate_files(&KotlinOkhttpCodeGenerator::new(empty_config()), &fixture).unwrap();
    assert!(generated_file(&kotlin_files, "apis/TransferApi.kt").contains(expected));

    let httpx_files =
        generate_files(&PythonHttpxCodeGenerator::new(empty_config()), &fixture).unwrap();
    assert!(generated_file(&httpx_files, "apis/transfer_api.py").contains(expected));

    let requests_files =
        generate_files(&PythonRequestsCodeGenerator::new(empty_config()), &fixture).unwrap();
    assert!(generated_file(&requests_files, "apis/transfer_api.py").contains(expected));

    let ts_files =
        generate_files(&TypeScriptFetchCodeGenerator::new(empty_config()), &fixture).unwrap();
    assert!(generated_file(&ts_files, "apis/TransferApi.ts").contains(expected));
}

#[test]
fn typescript_camel_case_multipart_object_parts_use_wire_json_conversion() {
    let fixture = read_fixture("valid/multipart-nested-object-parts.yaml");
    let config = toml::from_str(
        r#"
property_naming = "camelCase"
"#,
    )
    .unwrap();
    let files = generate_files(&TypeScriptFetchCodeGenerator::new(config), &fixture).unwrap();
    let api = generated_file(&files, "apis/MultipartApi.ts");

    assert!(api.contains("itemConfigToJSON"));
    assert!(api.contains(
        "multipartChunks.push(JSON.stringify(itemConfigToJSON(requestParameters.body.itemConfig)));"
    ));
}
