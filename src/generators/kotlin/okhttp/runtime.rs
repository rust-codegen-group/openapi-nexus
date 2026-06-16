use crate::codegen::traits::file_writer::FileInfo;

const API_CLIENT_KT: &str = include_str!("runtime/api_client.kt.txt");
const API_EXCEPTION_KT: &str = include_str!("runtime/api_exception.kt.txt");
const AUTH_KT: &str = include_str!("runtime/auth.kt.txt");
const UPLOAD_FILE_KT: &str = include_str!("runtime/upload_file.kt.txt");

pub fn runtime_files(header: &str, package_name: &str, include_upload_file: bool) -> Vec<FileInfo> {
    let runtime_package = format!("{package_name}.runtime");
    let mut files = vec![
        FileInfo::runtime(
            "ApiClient.kt".to_string(),
            repackage(header, API_CLIENT_KT, &runtime_package),
        ),
        FileInfo::runtime(
            "ApiException.kt".to_string(),
            repackage(header, API_EXCEPTION_KT, &runtime_package),
        ),
        FileInfo::runtime(
            "Auth.kt".to_string(),
            repackage(header, AUTH_KT, &runtime_package),
        ),
    ];
    if include_upload_file {
        files.push(FileInfo::runtime(
            "UploadFile.kt".to_string(),
            repackage(header, UPLOAD_FILE_KT, &runtime_package),
        ));
    }
    files
}

fn repackage(header: &str, body: &str, runtime_package: &str) -> String {
    let patched = body.replacen("package runtime", &format!("package {runtime_package}"), 1);
    let mut out = String::with_capacity(header.len() + patched.len());
    out.push_str(header);
    out.push_str(&patched);
    out
}
