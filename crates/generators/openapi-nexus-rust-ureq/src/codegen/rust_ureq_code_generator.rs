//! Rust ureq code generator: IR → idiomatic Rust + ureq (synchronous).

use std::error::Error;

use heck::ToKebabCase as _;

use crate::runtime::runtime_files;
use crate::sigil_emit_api::{emit_method_body, ureq_backend_config};
use openapi_nexus_core::traits::code_generator::CodeGenerator;
use openapi_nexus_core::traits::file_writer::{FileInfo, FileWriter};
use openapi_nexus_core::{GeneratorType, Language};
use openapi_nexus_ir::types::{IrInfo, IrSpec};
use openapi_nexus_rust_common::{
    config::RustGeneratorConfig, emit_api, emit_models, project_files,
};

/// Rust ureq code generator (synchronous).
#[derive(Debug, Clone)]
pub struct RustUreqCodeGenerator {
    config: RustGeneratorConfig,
}

impl RustUreqCodeGenerator {
    pub fn new(config: toml::value::Table) -> Self {
        Self {
            config: RustGeneratorConfig::from(config),
        }
    }

    fn crate_name(&self, info: &IrInfo) -> String {
        self.config
            .crate_name
            .clone()
            .unwrap_or_else(|| info.title.to_kebab_case())
    }

    fn generate_ir(&self, ir: &IrSpec) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let header = project_files::render_file_header(&ir.info);
        let crate_name = self.crate_name(&ir.info);
        let backend_config = ureq_backend_config();

        let mut files = Vec::new();

        files.extend(
            emit_models::generate_model_files(ir, &header).map_err(|msg| {
                Box::<dyn Error + Send + Sync>::from(format!("emit_models: {msg}"))
            })?,
        );

        files.extend(
            emit_api::generate_api_files(ir, &header, &backend_config, &emit_method_body)
                .map_err(|msg| Box::<dyn Error + Send + Sync>::from(format!("emit_api: {msg}")))?,
        );

        files.extend(runtime_files(&header));

        files.push(cargo_toml_file(&crate_name, &ir.info));
        files.push(project_files::lib_rs_file(&header));
        files.push(project_files::readme_file(&ir.info));

        Ok(files)
    }
}

impl CodeGenerator for RustUreqCodeGenerator {
    fn language(&self) -> Language {
        Language::Rust
    }

    fn generator_type(&self) -> GeneratorType {
        GeneratorType::RustUreq
    }

    fn generate(&self, ir: &IrSpec) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        self.generate_ir(ir)
    }
}

impl FileWriter for RustUreqCodeGenerator {
    fn source_dir(&self) -> Option<&str> {
        Some("src")
    }
}

fn cargo_toml_file(crate_name: &str, info: &IrInfo) -> FileInfo {
    let description = info
        .description
        .as_deref()
        .unwrap_or("Generated Rust SDK.")
        .lines()
        .next()
        .unwrap_or("Generated Rust SDK.");
    let content = format!(
        r#"[package]
name = "{crate_name}"
version = "0.1.0"
edition = "2024"
description = "{description}"

[dependencies]
ureq = {{ version = "3", features = ["json"] }}
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
serde_repr = "0.1"
"#,
    );
    FileInfo::project("Cargo.toml".to_string(), content)
}
