//! Global configuration settings

use clap::Args;
use serde::{Deserialize, Serialize};

use openapi_nexus_common::Language;

/// Global configuration settings (supports CLI args, env vars, and config files)
#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct GlobalConfig {
    /// Path to the OpenAPI specification file
    #[arg(short, long, env = "OPENAPI_NEXUS_INPUT")]
    #[serde(default)]
    pub input: String,

    /// Output directory for generated code
    #[arg(short, long, env = "OPENAPI_NEXUS_OUTPUT", default_value = default_output())]
    #[serde(default = "default_output_string")]
    pub output: String,

    /// Language to generate code for
    #[arg(short, long, env = "OPENAPI_NEXUS_LANGUAGE")]
    pub language: Option<Language>,
}

fn default_output() -> &'static str {
    "generated"
}

fn default_output_string() -> String {
    default_output().to_string()
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            input: String::new(),
            output: default_output_string(),
            language: None,
        }
    }
}

impl GlobalConfig {
    /// Resolve configuration with defaults applied
    ///
    /// Takes required values that must come from CLI (input, language)
    /// and merges with optional values from config/env, applying defaults.
    pub fn resolve(self, input: String, language: Language) -> Result<Self, String> {
        if input.is_empty() {
            return Err("Input is required and cannot be empty".to_string());
        }

        Ok(Self {
            input,
            output: self.output,
            language: Some(language),
        })
    }

    /// Get input
    pub fn input(&self) -> &str {
        &self.input
    }

    /// Get output with default
    pub fn output(&self) -> &str {
        &self.output
    }

    /// Get language
    pub fn language(&self) -> Language {
        self.language.unwrap_or(Language::TypeScript)
    }
}
