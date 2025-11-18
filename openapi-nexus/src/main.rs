//! OpenAPI Code Generator CLI

use clap::Parser;
use std::process;
use tracing::{error, info};

use openapi_nexus_config::{CliArgs, Commands, ConfigLoader, ConfigMerger};
use openapi_nexus_core::OpenApiCodeGenerator;
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
            let generator_framework = match resolved_config.global.generator() {
                Some(g) => g,
                None => {
                    error!(
                        "Generator is required. Please specify a generator using --generator (e.g., --generator typescript-fetch)"
                    );
                    process::exit(1);
                }
            };
            let language = match resolved_config.global.language() {
                Some(l) => l,
                None => {
                    error!("Language could not be determined from generator");
                    process::exit(1);
                }
            };
            info!("Starting code generation");
            info!("Input: {}", resolved_config.global.input());
            info!("Output: {}", resolved_config.global.output());
            info!("Generator: {}", generator_framework);

            let mut generator = OpenApiCodeGenerator::new();

            // Register generators based on selected generator framework
            match generator_framework {
                openapi_nexus_common::Generator::TypeScriptFetch => {
                    let ts_generator = TsLangGenerator::new(resolved_config.typescript.clone());
                    if let Err(e) = generator.register_language_generator(language, ts_generator) {
                        error!(
                            "Failed to register {} generator: {}",
                            generator_framework, e
                        );
                        process::exit(1);
                    }
                }
            }

            if let Err(e) = generator.generate_from_file(
                resolved_config.global.input(),
                resolved_config.global.output(),
                language,
            ) {
                error!("Code generation failed: {}", e);
                process::exit(1);
            }

            info!("Code generation completed successfully");
        }
    }
}
