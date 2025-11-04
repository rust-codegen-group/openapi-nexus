//! Generator configuration and management

use crate::error::Error;
use openapi_nexus_common::Language;

/// Configuration for code generation
#[derive(Debug, Clone)]
pub struct GeneratorConfig {
    /// Output directory for generated code
    pub output_dir: std::path::PathBuf,
    /// Language to generate code for
    pub language: Language,
    /// Whether to overwrite existing files
    pub overwrite: bool,
}

impl Default for GeneratorConfig {
    fn default() -> Self {
        Self {
            output_dir: std::path::PathBuf::from("generated"),
            language: Language::TypeScript,
            overwrite: false,
        }
    }
}

impl GeneratorConfig {
    /// Create a new generator configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the output directory
    pub fn output_dir<P: Into<std::path::PathBuf>>(mut self, dir: P) -> Self {
        self.output_dir = dir.into();
        self
    }

    /// Set the language to generate
    pub fn language(mut self, language: Language) -> Self {
        self.language = language;
        self
    }

    /// Set whether to overwrite existing files
    pub fn overwrite(mut self, overwrite: bool) -> Self {
        self.overwrite = overwrite;
        self
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), Error> {
        // All languages in the enum are supported
        Ok(())
    }
}
