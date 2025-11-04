//! Global configuration settings

use clap::Args;
use serde::{Deserialize, Serialize};

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

    /// Language to generate code for (as string: "typescript", etc.)
    #[arg(short, long, env = "OPENAPI_NEXUS_LANGUAGE")]
    #[serde(default)]
    pub language: String,

    /// Verbose output
    #[arg(short, long, env = "OPENAPI_NEXUS_VERBOSE", default_value_t = default_verbose())]
    #[serde(default = "default_verbose")]
    pub verbose: bool,
}

fn default_output() -> &'static str {
    "generated"
}

fn default_output_string() -> String {
    default_output().to_string()
}

fn default_verbose() -> bool {
    false
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            input: String::new(),
            output: default_output_string(),
            language: String::new(),
            verbose: default_verbose(),
        }
    }
}

impl GlobalConfig {
    /// Resolve configuration with defaults applied
    ///
    /// Takes required values that must come from CLI (input, language)
    /// and merges with optional values from config/env, applying defaults.
    pub fn resolve(self, input: String, language: String) -> Result<Self, String> {
        if input.is_empty() {
            return Err("Input is required and cannot be empty".to_string());
        }
        if language.is_empty() {
            return Err("Language is required and cannot be empty".to_string());
        }

        Ok(Self {
            input,
            output: self.output,
            language,
            verbose: self.verbose,
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
    pub fn language(&self) -> &str {
        &self.language
    }

    /// Get verbose flag with default
    pub fn verbose(&self) -> bool {
        self.verbose
    }
}
