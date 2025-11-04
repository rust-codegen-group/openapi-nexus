//! Main code generation orchestrator

use std::collections::HashMap;
use std::path::Path;

use snafu::ResultExt as _;
use utoipa::openapi::OpenApi;

use crate::error;
use crate::generator_registry::{GeneratorRegistry, LanguageGenerator};
use crate::parse_error::ParseError;
use openapi_nexus_transforms::{
    TransformPipeline,
    passes::{NamingConvention, NamingConventionPass, ReferenceResolutionPass},
};

/// Main code generation orchestrator
pub struct OpenApiCodeGenerator {
    transform_pipeline: TransformPipeline,
    generator_registry: GeneratorRegistry,
    language_pipelines: HashMap<String, TransformPipeline>,
}

impl OpenApiCodeGenerator {
    /// Create a new code generator with default configuration
    pub fn new() -> Self {
        let pipeline = TransformPipeline::new()
            .add_pass(ReferenceResolutionPass::new())
            .add_pass(NamingConventionPass {
                target_case: NamingConvention::CamelCase,
            });

        Self {
            transform_pipeline: pipeline,
            generator_registry: GeneratorRegistry::new(),
            language_pipelines: HashMap::new(),
        }
    }

    /// Register a language generator
    pub fn register_language_generator<G>(
        &mut self,
        language: impl Into<String>,
        generator: G,
    ) -> Result<(), error::Error>
    where
        G: LanguageGenerator + Send + Sync + 'static,
    {
        self.generator_registry
            .register_generator(language.into(), generator)
            .map_err(|msg| error::Error::Generate {
                source: Box::new(std::io::Error::other(msg)),
            })
    }

    /// Set a custom transformation pipeline for a specific language
    pub fn with_language_pipeline(mut self, language: String, pipeline: TransformPipeline) -> Self {
        self.language_pipelines.insert(language, pipeline);
        self
    }

    /// Generate code from an OpenAPI specification file
    pub fn generate_from_file<P: AsRef<Path>>(
        &self,
        input_path: P,
        output_dir: P,
        languages: &[String],
    ) -> Result<(), error::Error> {
        tracing::info!(
            "Parsing OpenAPI specification from: {:?}",
            input_path.as_ref()
        );
        let openapi = parse_openapi_file(input_path.as_ref()).context(error::ParseSnafu)?;

        for language in languages {
            tracing::info!("Generating {} code", language);

            // Check if generator is registered
            if !self.generator_registry.has_generator(language) {
                return Err(error::Error::GeneratorNotFound {
                    language: language.clone(),
                });
            }

            // Clone the OpenAPI spec for this language
            let mut language_openapi = openapi.clone();

            // Apply transformations - use language-specific pipeline if available, otherwise default
            let pipeline = self
                .language_pipelines
                .get(language)
                .unwrap_or(&self.transform_pipeline);

            tracing::info!("Applying transformations for {}", language);
            pipeline
                .transform(&mut language_openapi)
                .context(error::TransformSnafu)?;

            // Get the generator and generate files
            let generator = self
                .generator_registry
                .get_generator(language)
                .ok_or_else(|| error::Error::GeneratorNotFound {
                    language: language.clone(),
                })?;

            let files = generator
                .generate(&language_openapi)
                .map_err(|e| error::Error::Generate { source: e })?;

            // Write files using the FileWriter trait
            generator
                .write_files(output_dir.as_ref(), &files)
                .map_err(|e| error::Error::Generate { source: e })?;

            tracing::info!(
                "Successfully generated {} files for {}",
                files.len(),
                language
            );
        }

        Ok(())
    }
}

impl Default for OpenApiCodeGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse an OpenAPI specification from a file
fn parse_openapi_file(path: &Path) -> Result<OpenApi, ParseError> {
    let content = std::fs::read_to_string(path).map_err(|e| ParseError::FileRead {
        path: path.to_string_lossy().to_string(),
        source: e,
    })?;

    let file_extension = path.extension().and_then(|ext| ext.to_str());

    match file_extension {
        Some("json") => {
            serde_json::from_str(&content).map_err(|e| ParseError::JsonParse { source: e })
        }
        Some("yaml") | Some("yml") => {
            serde_norway::from_str(&content).map_err(|e| ParseError::YamlParse { source: e })
        }
        Some(ext) => Err(ParseError::UnsupportedFormat {
            format: ext.to_string(),
        }),
        None => {
            // Try JSON first, then YAML
            serde_json::from_str(&content)
                .or_else(|_| serde_norway::from_str(&content))
                .map_err(|_| ParseError::UnsupportedFormat {
                    format: "unknown".to_string(),
                })
        }
    }
}
