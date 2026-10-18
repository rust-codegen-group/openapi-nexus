//! Main code generation orchestrator

use std::path::{Path, PathBuf};

use tracing::{error, info};

use super::registry::GeneratorRegistry;
use crate::codegen::GeneratorType;
use crate::codegen::traits::{CodeGenerator, FileWriter};
use crate::config::Config;
use crate::error::{GenerationFailure, GenerationReport, PipelineError};
use crate::generators::go::http::GoHttpCodeGenerator;
use crate::generators::java::okhttp::JavaOkhttpCodeGenerator;
use crate::generators::kotlin::okhttp::KotlinOkhttpCodeGenerator;
use crate::generators::python::httpx::PythonHttpxCodeGenerator;
use crate::generators::python::requests::PythonRequestsCodeGenerator;
use crate::generators::rust::aioduct::RustAioductCodeGenerator;
use crate::generators::rust::reqwest::RustReqwestCodeGenerator;
use crate::generators::rust::ureq::RustUreqCodeGenerator;
use crate::generators::typescript::fetch::TypeScriptFetchCodeGenerator;
use crate::parser::parse_file;

/// Main code generation orchestrator
pub struct OpenApiCodeGenerator {
    generator_registry: GeneratorRegistry,
}

impl OpenApiCodeGenerator {
    /// Create a new code generator with default configuration
    pub fn new(config: &Config) -> Self {
        let mut generator_registry = GeneratorRegistry::new();
        generator_registry.register_generator(
            GeneratorType::TypeScriptFetch,
            TypeScriptFetchCodeGenerator::new(
                config
                    .generators
                    .get(&GeneratorType::TypeScriptFetch)
                    .cloned()
                    .unwrap_or_default(),
            ),
        );
        generator_registry.register_generator(
            GeneratorType::GoHttp,
            GoHttpCodeGenerator::new(
                config
                    .generators
                    .get(&GeneratorType::GoHttp)
                    .cloned()
                    .unwrap_or_default(),
            ),
        );
        generator_registry.register_generator(
            GeneratorType::RustReqwest,
            RustReqwestCodeGenerator::new(
                config
                    .generators
                    .get(&GeneratorType::RustReqwest)
                    .cloned()
                    .unwrap_or_default(),
            ),
        );
        generator_registry.register_generator(
            GeneratorType::RustUreq,
            RustUreqCodeGenerator::new(
                config
                    .generators
                    .get(&GeneratorType::RustUreq)
                    .cloned()
                    .unwrap_or_default(),
            ),
        );
        generator_registry.register_generator(
            GeneratorType::RustAioduct,
            RustAioductCodeGenerator::new(
                config
                    .generators
                    .get(&GeneratorType::RustAioduct)
                    .cloned()
                    .unwrap_or_default(),
            ),
        );
        generator_registry.register_generator(
            GeneratorType::PythonHttpx,
            PythonHttpxCodeGenerator::new(
                config
                    .generators
                    .get(&GeneratorType::PythonHttpx)
                    .cloned()
                    .unwrap_or_default(),
            ),
        );
        generator_registry.register_generator(
            GeneratorType::PythonRequests,
            PythonRequestsCodeGenerator::new(
                config
                    .generators
                    .get(&GeneratorType::PythonRequests)
                    .cloned()
                    .unwrap_or_default(),
            ),
        );
        generator_registry.register_generator(
            GeneratorType::KotlinOkhttp,
            KotlinOkhttpCodeGenerator::new(
                config
                    .generators
                    .get(&GeneratorType::KotlinOkhttp)
                    .cloned()
                    .unwrap_or_default(),
            ),
        );
        generator_registry.register_generator(
            GeneratorType::JavaOkhttp,
            JavaOkhttpCodeGenerator::new(
                config
                    .generators
                    .get(&GeneratorType::JavaOkhttp)
                    .cloned()
                    .unwrap_or_default(),
            ),
        );

        Self { generator_registry }
    }

    /// Register a generator
    pub fn register_generator<G>(&mut self, generator_type: GeneratorType, generator: G)
    where
        G: CodeGenerator + FileWriter + Send + Sync + 'static,
    {
        self.generator_registry
            .register_generator(generator_type, generator)
    }

    /// Generate code from the configuration.
    ///
    /// Parsing and lowering are fatal because every generator depends on the
    /// shared IR. Individual generators and file writes are collected in the
    /// report so one failed target does not prevent the remaining targets from
    /// running.
    pub fn generate(&self, config: &Config) -> Result<GenerationReport, PipelineError> {
        info!("Parsing OpenAPI specification from: {}", config.input);
        let parsed = match parse_file(Path::new(&config.input)) {
            Ok(parsed) => parsed,
            Err(e) => {
                error!(
                    "Failed to parse OpenAPI file {:?}: {}. Skipping code generation.",
                    config.input, e
                );
                return Err(PipelineError::Parse { source: e });
            }
        };

        let ir = match crate::ir::lower::lower(parsed) {
            Ok(ir) => ir,
            Err(e) => {
                error!("Failed to lower OpenAPI spec to IR: {}. Skipping.", e);
                return Err(PipelineError::Lower { source: e });
            }
        };
        info!(
            "Lowered to IR: {} schemas, {} operations",
            ir.schemas.len(),
            ir.operations.len()
        );

        // Get generators from config
        let generators = config
            .global
            .generators
            .as_ref()
            .cloned()
            .unwrap_or_default();

        let mut report = GenerationReport::default();

        // Generate code for each generator
        for generator_type in generators {
            tracing::info!("Generating code with generator {}", generator_type);

            // Check if generator is registered
            if !self.generator_registry.has_generator(generator_type) {
                error!("Generator {} not found. Skipping.", generator_type);
                continue;
            }

            // Get the generator and generate files
            let generator = match self.generator_registry.get_generator(generator_type) {
                Some(generator) => generator,
                None => {
                    error!(
                        "Generator {} not found in registry. Skipping.",
                        generator_type
                    );
                    continue;
                }
            };

            let files = match generator.generate(&ir) {
                Ok(files) => files,
                Err(e) => {
                    error!(
                        "Failed to generate code with generator {}: {}. Continuing to next generator.",
                        generator_type, e
                    );
                    report.failures.push(GenerationFailure::Generate {
                        generator: generator_type,
                        source: e,
                    });
                    continue;
                }
            };

            // Get output directory for this generator
            let output_dir = config.global.output_for_generator(generator_type);

            // Write files using the FileWriter trait
            if let Err(e) = generator.write_files(Path::new(&output_dir), &files) {
                error!(
                    "Failed to write files for generator {}: {}. Continuing to next generator.",
                    generator_type, e
                );
                report.failures.push(GenerationFailure::Write {
                    generator: generator_type,
                    output_dir: PathBuf::from(output_dir),
                    source: e,
                });
                continue;
            }

            info!(
                "Successfully generated {} files with generator {}",
                files.len(),
                generator_type
            );
            report.succeeded.push(generator_type);
        }

        Ok(report)
    }
}
