//! Rust ureq code generator: IR → idiomatic Rust + ureq (synchronous).

use std::error::Error;

use heck::ToKebabCase as _;

use super::runtime::runtime_files;
use super::sigil_emit_api::{emit_method_body, ureq_backend_config};
use crate::codegen::traits::code_generator::CodeGenerator;
use crate::codegen::traits::file_writer::{FileInfo, FileWriter};
use crate::codegen::{GeneratorType, Language};
use crate::generators::rust::common::{
    config::RustGeneratorConfig, emit_api, emit_models, project_files,
};
use crate::ir::types::{IrInfo, IrSpec};

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
            emit_models::generate_model_files(ir, &header, &self.config).map_err(|msg| {
                Box::<dyn Error + Send + Sync>::from(format!("emit_models: {msg}"))
            })?,
        );

        let response_extra = self
            .config
            .extra_derives
            .as_ref()
            .and_then(|e| e.response_structs.as_ref());
        files.extend(
            emit_api::generate_api_files(
                ir,
                &header,
                &backend_config,
                response_extra,
                &emit_method_body,
            )
            .map_err(|msg| Box::<dyn Error + Send + Sync>::from(format!("emit_api: {msg}")))?,
        );

        files.extend(runtime_files(&header));

        files.push(cargo_toml_file(&crate_name, &ir.info, &self.config));
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

fn cargo_toml_file(crate_name: &str, info: &IrInfo, config: &RustGeneratorConfig) -> FileInfo {
    use crate::generators::rust::common::config::WorkspaceDepsMode;

    let description = info
        .description
        .as_deref()
        .unwrap_or("Generated Rust SDK.")
        .lines()
        .next()
        .unwrap_or("Generated Rust SDK.")
        .replace('"', r#"\""#);
    let workspace = config.workspace_mode.unwrap_or(false);
    let deps_mode = config.workspace_deps.as_ref().cloned().unwrap_or_default();

    let pkg_section = if workspace {
        format!(
            r#"[package]
name = "{crate_name}"
version.workspace = true
edition.workspace = true
description = "{description}"
"#,
        )
    } else {
        format!(
            r#"[package]
name = "{crate_name}"
version = "0.1.0"
edition = "2024"
description = "{description}"
"#,
        )
    };

    let deps_section = match deps_mode {
        WorkspaceDepsMode::Full => r#"
[dependencies]
ureq.workspace = true
serde.workspace = true
serde_json.workspace = true
serde_repr.workspace = true
"#
        .to_string(),
        WorkspaceDepsMode::WorkspaceVersion => r#"
[dependencies]
ureq = { workspace = true, features = ["json"] }
serde = { workspace = true, features = ["derive"] }
serde_json.workspace = true
serde_repr.workspace = true
"#
        .to_string(),
        WorkspaceDepsMode::Explicit => r#"
[dependencies]
ureq = { version = "3", features = ["json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_repr = "0.1"
"#
        .to_string(),
    };

    let mut content = format!("{pkg_section}{deps_section}");

    if let Some(extra) = &config.extra_derives {
        extra.warn_missing_dependencies();
        for (name, spec) in extra.all_dependencies() {
            match deps_mode {
                WorkspaceDepsMode::Full | WorkspaceDepsMode::WorkspaceVersion => {
                    content.push_str(&format!("{name}.workspace = true\n"));
                }
                WorkspaceDepsMode::Explicit => {
                    content.push_str(&format!("{name} = {spec}\n"));
                }
            }
        }
    }
    if let Some(utoipa_cfg) = &config.utoipa
        && utoipa_cfg.enabled
    {
        let spec = utoipa_cfg.dependency.as_deref().unwrap_or("\"*\"");
        match deps_mode {
            WorkspaceDepsMode::Full | WorkspaceDepsMode::WorkspaceVersion => {
                content.push_str("utoipa.workspace = true\n");
            }
            WorkspaceDepsMode::Explicit => {
                content.push_str(&format!("utoipa = {spec}\n"));
            }
        }
    }

    if workspace {
        content.push_str("\n[lints]\nworkspace = true\n");
    }

    FileInfo::project("Cargo.toml".to_string(), content)
}
