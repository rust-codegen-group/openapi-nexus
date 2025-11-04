//! Main code generation orchestrator

use std::path::Path;

use snafu::ResultExt as _;
use tracing::error;

use crate::error;
use crate::generator_registry::{GeneratorRegistry, LanguageGenerator};
use openapi_nexus_common::Language;
use openapi_nexus_parser::parse_file;

/// Main code generation orchestrator
pub struct OpenApiCodeGenerator {
    generator_registry: GeneratorRegistry,
}

impl OpenApiCodeGenerator {
    /// Create a new code generator with default configuration
    pub fn new() -> Self {
        Self {
            generator_registry: GeneratorRegistry::new(),
        }
    }

    /// Register a language generator
    pub fn register_language_generator<G>(
        &mut self,
        language: Language,
        generator: G,
    ) -> Result<(), error::Error>
    where
        G: LanguageGenerator + Send + Sync + 'static,
    {
        self.generator_registry
            .register_generator(language, generator)
            .map_err(|msg| {
                error!(
                    "Failed to register language generator for {}: {}",
                    language, msg
                );
                error::Error::Generate {
                    source: Box::new(std::io::Error::other(msg)),
                }
            })
    }

    /// Generate code from an OpenAPI specification file
    pub fn generate_from_file<P: AsRef<Path>>(
        &self,
        input_path: P,
        output_dir: P,
        language: Language,
    ) -> Result<(), error::Error> {
        tracing::info!(
            "Parsing OpenAPI specification from: {:?}",
            input_path.as_ref()
        );
        let openapi = parse_file(input_path.as_ref())
            .map_err(|e| {
                error!(
                    "Failed to parse OpenAPI file {:?}: {}",
                    input_path.as_ref(),
                    e
                );
                e
            })
            .context(error::ParseSnafu)?;

        tracing::info!("Generating {} code", language);

        // Check if generator is registered
        if !self.generator_registry.has_generator(language) {
            let err = error::Error::GeneratorNotFound {
                language: language.to_string(),
            };
            error!("{}", err);
            return Err(err);
        }

        // Get the generator and generate files
        let generator = self
            .generator_registry
            .get_generator(language)
            .ok_or_else(|| {
                let err = error::Error::GeneratorNotFound {
                    language: language.to_string(),
                };
                error!("{}", err);
                err
            })?;

        let files = generator.generate(&openapi).map_err(|e| {
            error!("Failed to generate code for {}: {}", language, e);
            error::Error::Generate { source: e }
        })?;

        // Write files using the FileWriter trait
        generator
            .write_files(output_dir.as_ref(), &files)
            .map_err(|e| {
                error!("Failed to write files for {}: {}", language, e);
                error::Error::Generate { source: e }
            })?;

        tracing::info!(
            "Successfully generated {} files for {}",
            files.len(),
            language
        );

        Ok(())
    }
}

impl Default for OpenApiCodeGenerator {
    fn default() -> Self {
        Self::new()
    }
}
