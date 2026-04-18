//! Command-line argument definitions

use std::collections::HashMap;
use std::str::FromStr;

use clap::Parser;

use crate::errors::ConfigError;
use crate::global_config::GlobalConfig;
use openapi_nexus_common::GeneratorType;

/// Command-line arguments with environment variable support
#[derive(Debug, Parser)]
#[command(name = "openapi-nexus")]
#[command(about = "Generate code from OpenAPI 3.1 specifications")]
#[command(version)]
pub struct CliArgs {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Parser)]
pub enum Commands {
    /// Generate code from an OpenAPI specification
    Generate {
        /// Path to the OpenAPI specification file
        #[arg(short, long, env = "OPENAPI_NEXUS_INPUT")]
        input: String,

        /// Verbose output
        #[arg(short, long, env = "OPENAPI_NEXUS_VERBOSE")]
        verbose: bool,

        /// Path to configuration file (overrides auto-discovery)
        #[arg(long, env = "OPENAPI_NEXUS_CONFIG")]
        config: Option<String>,

        /// Global configuration options
        #[command(flatten)]
        global: GlobalConfig,

        /// Override generator-specific config values
        /// Format: <generator>.<key>=<value>
        /// Example: --generator-config typescript-fetch.file_naming_convention=PascalCase
        #[arg(long = "generator-config", value_name = "GENERATOR.KEY=VALUE")]
        generator_config: Vec<String>,
    },
}

impl Commands {
    /// Parse generator config overrides from CLI arguments
    /// Returns a map from Generator to TOML table values
    pub fn parse_generator_overrides(
        &self,
    ) -> Result<HashMap<GeneratorType, toml::value::Table>, ConfigError> {
        let generator_configs = match self {
            Commands::Generate {
                generator_config, ..
            } => generator_config,
        };

        let mut overrides: HashMap<GeneratorType, toml::value::Table> = HashMap::new();

        for config_str in generator_configs {
            // Parse format: generator.key=value
            let parts: Vec<&str> = config_str.splitn(2, '=').collect();
            if parts.len() != 2 {
                return Err(ConfigError::ParseOverrides(format!(
                    "Invalid generator config format: '{}'. Expected format: <generator>.<key>=<value>",
                    config_str
                )));
            }

            let key_part = parts[0];
            let value_str = parts[1];

            // Split generator and key
            let key_parts: Vec<&str> = key_part.splitn(2, '.').collect();
            if key_parts.len() != 2 {
                return Err(ConfigError::ParseOverrides(format!(
                    "Invalid generator config format: '{}'. Expected format: <generator>.<key>=<value>",
                    config_str
                )));
            }

            let generator_str = key_parts[0];
            let key = key_parts[1].to_string();

            // Parse generator
            let generator = GeneratorType::from_str(generator_str).map_err(|e| {
                ConfigError::ParseOverrides(format!(
                    "Invalid generator name '{}': {}",
                    generator_str, e
                ))
            })?;

            // Parse value as TOML value
            let toml_value = Self::parse_toml_value(value_str);

            // Add to overrides
            overrides
                .entry(generator)
                .or_default()
                .insert(key, toml_value);
        }

        Ok(overrides)
    }

    /// Parse a string value into an appropriate TOML value
    fn parse_toml_value(value: &str) -> toml::Value {
        // Try to parse as boolean
        if let Ok(b) = value.parse::<bool>() {
            return toml::Value::Boolean(b);
        }
        // Try to parse as integer
        if let Ok(i) = value.parse::<i64>() {
            return toml::Value::Integer(i);
        }
        // Try to parse as float
        if let Ok(f) = value.parse::<f64>() {
            return toml::Value::Float(f);
        }
        // Default to string
        toml::Value::String(value.to_string())
    }
}
