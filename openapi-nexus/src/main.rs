//! OpenAPI Code Generator CLI

use clap::Parser;
use tracing::info;

mod language;

use openapi_nexus_config::{CliArgs, Commands, ConfigLoader, ConfigMerger};
use openapi_nexus_core::OpenApiCodeGenerator;
use openapi_nexus_typescript::TsLangGenerator;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli_args = CliArgs::parse();

    // Load config file (if exists)
    let config_file = match &cli_args.command {
        Commands::Generate {
            config: Some(path), ..
        } => Some(ConfigLoader::load_from_file(path)?),
        Commands::Generate { .. } => ConfigLoader::discover_config_file()
            .and_then(|path| ConfigLoader::load_from_file(&path).ok()),
    };

    // Merge configurations with precedence: CLI > Env > Config File > Defaults
    let resolved_config = ConfigMerger::merge(config_file.as_ref(), &cli_args)?;

    // Initialize logging
    let log_level = if resolved_config.global.verbose() {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };

    tracing_subscriber::fmt().with_max_level(log_level).init();

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
                        generator.register_language_generator(alias, ts_generator)?;
                    }
                }
                _ => {
                    return Err(format!(
                        "Unsupported language: {}",
                        resolved_config.global.language()
                    )
                    .into());
                }
            }

            // Convert to Vec<String> for the generate_from_file method
            let languages = vec![resolved_config.global.language().to_string()];
            generator.generate_from_file(
                resolved_config.global.input(),
                resolved_config.global.output(),
                &languages,
            )?;

            info!("Code generation completed successfully");
        }
    }

    Ok(())
}
