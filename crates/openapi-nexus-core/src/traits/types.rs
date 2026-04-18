//! Shared types for language generators

/// Information about a generated file
#[derive(Debug, Clone)]
pub struct GeneratedFile {
    pub filename: String,
    pub content: String,
    pub file_type: FileType,
}

/// Type of generated file
#[derive(Debug, Clone)]
pub enum FileType {
    Schema,
    Api,
    Runtime,
    PackageJson,
    TsConfig,
    TsConfigEsm,
    Readme,
}
