//! Main code generation orchestrator

use std::path::Path;

use tracing::{error, info};

use crate::generator_registry::GeneratorRegistry;
use openapi_nexus_config::Config;
use openapi_nexus_core::GeneratorType;
use openapi_nexus_core::traits::{CodeGenerator, FileWriter};
use openapi_nexus_go_http::GoHttpCodeGenerator;
use openapi_nexus_parser::parse_file;
use openapi_nexus_python_httpx::PythonHttpxCodeGenerator;
use openapi_nexus_rust_aioduct::RustAioductCodeGenerator;
use openapi_nexus_rust_reqwest::RustReqwestCodeGenerator;
use openapi_nexus_rust_ureq::RustUreqCodeGenerator;
use openapi_nexus_typescript_fetch::TypeScriptFetchCodeGenerator;

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

    /// Generate code from the configuration
    /// Logs errors and continues to generate the next generator instead of returning errors
    pub fn generate(&self, config: &Config) {
        info!("Parsing OpenAPI specification from: {}", config.input);
        let parsed = match parse_file(Path::new(&config.input)) {
            Ok(parsed) => parsed,
            Err(e) => {
                error!(
                    "Failed to parse OpenAPI file {:?}: {}. Skipping code generation.",
                    config.input, e
                );
                return;
            }
        };

        let ir = match openapi_nexus_ir::lower::lower(parsed) {
            Ok(ir) => ir,
            Err(e) => {
                error!("Failed to lower OpenAPI spec to IR: {}. Skipping.", e);
                return;
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
                continue;
            }

            info!(
                "Successfully generated {} files with generator {}",
                files.len(),
                generator_type
            );
        }
    }
}
