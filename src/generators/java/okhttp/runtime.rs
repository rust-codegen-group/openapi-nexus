use crate::codegen::traits::file_writer::FileInfo;

const API_CLIENT_JAVA: &str = include_str!("runtime/ApiClient.java.txt");
const API_EXCEPTION_JAVA: &str = include_str!("runtime/ApiException.java.txt");
const AUTHENTICATOR_JAVA: &str = include_str!("runtime/Auth.java.txt");
const BEARER_AUTH_JAVA: &str = include_str!("runtime/BearerAuth.java.txt");
const API_KEY_AUTH_JAVA: &str = include_str!("runtime/ApiKeyAuth.java.txt");
const API_KEY_LOCATION_JAVA: &str = include_str!("runtime/ApiKeyLocation.java.txt");
const UPLOAD_FILE_JAVA: &str = include_str!("runtime/UploadFile.java.txt");

pub fn runtime_files(header: &str, package_name: &str, include_upload_file: bool) -> Vec<FileInfo> {
    let runtime_package = format!("{package_name}.runtime");
    let mut files = vec![
        FileInfo::runtime(
            "ApiClient.java".to_string(),
            repackage(header, API_CLIENT_JAVA, &runtime_package),
        ),
        FileInfo::runtime(
            "ApiException.java".to_string(),
            repackage(header, API_EXCEPTION_JAVA, &runtime_package),
        ),
        FileInfo::runtime(
            "Authenticator.java".to_string(),
            repackage(header, AUTHENTICATOR_JAVA, &runtime_package),
        ),
        FileInfo::runtime(
            "BearerAuth.java".to_string(),
            repackage(header, BEARER_AUTH_JAVA, &runtime_package),
        ),
        FileInfo::runtime(
            "ApiKeyAuth.java".to_string(),
            repackage(header, API_KEY_AUTH_JAVA, &runtime_package),
        ),
        FileInfo::runtime(
            "ApiKeyLocation.java".to_string(),
            repackage(header, API_KEY_LOCATION_JAVA, &runtime_package),
        ),
    ];
    if include_upload_file {
        files.push(FileInfo::runtime(
            "UploadFile.java".to_string(),
            repackage(header, UPLOAD_FILE_JAVA, &runtime_package),
        ));
    }
    files
}

fn repackage(header: &str, body: &str, runtime_package: &str) -> String {
    let patched = body.replacen(
        "package runtime;",
        &format!("package {runtime_package};"),
        1,
    );
    let mut out = String::with_capacity(header.len() + patched.len());
    out.push_str(header);
    out.push_str(&patched);
    out
}
