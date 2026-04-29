//! Hardcoded Go runtime source files.
//!
//! The runtime surface is tiny on purpose: one `runtime` package with a
//! `Client` carrying functional options, an `Authenticator` interface, and
//! typed `APIError`. Advanced needs (retries, hooks, multi-server) are the
//! user's responsibility and can be layered on a wrapped `*http.Client`.

use crate::codegen::traits::file_writer::FileInfo;

const CLIENT_GO: &str = include_str!("runtime/client.go.txt");
const AUTH_GO: &str = include_str!("runtime/auth.go.txt");
const ERRORS_GO: &str = include_str!("runtime/errors.go.txt");

/// Returns client.go, auth.go, errors.go ready to write.
///
/// The category routes these under `<output>/runtime/` via `FileWriter`.
pub fn runtime_files(header: &str) -> Vec<FileInfo> {
    vec![
        FileInfo::runtime("client.go".to_string(), with_header(header, CLIENT_GO)),
        FileInfo::runtime("auth.go".to_string(), with_header(header, AUTH_GO)),
        FileInfo::runtime("errors.go".to_string(), with_header(header, ERRORS_GO)),
    ]
}

fn with_header(header: &str, body: &str) -> String {
    let mut out = String::with_capacity(header.len() + body.len());
    out.push_str(header);
    out.push_str(body);
    out
}
