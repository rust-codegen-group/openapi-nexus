//! Hardcoded Rust runtime source files for aioduct.

use crate::codegen::traits::file_writer::FileInfo;
use crate::generators::rust::common::project_files::with_header;

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
