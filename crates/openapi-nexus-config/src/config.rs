//! Configuration structures for OpenAPI Nexus

use std::collections::HashMap;

use crate::cli::{CliArgs, Commands};
use crate::config_file::ConfigFile;
use crate::errors::ConfigError;
use crate::global_config::GlobalConfig;
use crate::loader::ConfigLoader;
use openapi_nexus_common::GeneratorType;

/// Unified configuration structure that works for both file and CLI configs
#[derive(Debug, Clone)]
pub struct Config {
    /// Input file path (CLI only)
    pub input: String,
    /// Global settings
    pub global: GlobalConfig,
    /// Generator-specific configurations stored as TOML tables
    pub generators: HashMap<GeneratorType, toml::value::Table>,
}

impl Config {
    /// Load configuration from CLI arguments and file system
    ///
    /// This method handles:
    /// - Loading config file from specified path or auto-discovery
    /// - Parsing generator config overrides from CLI
    /// - Merging all configurations with proper precedence
    ///
    /// Precedence: CLI overrides > CLI global args > Env vars > Config file > Defaults
    pub fn load(cli_args: &CliArgs) -> Result<Self, ConfigError> {
        // Load config file (if exists)
        let config_file = match &cli_args.command {
            Commands::Generate {
                config: Some(path), ..
            } => Some(ConfigLoader::load_from_file(path)?),
            Commands::Generate { .. } => ConfigLoader::discover_config_file()
                .and_then(|path| ConfigLoader::load_from_file(&path).ok()),
        };

        // Merge configurations
        Self::merge(config_file.as_ref(), cli_args)
    }

    /// Merge CLI arguments with config file, applying precedence rules
    /// Precedence: CLI overrides > Config file > Defaults
    fn merge(config_file: Option<&ConfigFile>, cli_args: &CliArgs) -> Result<Self, ConfigError> {
        let mut config = Config {
            input: String::new(),
            global: GlobalConfig::default(),
            generators: HashMap::new(),
        };

        // Extract generator configs from config file
        if let Some(config_file) = config_file {
            config.merge_file(config_file);
        }

        // Extract CLI configs
        let (input, cli_global) = match &cli_args.command {
            Commands::Generate { input, global, .. } => (input, global),
        };
        if input.is_empty() {
            return Err(ConfigError::Validation(
                "Input is required and cannot be empty".to_string(),
            ));
        }
        let generator_overrides = cli_args.command.parse_generator_overrides()?;

        config.input = input.clone();
        config.merge_global(cli_global);
        config.merge_generator_configs(&generator_overrides);
        Ok(config)
    }

    fn merge_file(&mut self, config_file: &ConfigFile) {
        self.merge_global(&config_file.global);
        self.merge_generator_configs(&config_file.generators);
    }

    fn merge_global(&mut self, global: &GlobalConfig) {
        if global.output.is_some() {
            self.global.output = global.output.clone();
        }
        if global.generators.is_some() {
            self.global.generators = global.generators.clone();
        }
    }

    fn merge_generator_configs(
        &mut self,
        generator_configs: &HashMap<GeneratorType, toml::value::Table>,
    ) {
        for (generator, table_rhs) in generator_configs {
            if let Some(table_lhs) = self.generators.get_mut(generator) {
                for (key, value) in table_rhs.iter() {
                    table_lhs.insert(key.clone(), value.clone());
                }
            } else {
                self.generators.insert(*generator, table_rhs.clone());
            }
        }
    }
}
