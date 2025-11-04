//! Generator configuration and management

use crate::error::Error;

/// Configuration for code generation
#[derive(Debug, Clone)]
pub struct GeneratorConfig {
    /// Output directory for generated code
    pub output_dir: std::path::PathBuf,
    /// Languages to generate code for
    pub languages: Vec<String>,
    /// Whether to create subdirectories for each language
    pub create_subdirs: bool,
    /// Whether to overwrite existing files
    pub overwrite: bool,
}

impl Default for GeneratorConfig {
    fn default() -> Self {
        Self {
            output_dir: std::path::PathBuf::from("generated"),
            languages: vec!["typescript".to_string()],
            create_subdirs: true,
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

    /// Set the languages to generate
    pub fn languages(mut self, languages: Vec<String>) -> Self {
        self.languages = languages;
        self
    }

    /// Add a language to generate
    pub fn add_language(mut self, language: String) -> Self {
        if !self.languages.contains(&language) {
            self.languages.push(language);
        }
        self
    }

    /// Set whether to create subdirectories
    pub fn create_subdirs(mut self, create: bool) -> Self {
        self.create_subdirs = create;
        self
    }

    /// Set whether to overwrite existing files
    pub fn overwrite(mut self, overwrite: bool) -> Self {
        self.overwrite = overwrite;
        self
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), Error> {
        if self.languages.is_empty() {
            let err = Error::UnsupportedLanguage {
                language: "none".to_string(),
            };
            tracing::error!("{}", err);
            return Err(err);
        }

        for language in &self.languages {
            match language.as_str() {
                "typescript" | "ts" | "rust" => {}
                _ => {
                    let err = Error::UnsupportedLanguage {
                        language: language.clone(),
                    };
                    tracing::error!("{}", err);
                    return Err(err);
                }
            }
        }

        Ok(())
    }
}
