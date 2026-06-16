//! Hardcoded Rust runtime source files for ureq (synchronous).

use crate::codegen::traits::file_writer::FileInfo;
use crate::generators::rust::common::project_files::with_header;

const CLIENT_RS: &str = include_str!("runtime/client.rs.txt");
const ERROR_RS: &str = include_str!("runtime/error.rs.txt");
const AUTH_RS: &str = include_str!("runtime/auth.rs.txt");
const MOD_RS: &str = include_str!("runtime/mod.rs.txt");
const UPLOAD_FILE_RS: &str = include_str!("runtime/upload_file.rs.txt");

/// Returns runtime files ready to write.
pub fn runtime_files(header: &str, include_upload_file: bool) -> Vec<FileInfo> {
    let mut mod_rs = MOD_RS.to_string();
    let mut files = vec![
        FileInfo::runtime("client.rs".to_string(), with_header(header, CLIENT_RS)),
        FileInfo::runtime("error.rs".to_string(), with_header(header, ERROR_RS)),
        FileInfo::runtime("auth.rs".to_string(), with_header(header, AUTH_RS)),
    ];
    if include_upload_file {
        mod_rs.push_str(
            "mod upload_file;\npub use upload_file::{multipart_header_value, UploadFile};\n",
        );
        files.push(FileInfo::runtime(
            "upload_file.rs".to_string(),
            with_header(header, UPLOAD_FILE_RS),
        ));
    }
    files.push(FileInfo::runtime(
        "mod.rs".to_string(),
        with_header(header, &mod_rs),
    ));
    files
}
