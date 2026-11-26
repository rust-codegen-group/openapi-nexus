//! Python requests code generator: IR → idiomatic Python + requests (synchronous).

use std::sync::OnceLock;

use heck::ToKebabCase as _;

use super::config::PythonRequestsConfig;
use super::runtime::runtime_files;
use crate::codegen::traits::code_generator::CodeGenerator;
use crate::codegen::traits::file_writer::{FileInfo, FileWriter};
use crate::codegen::{GeneratorType, Language};
use crate::generators::GeneratorError;
use crate::generators::python::emit_api::{self, Transport};
use crate::generators::python::project_files::{self, render_file_header};
use crate::generators::python::{emit_models, value::ValuePlan};
use crate::generators::request_inputs::plan_multipart_request_inputs;
use crate::ir::types::{IrInfo, IrSpec};

/// Python requests code generator (synchronous).
#[derive(Debug)]
pub struct PythonRequestsCodeGenerator {
    config: PythonRequestsConfig,
    resolved_package_name: OnceLock<String>,
}

impl Clone for PythonRequestsCodeGenerator {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            resolved_package_name: OnceLock::new(),
        }
    }
}

impl PythonRequestsCodeGenerator {
    pub fn new(config: toml::value::Table) -> Self {
        Self {
            config: PythonRequestsConfig::from(config),
            resolved_package_name: OnceLock::new(),
        }
    }

    fn package_name(&self, info: &IrInfo) -> String {
        self.config
            .package_name
            .clone()
            .unwrap_or_else(|| info.title.to_kebab_case().replace('-', "_"))
    }

    fn generate_ir(&self, ir: &IrSpec) -> Result<Vec<FileInfo>, GeneratorError> {
        let package_name = self.package_name(&ir.info);
        let _ = self.resolved_package_name.set(package_name.clone());
        let header = render_file_header(&ir.info);
        let request_inputs = plan_multipart_request_inputs(ir);

        let values = ValuePlan::with_strategies(
            ir,
            self.config.untagged_union_strategy,
            self.config.tagged_union_strategy,
        );
        let mut files = Vec::new();

        files.extend(emit_models::generate_model_files_with_plan(
            ir,
            &header,
            &request_inputs,
            &values,
        )?);

        files.extend(emit_api::generate_api_files_with_plan(
            ir,
            &header,
            &request_inputs,
            &values,
            Transport::Requests,
        ));

        files.extend(runtime_files(&header, request_inputs.has_uploads()));

        files.extend(project_files::generate_project_files(
            &ir.info,
            &package_name,
            &header,
            &ir.schemas,
            &ir.operations,
            request_inputs.models(),
            request_inputs.has_uploads(),
            "requests>=2.32",
            values.tagged.exports(),
        ));

        Ok(files)
    }
}

impl CodeGenerator for PythonRequestsCodeGenerator {
    fn language(&self) -> Language {
        Language::Python
    }

    fn generator_type(&self) -> GeneratorType {
        GeneratorType::PythonRequests
    }

    fn generate(&self, ir: &IrSpec) -> Result<Vec<FileInfo>, GeneratorError> {
        self.generate_ir(ir)
    }
}

impl FileWriter for PythonRequestsCodeGenerator {
    fn source_dir(&self) -> Option<&str> {
        self.resolved_package_name.get().map(|s| s.as_str())
    }
}
