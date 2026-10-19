use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use openapi_nexus::codegen::traits::code_generator::CodeGenerator;
use openapi_nexus::codegen::traits::file_writer::FileWriter;
use openapi_nexus::generators::rust::aioduct::RustAioductCodeGenerator;
use openapi_nexus::generators::rust::reqwest::RustReqwestCodeGenerator;
use openapi_nexus::generators::rust::ureq::RustUreqCodeGenerator;
use openapi_nexus::test_utils::{generate_files, read_fixture};

fn empty_config() -> toml::value::Table {
    toml::value::Table::new()
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(name: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after UNIX_EPOCH")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "openapi-nexus-{name}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temporary crate directory should be created");
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn write_generated_crate(files: &HashMap<String, String>, root: &Path) {
    for (relative_path, contents) in files {
        let path = root.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("generated parent directory should be created");
        }
        fs::write(path, contents).expect("generated file should be written");
    }
}

const RUST_TEST_SERVER: &str = r##"
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

fn spawn_one_response_server(status: &str, body: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("test server should bind");
    let addr = listener.local_addr().expect("test server address should be readable");
    let status = status.to_string();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("test server should accept one request");
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request);
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nRetry-After: 120\r\nX-Request-Id: typed-response-test\r\nX-Trace-Id: typed-error-test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("test response should be written");
    });
    format!("http://{addr}")
}

fn spawn_incomplete_success_body_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("test server should bind");
    let addr = listener.local_addr().expect("test server address should be readable");
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("test server should accept one request");
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request);
        let response = "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nX-Request-Id: typed-response-test\r\nContent-Length: 999999\r\nConnection: close\r\n\r\n";
        stream
            .write_all(response.as_bytes())
            .expect("test response headers should be written");
    });
    format!("http://{addr}")
}
"##;

fn add_reqwest_runtime_test(root: &Path) {
    let cargo_toml = root.join("Cargo.toml");
    let mut manifest =
        fs::read_to_string(&cargo_toml).expect("generated Cargo.toml should be readable");
    manifest.push_str(
        r#"

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
thiserror = "2"
snafu = "0.9.1"
"#,
    );
    fs::write(cargo_toml, manifest).expect("generated Cargo.toml should be updated");

    let tests_dir = root.join("tests");
    fs::create_dir_all(&tests_dir).expect("generated tests directory should be created");
    fs::write(
        tests_dir.join("http_error.rs"),
        [
            RUST_TEST_SERVER,
            r##"
use typed_error_responses_api::apis::{
    CreateResourceError, CreateResourceWithWildcardSuccessError, ResourcesApi,
};
use typed_error_responses_api::models::CreateResourceRequest;
use typed_error_responses_api::runtime::client::Client;
use typed_error_responses_api::runtime::error::{ApiCallError, Error};

#[test]
fn api_call_error_is_pointer_sized() {
    assert_eq!(
        std::mem::size_of::<ApiCallError>(),
        std::mem::size_of::<Box<()>>()
    );
}

#[allow(dead_code)]
async fn propagate_uniformly(
    api: &ResourcesApi<'_>,
    request: &CreateResourceRequest,
) -> Result<(), ApiCallError> {
    api.create_resource(request).await?;
    api.check_no_success_body().await?;
    Ok(())
}

#[derive(Debug, thiserror::Error)]
#[error("SDK request failed: {source}")]
struct ThisErrorAppError {
    #[source]
    source: ApiCallError,
}

impl<E> From<E> for ThisErrorAppError
where
    E: Into<ApiCallError>,
{
    fn from(error: E) -> Self {
        Self {
            source: error.into(),
        }
    }
}

#[allow(dead_code)]
async fn propagate_with_thiserror(
    api: &ResourcesApi<'_>,
    request: &CreateResourceRequest,
) -> Result<(), ThisErrorAppError> {
    api.create_resource(request).await?;
    api.check_no_success_body().await?;
    Ok(())
}

#[derive(Debug, snafu::Snafu)]
#[snafu(context(false))]
struct SnafuAppError {
    #[snafu(source(from(generic)))]
    source: ApiCallError,
}

#[allow(dead_code)]
async fn propagate_with_snafu(
    api: &ResourcesApi<'_>,
    request: &CreateResourceRequest,
) -> Result<(), SnafuAppError> {
    api.create_resource(request).await?;
    api.check_no_success_body().await?;
    Ok(())
}

#[tokio::test]
async fn http_503_returns_operation_error_with_typed_body() {
    let body = r#"{"message":"temporarily unavailable","retryable":true}"#;
    let base_url = spawn_one_response_server("503 Service Unavailable", body);
    let client = Client::new(&base_url);
    let api = ResourcesApi::new(&client);
    let request = CreateResourceRequest {
        name: "resource".to_string(),
    };

    let err = api
        .create_resource(&request)
        .await
        .expect_err("HTTP 503 must not be returned as success");
    assert_eq!(err.retry_after_header(), Some("120"));

    match err {
        CreateResourceError::ServerError(api_error) => {
            assert_eq!(api_error.status_code(), 503);
            assert_eq!(api_error.raw_body(), body.as_bytes());
            assert_eq!(
                api_error
                    .headers()
                    .get("retry-after")
                    .and_then(|value| value.to_str().ok()),
                Some("120")
            );
            assert_eq!(
                api_error
                    .headers()
                    .get("x-trace-id")
                    .and_then(|value| value.to_str().ok()),
                Some("typed-error-test")
            );
            let typed_body = api_error
                .body()
                .expect("documented 5XX error body should decode lazily");
            assert_eq!(typed_body.message, "temporarily unavailable");
            assert!(typed_body.retryable);
        }
        other => panic!("expected ServerError variant, got {other:?}"),
    }
}

#[tokio::test]
async fn invalid_error_body_keeps_http_error_and_raw_body() {
    let body = r#"{"message":42,"retryable":true}"#;
    let base_url = spawn_one_response_server("503 Service Unavailable", body);
    let client = Client::new(&base_url);
    let api = ResourcesApi::new(&client);
    let request = CreateResourceRequest {
        name: "resource".to_string(),
    };

    let err = api
        .create_resource(&request)
        .await
        .expect_err("HTTP 503 must remain an operation error even when its body is invalid");

    match err {
        CreateResourceError::ServerError(api_error) => {
            assert_eq!(api_error.status_code(), 503);
            assert_eq!(api_error.raw_body(), body.as_bytes());
            assert!(matches!(api_error.body(), Err(Error::Deserialize(_))));

            let error: ApiCallError = CreateResourceError::ServerError(api_error).into();
            assert_eq!(error.operation_id(), "createResource");
            assert_eq!(error.status_code(), Some(503));
            assert_eq!(error.raw_body(), Some(body.as_bytes()));
            assert!(matches!(
                std::error::Error::source(&error).and_then(|source| source.downcast_ref::<Error>()),
                Some(Error::Deserialize(_))
            ));
        }
        other => panic!("expected ServerError variant, got {other:?}"),
    }
}

#[tokio::test]
async fn operation_error_converts_to_uniform_api_call_error() {
    let body = r#"{"message":"temporarily unavailable","retryable":true}"#;
    let base_url = spawn_one_response_server("503 Service Unavailable", body);
    let client = Client::new(&base_url);
    let api = ResourcesApi::new(&client);
    let request = CreateResourceRequest {
        name: "resource".to_string(),
    };

    let error: ApiCallError = api
        .create_resource(&request)
        .await
        .expect_err("HTTP 503 must be returned as an operation error")
        .into();

    assert_eq!(error.operation_id(), "createResource");
    assert_eq!(error.status_code(), Some(503));
    assert_eq!(error.raw_body(), Some(body.as_bytes()));
    assert_eq!(
        error
            .headers()
            .and_then(|headers| headers.get("retry-after"))
            .and_then(|value| value.to_str().ok()),
        Some("120")
    );
    assert_eq!(
        error.to_string(),
        "operation createResource failed with HTTP status 503"
    );
    assert!(std::error::Error::source(&error).is_none());
}

#[test]
fn transport_error_converts_to_uniform_api_call_error() {
    let error: ApiCallError =
        CreateResourceError::Transport(Error::Unsupported("test transport failure")).into();

    assert_eq!(error.operation_id(), "createResource");
    assert_eq!(error.status_code(), None);
    assert!(error.headers().is_none());
    assert!(error.raw_body().is_none());
    assert!(matches!(
        std::error::Error::source(&error).and_then(|source| source.downcast_ref::<Error>()),
        Some(Error::Unsupported("test transport failure"))
    ));
}

#[tokio::test]
async fn exact_success_status_wins_over_2xx_wildcard() {
    let body = r#"{"id":"exact-created"}"#;
    let base_url = spawn_one_response_server("201 Created", body);
    let client = Client::new(&base_url);
    let api = ResourcesApi::new(&client);
    let request = CreateResourceRequest {
        name: "resource".to_string(),
    };

    let response = api
        .create_resource_with_wildcard_success(&request)
        .await
        .expect("HTTP 201 should be a successful exact response");

    let created = response
        .created
        .expect("201 must match the exact branch before 2XX wildcard");
    assert_eq!(created.id, "exact-created");
    assert!(response.status_2xx.is_none());
    assert_eq!(
        response
            .headers
            .get("x-request-id")
            .and_then(|value| value.to_str().ok()),
        Some("typed-response-test")
    );
}

#[tokio::test]
async fn unexpected_non_success_status_uses_unexpected_detail() {
    let body = r#"{"message":"teapot"}"#;
    let base_url = spawn_one_response_server("418 I'm a teapot", body);
    let client = Client::new(&base_url);
    let api = ResourcesApi::new(&client);
    let request = CreateResourceRequest {
        name: "resource".to_string(),
    };

    let err = api
        .create_resource_with_wildcard_success(&request)
        .await
        .expect_err("undocumented HTTP 418 must not be returned as success");

    match err {
        CreateResourceWithWildcardSuccessError::Unexpected(api_error) => {
            assert_eq!(api_error.status_code(), 418);
            assert_eq!(api_error.raw_body(), body.as_bytes());
        }
        other => panic!("expected Unexpected variant, got {other:?}"),
    }
}

#[tokio::test]
async fn success_without_documented_body_does_not_drain_body() {
    let base_url = spawn_incomplete_success_body_server();
    let client = Client::new(&base_url);
    let api = ResourcesApi::new(&client);

    let response = api
        .check_no_success_body()
        .await
        .expect("success without a documented body must not read an incomplete body");

    assert_eq!(response.status_code, 200);
    assert_eq!(response.x_request_id_header(), Some("typed-response-test"));
    assert_eq!(
        response
            .headers
            .get("x-request-id")
            .and_then(|value| value.to_str().ok()),
        Some("typed-response-test")
    );
}
"##,
        ]
        .join("\n"),
    )
    .expect("generated runtime test should be written");
}

fn add_ureq_runtime_test(root: &Path) {
    let tests_dir = root.join("tests");
    fs::create_dir_all(&tests_dir).expect("generated tests directory should be created");
    fs::write(
        tests_dir.join("http_error.rs"),
        [
            RUST_TEST_SERVER,
            r##"
use typed_error_responses_api::apis::{CreateResourceError, ResourcesApi};
use typed_error_responses_api::models::CreateResourceRequest;
use typed_error_responses_api::runtime::client::Client;
use typed_error_responses_api::runtime::error::ApiCallError;

#[test]
fn api_call_error_is_pointer_sized() {
    assert_eq!(
        std::mem::size_of::<ApiCallError>(),
        std::mem::size_of::<Box<()>>()
    );
}

#[allow(dead_code)]
fn propagate_uniformly(
    api: &ResourcesApi<'_>,
    request: &CreateResourceRequest,
) -> Result<(), ApiCallError> {
    api.create_resource(request)?;
    api.check_no_success_body()?;
    Ok(())
}

#[test]
fn http_503_returns_operation_error_with_typed_body() {
    let body = r#"{"message":"temporarily unavailable","retryable":true}"#;
    let base_url = spawn_one_response_server("503 Service Unavailable", body);
    let client = Client::new(&base_url);
    let api = ResourcesApi::new(&client);
    let request = CreateResourceRequest {
        name: "resource".to_string(),
    };

    let err = api
        .create_resource(&request)
        .expect_err("HTTP 503 must not be returned as transport or success");
    assert_eq!(err.retry_after_header(), Some("120"));

    match err {
        CreateResourceError::ServerError(api_error) => {
            assert_eq!(api_error.status_code(), 503);
            assert_eq!(api_error.raw_body(), body.as_bytes());
            assert_eq!(
                api_error
                    .headers()
                    .get("retry-after")
                    .and_then(|value| value.to_str().ok()),
                Some("120")
            );
            assert_eq!(
                api_error
                    .headers()
                    .get("x-trace-id")
                    .and_then(|value| value.to_str().ok()),
                Some("typed-error-test")
            );
            let typed_body = api_error
                .body()
                .expect("documented 5XX error body should decode");
            assert_eq!(typed_body.message, "temporarily unavailable");
            assert!(typed_body.retryable);
        }
        other => panic!("expected ServerError variant, got {other:?}"),
    }
}

#[test]
fn operation_error_converts_to_uniform_api_call_error() {
    let body = r#"{"message":"temporarily unavailable","retryable":true}"#;
    let base_url = spawn_one_response_server("503 Service Unavailable", body);
    let client = Client::new(&base_url);
    let api = ResourcesApi::new(&client);
    let request = CreateResourceRequest {
        name: "resource".to_string(),
    };

    let error: ApiCallError = api
        .create_resource(&request)
        .expect_err("HTTP 503 must be returned as an operation error")
        .into();

    assert_eq!(error.operation_id(), "createResource");
    assert_eq!(error.status_code(), Some(503));
    assert_eq!(error.raw_body(), Some(body.as_bytes()));
    assert_eq!(
        error
            .headers()
            .and_then(|headers| headers.get("retry-after"))
            .and_then(|value| value.to_str().ok()),
        Some("120")
    );
}

#[test]
fn success_without_documented_body_does_not_drain_body() {
    let base_url = spawn_incomplete_success_body_server();
    let client = Client::new(&base_url);
    let api = ResourcesApi::new(&client);

    let response = api
        .check_no_success_body()
        .expect("success without a documented body must not read an incomplete body");

    assert_eq!(response.status_code, 200);
    assert_eq!(response.x_request_id_header(), Some("typed-response-test"));
    assert_eq!(
        response
            .headers
            .get("x-request-id")
            .and_then(|value| value.to_str().ok()),
        Some("typed-response-test")
    );
}
"##,
        ]
        .join("\n"),
    )
    .expect("generated runtime test should be written");
}

fn add_aioduct_layout_test(root: &Path) {
    let tests_dir = root.join("tests");
    fs::create_dir_all(&tests_dir).expect("generated tests directory should be created");
    fs::write(
        tests_dir.join("api_call_error_layout.rs"),
        r#"use typed_error_responses_api::runtime::error::ApiCallError;

#[test]
fn api_call_error_is_pointer_sized() {
    assert_eq!(
        std::mem::size_of::<ApiCallError>(),
        std::mem::size_of::<Box<()>>()
    );
}
"#,
    )
    .expect("generated layout test should be written");
}

#[test]
fn reqwest_http_503_is_not_success_and_preserves_error_detail() {
    let fixture = read_fixture("valid/typed-error-responses.yaml");
    let files = generate_files(&RustReqwestCodeGenerator::new(empty_config()), &fixture)
        .expect("typed error fixture should generate");
    let temp = TempDir::new("typed-http-error-reqwest");
    write_generated_crate(&files, &temp.path);
    add_reqwest_runtime_test(&temp.path);

    let output = Command::new("cargo")
        .arg("test")
        .arg("--quiet")
        .current_dir(&temp.path)
        .output()
        .expect("generated crate cargo test should run");

    assert!(
        output.status.success(),
        "generated reqwest runtime test failed\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn ureq_http_503_is_not_transport_and_preserves_error_detail() {
    let fixture = read_fixture("valid/typed-error-responses.yaml");
    let files = generate_files(&RustUreqCodeGenerator::new(empty_config()), &fixture)
        .expect("typed error fixture should generate");
    let temp = TempDir::new("typed-http-error-ureq");
    write_generated_crate(&files, &temp.path);
    add_ureq_runtime_test(&temp.path);

    let output = Command::new("cargo")
        .arg("test")
        .arg("--quiet")
        .current_dir(&temp.path)
        .output()
        .expect("generated crate cargo test should run");

    assert!(
        output.status.success(),
        "generated ureq runtime test failed\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn aioduct_api_call_error_is_pointer_sized() {
    let fixture = read_fixture("valid/typed-error-responses.yaml");
    let files = generate_files(&RustAioductCodeGenerator::new(empty_config()), &fixture)
        .expect("typed error fixture should generate");
    let temp = TempDir::new("api-call-error-layout-aioduct");
    write_generated_crate(&files, &temp.path);
    add_aioduct_layout_test(&temp.path);

    let output = Command::new("cargo")
        .arg("test")
        .arg("--quiet")
        .current_dir(&temp.path)
        .output()
        .expect("generated crate cargo test should run");

    assert!(
        output.status.success(),
        "generated aioduct layout test failed\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn check_error_payload_crate(generator: &(impl CodeGenerator + FileWriter)) {
    let fixture = read_fixture("valid/rust-error-payload-shapes.yaml");
    let files = generate_files(generator, &fixture).expect("composite error fixture generates");
    let temp = TempDir::new(&format!("error-payload-{}", generator.generator_type()));
    write_generated_crate(&files, &temp.path);

    let output = Command::new("cargo")
        .arg("check")
        .arg("--quiet")
        .current_dir(&temp.path)
        .output()
        .expect("generated error payload crate cargo check should run");
    assert!(
        output.status.success(),
        "{} error payload crate failed to compile\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        generator.generator_type(),
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

// Value: protects=reqwest generated composite error payloads compile;
// fails_when=payload type syntax, imports, aliases, or decode inference is invalid;
// why_new=existing compiled fixtures only cover named error model payloads;
// seam=CodeGenerator::generate and cargo check; no_claim=live HTTP decoding.
#[test]
fn reqwest_composite_error_payloads_compile() {
    check_error_payload_crate(&RustReqwestCodeGenerator::new(empty_config()));
}

// Value: protects=ureq generated composite error payloads compile;
// fails_when=payload type syntax, imports, aliases, or decode inference is invalid;
// why_new=existing compiled fixtures only cover named error model payloads;
// seam=CodeGenerator::generate and cargo check; no_claim=live HTTP decoding.
#[test]
fn ureq_composite_error_payloads_compile() {
    check_error_payload_crate(&RustUreqCodeGenerator::new(empty_config()));
}

// Value: protects=aioduct generated composite error payloads compile;
// fails_when=payload types or generic backend bodies cannot type-check;
// why_new=existing aioduct layout test only covers named error payloads;
// seam=CodeGenerator::generate and cargo check; no_claim=live HTTP decoding.
#[test]
fn aioduct_composite_error_payloads_compile() {
    check_error_payload_crate(&RustAioductCodeGenerator::new(empty_config()));
}
