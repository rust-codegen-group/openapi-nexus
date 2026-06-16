use std::collections::HashMap;

use openapi_nexus::generators::go::http::GoHttpCodeGenerator;
use openapi_nexus::generators::java::okhttp::JavaOkhttpCodeGenerator;
use openapi_nexus::generators::kotlin::okhttp::KotlinOkhttpCodeGenerator;
use openapi_nexus::generators::python::httpx::PythonHttpxCodeGenerator;
use openapi_nexus::generators::python::requests::PythonRequestsCodeGenerator;
use openapi_nexus::generators::rust::aioduct::RustAioductCodeGenerator;
use openapi_nexus::generators::rust::reqwest::RustReqwestCodeGenerator;
use openapi_nexus::generators::rust::ureq::RustUreqCodeGenerator;
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
fn multipart_wire_construction_is_pinned_across_clients() {
    let fixture = read_fixture("valid/multipart-edge-cases.yaml");

    let go_files = generate_files(&GoHttpCodeGenerator::new(empty_config()), &fixture).unwrap();
    let go_api = generated_file(&go_files, "apis/multipart.go");
    let go_input = generated_file(
        &go_files,
        "models/send_optional_parts_multipart_request_body.go",
    );
    assert!(go_api.contains("multipart.NewWriter(buf)"));
    assert!(go_api.contains("mime.FormatMediaType(\"form-data\""));
    assert!(go_api.contains("value.FilenameOrDefault(\"file\")"));
    assert!(go_api.contains("partWriter.Write(value.Data)"));
    assert!(go_api.contains("strconv.FormatInt(int64(value), 10)"));
    assert!(go_api.contains("strconv.FormatBool(value)"));
    assert!(go_input.contains("File *runtime.UploadFile"));
    assert!(!go_api.contains("filename=\"file\""));

    let java_files =
        generate_files(&JavaOkhttpCodeGenerator::new(empty_config()), &fixture).unwrap();
    let java_api = generated_file(&java_files, "apis/MultipartApi.java");
    let java_input = generated_file(
        &java_files,
        "models/SendOptionalPartsMultipartRequestBody.java",
    );
    assert!(java_api.contains("client.newRequestWithBody(\"POST\", path, null, multipartBody)"));
    assert!(java_api.contains("RequestBody multipartBody = RequestBody.create(new byte[0], null)"));
    assert!(java_api.contains("if (body != null)"));
    assert!(java_api.contains("if (body.getFile() != null)"));
    assert!(java_api.contains("body.getFile().filenameOrDefault(\"file\")"));
    assert!(java_api.contains("body.getFile().getData()"));
    assert!(java_input.contains("private final UploadFile file;"));
    assert!(!java_api.contains("addFormDataPart(\"file\", \"file\""));

    let kotlin_files =
        generate_files(&KotlinOkhttpCodeGenerator::new(empty_config()), &fixture).unwrap();
    let kotlin_api = generated_file(&kotlin_files, "apis/MultipartApi.kt");
    let kotlin_input = generated_file(
        &kotlin_files,
        "models/SendOptionalPartsMultipartRequestBody.kt",
    );
    assert!(kotlin_api.contains("client.newRequestWithBody(\"POST\", path, null, multipartBody)"));
    assert!(kotlin_api.contains("body: SendOptionalPartsMultipartRequestBody?"));
    assert!(kotlin_api.contains("var multipartBody = ByteArray(0).toRequestBody(null)"));
    assert!(kotlin_api.contains("if (body != null)"));
    assert!(kotlin_api.contains("if (body.file != null)"));
    assert!(kotlin_api.contains("body.file.filenameOrDefault(\"file\")"));
    assert!(kotlin_api.contains("body.file.data.toRequestBody"));
    assert!(kotlin_input.contains("val file: UploadFile? = null"));
    assert!(!kotlin_api.contains("addFormDataPart(\"file\", \"file\""));

    let httpx_files =
        generate_files(&PythonHttpxCodeGenerator::new(empty_config()), &fixture).unwrap();
    let httpx_api = generated_file(&httpx_files, "apis/multipart_api.py");
    let httpx_input = generated_file(
        &httpx_files,
        "models/send_optional_parts_multipart_request_body.py",
    );
    assert!(httpx_api.contains("files: dict[str, object] = {}"));
    assert!(httpx_api.contains("files[\"note\"] = (None, str(body.note), \"text/plain\")"));
    assert!(
        httpx_api
            .contains("files[\"file\"] = (body.file.filename_or_default(\"file\"), body.file.data")
    );
    assert!(httpx_api.contains("files=files if files else None"));
    assert!(httpx_input.contains("file: UploadFile | None = None"));
    assert!(!httpx_api.contains("data=data"));
    assert!(!httpx_api.contains("files[\"file\"] = (\"file\""));

    let requests_files =
        generate_files(&PythonRequestsCodeGenerator::new(empty_config()), &fixture).unwrap();
    let requests_api = generated_file(&requests_files, "apis/multipart_api.py");
    let requests_input = generated_file(
        &requests_files,
        "models/send_optional_parts_multipart_request_body.py",
    );
    assert!(requests_api.contains("files: dict[str, object] = {}"));
    assert!(requests_api.contains("files[\"note\"] = (None, str(body.note), \"text/plain\")"));
    assert!(
        requests_api
            .contains("files[\"file\"] = (body.file.filename_or_default(\"file\"), body.file.data")
    );
    assert!(requests_api.contains("files=files if files else None"));
    assert!(requests_input.contains("file: UploadFile | None = None"));
    assert!(!requests_api.contains("data=data"));
    assert!(!requests_api.contains("files[\"file\"] = (\"file\""));

    let ts_files =
        generate_files(&TypeScriptFetchCodeGenerator::new(empty_config()), &fixture).unwrap();
    let ts_api = generated_file(&ts_files, "apis/MultipartApi.ts");
    let ts_input = generated_file(&ts_files, "models/SendOptionalPartsMultipartRequestBody.ts");
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
    assert!(ts_api.contains("uploadFileFilename(requestParameters.body.file, 'file')"));
    assert!(ts_api.contains("uploadFileData(requestParameters.body.file)"));
    assert!(ts_input.contains("file?: UploadFileInput;"));
    assert!(ts_api.contains("multipartChunks.push(String(requestParameters.body.note));"));
    assert!(!ts_api.contains("filename=\"file\""));

    let reqwest_files =
        generate_files(&RustReqwestCodeGenerator::new(empty_config()), &fixture).unwrap();
    let reqwest_api = generated_file(&reqwest_files, "apis/multipart.rs");
    let reqwest_input = generated_file(
        &reqwest_files,
        "models/send_optional_parts_multipart_request_body.rs",
    );
    assert!(
        reqwest_api.contains("body: Option<&crate::models::SendOptionalPartsMultipartRequestBody>")
    );
    assert!(reqwest_api.contains(".file_name(value.filename_or_default(\"file\").to_string())"));
    assert!(reqwest_api.contains("reqwest::multipart::Part::bytes(value.data.clone())"));
    assert!(reqwest_input.contains("pub file: Option<UploadFile>"));
    assert!(!reqwest_api.contains(".file_name(\"file\")"));

    let ureq_files = generate_files(&RustUreqCodeGenerator::new(empty_config()), &fixture).unwrap();
    let ureq_api = generated_file(&ureq_files, "apis/multipart.rs");
    let ureq_input = generated_file(
        &ureq_files,
        "models/send_optional_parts_multipart_request_body.rs",
    );
    assert!(ureq_api.contains(
        "crate::runtime::multipart_header_value(&value.filename_or_default(\"file\").to_string())"
    ));
    assert!(ureq_api.contains("multipart_body.extend_from_slice(&value.data.clone());"));
    assert!(ureq_input.contains("pub file: Option<UploadFile>"));
    assert!(!ureq_api.contains("filename=\\\"file\\\""));

    let aioduct_files =
        generate_files(&RustAioductCodeGenerator::new(empty_config()), &fixture).unwrap();
    let aioduct_api = generated_file(&aioduct_files, "apis/multipart.rs");
    let aioduct_input = generated_file(
        &aioduct_files,
        "models/send_optional_parts_multipart_request_body.rs",
    );
    assert!(
        aioduct_api.contains(
            "multipart.file(\"file\", value.filename_or_default(\"file\").to_string(), \"application/octet-stream\", value.data.clone())"
        )
    );
    assert!(aioduct_input.contains("pub file: Option<UploadFile>"));
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
