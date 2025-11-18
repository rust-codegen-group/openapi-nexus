//! Global configuration settings

use clap::Args;
use serde::{Deserialize, Serialize};

use openapi_nexus_common::{Generator, Language};

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

    /// Generator framework to use (e.g., typescript-fetch)
    #[arg(short = 'g', long, env = "OPENAPI_NEXUS_GENERATOR")]
    pub generator: Option<Generator>,
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
            generator: None,
        }
    }
}

impl GlobalConfig {
    /// Resolve configuration with defaults applied
    ///
    /// Takes required values that must come from CLI (input, generator)
    /// and merges with optional values from config/env, applying defaults.
    pub fn resolve(self, input: String, generator: Generator) -> Result<Self, String> {
        if input.is_empty() {
            return Err("Input is required and cannot be empty".to_string());
        }

        Ok(Self {
            input,
            output: self.output,
            generator: Some(generator),
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

    /// Get generator
    pub fn generator(&self) -> Option<Generator> {
        self.generator
    }

    /// Get language (extracted from generator for backward compatibility)
    pub fn language(&self) -> Option<Language> {
        self.generator.map(|g| g.to_language())
    }
}
