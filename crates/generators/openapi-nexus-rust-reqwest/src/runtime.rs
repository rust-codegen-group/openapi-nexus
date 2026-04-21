//! Hardcoded Rust runtime source files.
//!
//! The runtime provides a reqwest-based `Client`, `Error` enum, and `Auth`
//! trait. These ship as-is in every generated SDK.

use openapi_nexus_core::traits::file_writer::FileInfo;

const CLIENT_RS: &str = include_str!("runtime/client.rs.txt");
const ERROR_RS: &str = include_str!("runtime/error.rs.txt");
const AUTH_RS: &str = include_str!("runtime/auth.rs.txt");
const MOD_RS: &str = include_str!("runtime/mod.rs.txt");

/// Returns runtime files ready to write.
pub fn runtime_files(header: &str) -> Vec<FileInfo> {
    vec![
        FileInfo::runtime("client.rs".to_string(), with_header(header, CLIENT_RS)),
        FileInfo::runtime("error.rs".to_string(), with_header(header, ERROR_RS)),
        FileInfo::runtime("auth.rs".to_string(), with_header(header, AUTH_RS)),
        FileInfo::runtime("mod.rs".to_string(), with_header(header, MOD_RS)),
    ]
}

fn with_header(header: &str, body: &str) -> String {
    let mut out = String::with_capacity(header.len() + body.len());
    out.push_str(header);
    out.push_str(body);
    out
}
