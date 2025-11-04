//! OpenAPI Code Generator CLI

use clap::Parser;
use std::process;
use tracing::{error, info};

mod language;

use openapi_nexus_config::{CliArgs, Commands, ConfigLoader, ConfigMerger};
use openapi_nexus_core::{OpenApiCodeGenerator, error::Error};
use openapi_nexus_typescript::TsLangGenerator;

fn init_logging(verbose: bool) {
    let default_level = if verbose { "debug" } else { "info" };
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_level));
    tracing_subscriber::fmt().with_env_filter(env_filter).init();
}

fn main() {
    let cli_args = CliArgs::parse();

    // Get verbose flag from CLI args only (before loading configs)
    // This allows us to initialize logging early
    let verbose = match &cli_args.command {
        Commands::Generate { verbose, .. } => *verbose,
    };
    init_logging(verbose);

    // Load config file (if exists)
    let config_file = match &cli_args.command {
        Commands::Generate {
            config: Some(path), ..
        } => match ConfigLoader::load_from_file(path) {
            Ok(config) => Some(config),
            Err(e) => {
                error!("Failed to load config file from {:?}: {}", path, e);
                process::exit(1);
            }
        },
        Commands::Generate { .. } => ConfigLoader::discover_config_file()
            .and_then(|path| ConfigLoader::load_from_file(&path).ok()),
    };

    // Merge configurations with precedence: CLI > Env > Config File > Defaults
    let resolved_config = match ConfigMerger::merge(config_file.as_ref(), &cli_args) {
        Ok(config) => config,
        Err(e) => {
            error!("Configuration error: {}", e);
            process::exit(1);
        }
    };

    match cli_args.command {
        Commands::Generate { .. } => {
            info!("Starting code generation");
            info!("Input: {}", resolved_config.global.input());
            info!("Output: {}", resolved_config.global.output());
            info!("Language: {}", resolved_config.global.language());

            let mut generator = OpenApiCodeGenerator::new();

            // Register generators based on selected language
            match resolved_config.global.language() {
                "typescript" | "ts" => {
                    for alias in ["typescript", "ts"] {
                        let ts_generator = TsLangGenerator::new(resolved_config.typescript.clone());
                        if let Err(e) = generator.register_language_generator(alias, ts_generator) {
                            error!("Failed to register {} generator: {}", alias, e);
                            process::exit(1);
                        }
                    }
                }
                language => {
                    let err = Error::UnsupportedLanguage {
                        language: language.to_string(),
                    };
                    error!("{}", err);
                    process::exit(1);
                }
            }

            // Convert to Vec<String> for the generate_from_file method
            let languages = vec![resolved_config.global.language().to_string()];
            if let Err(e) = generator.generate_from_file(
                resolved_config.global.input(),
                resolved_config.global.output(),
                &languages,
            ) {
                error!("Code generation failed: {}", e);
                process::exit(1);
            }

            info!("Code generation completed successfully");
        }
    }
}
