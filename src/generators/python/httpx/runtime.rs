//! Hardcoded Python runtime source files for httpx.

use crate::codegen::traits::file_writer::FileInfo;

const CLIENT_PY: &str = include_str!("runtime/client.py.txt");
const AUTH_PY: &str = include_str!("runtime/auth.py.txt");
const ERRORS_PY: &str = include_str!("runtime/errors.py.txt");

/// Returns runtime files ready to write.
pub fn runtime_files(header: &str) -> Vec<FileInfo> {
    vec![
        FileInfo::runtime("client.py".to_string(), with_header(header, CLIENT_PY)),
        FileInfo::runtime("auth.py".to_string(), with_header(header, AUTH_PY)),
        FileInfo::runtime("errors.py".to_string(), with_header(header, ERRORS_PY)),
        FileInfo::runtime("__init__.py".to_string(), runtime_init(header)),
    ]
}

fn with_header(header: &str, body: &str) -> String {
    let mut out = String::with_capacity(header.len() + body.len());
    out.push_str(header);
    out.push_str(body);
    out
}

fn runtime_init(header: &str) -> String {
    let mut out = String::new();
    out.push_str(header);
    out.push_str("from .auth import ApiKeyAuth as ApiKeyAuth\n");
    out.push_str("from .auth import Authenticator as Authenticator\n");
    out.push_str("from .auth import BearerAuth as BearerAuth\n");
    out.push_str("from .client import Client as Client\n");
    out.push_str("from .errors import ApiError as ApiError\n");
    out
}
