//! Hardcoded Rust runtime source files for aioduct.

use crate::codegen::traits::file_writer::FileInfo;
use crate::generators::rust::aioduct::config::{AioductFeatureConfig, AioductTls};
use crate::generators::rust::common::project_files::with_header;

const CLIENT_RS_TEMPLATE: &str = include_str!("runtime/client.rs.txt");
const ERROR_RS: &str = include_str!("runtime/error.rs.txt");
const AUTH_RS: &str = include_str!("runtime/auth.rs.txt");
const MOD_RS: &str = include_str!("runtime/mod.rs.txt");
const UPLOAD_FILE_RS: &str = include_str!("runtime/upload_file.rs.txt");

/// Returns runtime files ready to write.
pub fn runtime_files(
    header: &str,
    aioduct_cfg: &AioductFeatureConfig,
    include_upload_file: bool,
) -> Vec<FileInfo> {
    let client_rs = render_client_rs(aioduct_cfg);
    let mut mod_rs = MOD_RS.to_string();
    let mut files = vec![
        FileInfo::runtime("client.rs".to_string(), with_header(header, &client_rs)),
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

fn render_client_rs(cfg: &AioductFeatureConfig) -> String {
    let has_http3 = cfg
        .features
        .as_ref()
        .is_some_and(|f| f.contains(&"http3".to_string()));
    let tls = cfg.tls.as_ref().unwrap_or(&AioductTls::RustlsRing);

    let constructor = match (tls, has_http3) {
        (AioductTls::RustlsRing | AioductTls::RustlsAwsLcRs, true) => {
            "aioduct::HttpEngineSend::<R, C>::with_http3().expect(\"aioduct HTTP/3 client build\")"
        }
        (AioductTls::RustlsRing | AioductTls::RustlsAwsLcRs, false) => {
            "aioduct::HttpEngineSend::<R, C>::with_rustls()"
        }
        (AioductTls::Disabled, _) => "aioduct::HttpEngineSend::<R, C>::new()",
    };

    CLIENT_RS_TEMPLATE.replace("{{CLIENT_CONSTRUCTOR}}", constructor)
}
