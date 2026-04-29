//! Global configuration settings

use clap::Args;
use serde::{Deserialize, Serialize};

use crate::codegen::GeneratorType;

/// Global configuration settings (supports CLI args, env vars, and config files)
#[derive(Debug, Clone, Args, Serialize, Deserialize)]
pub struct GlobalConfig {
    /// Output directory for generated code
    #[arg(short = 'o', long, env = "OPENAPI_NEXUS_OUTPUT")]
    pub output: Option<String>,

    /// Generator frameworks to use (e.g., typescript-fetch)
    #[arg(short = 'g', long, env = "OPENAPI_NEXUS_GENERATORS")]
    pub generators: Option<Vec<GeneratorType>>,
}

fn default_output() -> String {
    "generated/{generator}".to_string()
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            output: Some(default_output()),
            generators: None,
        }
    }
}

impl GlobalConfig {
    /// Get output path for a specific generator with template rendering
    ///
    /// Replaces template variables in the output string:
    /// - `{generator}`: generator string representation (e.g., "typescript-fetch")
    /// - `{language}`: language name (e.g., "TypeScript")
    /// - `{framework}`: framework name (e.g., "fetch")
    pub fn output_for_generator(&self, generator: GeneratorType) -> String {
        let generator_str = generator.to_string();
        let language = generator.language().to_string();
        let framework = generator.framework();

        let output = self.output.clone().unwrap_or_else(default_output);
        output
            .replace("{generator}", &generator_str)
            .replace("{language}", &language)
            .replace("{framework}", &framework)
    }
}
