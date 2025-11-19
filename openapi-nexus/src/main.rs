//! OpenAPI Code Generator CLI
use std::process;

use clap::Parser;
use tracing::{error, info};

use openapi_nexus::OpenApiCodeGenerator;
use openapi_nexus_config::{CliArgs, Commands, Config};

fn init_logging(verbose: bool) {
    let default_level = if verbose { "debug" } else { "info" };
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_level));
    tracing_subscriber::fmt().with_env_filter(env_filter).init();
}

fn main() -> process::ExitCode {
    let cli_args = CliArgs::parse();

    // Get verbose flag from CLI args only (before loading configs)
    // This allows us to initialize logging early
    let verbose = match &cli_args.command {
        Commands::Generate { verbose, .. } => *verbose,
    };
    init_logging(verbose);

    // Load and merge configurations
    let config = match Config::load(&cli_args) {
        Ok(config) => config,
        Err(e) => {
            error!("{}", e);
            return process::ExitCode::FAILURE;
        }
    };

    match cli_args.command {
        Commands::Generate { .. } => {
            let generators = config.global.generators.clone().unwrap_or_default();
            if generators.is_empty() {
                info!("No generators specified. Nothing to do.");
                return process::ExitCode::SUCCESS;
            }

            info!("Starting code generation");
            info!("Input: {}", config.input);

            let code_generator = OpenApiCodeGenerator::new(&config);
            code_generator.generate(&config);

            info!("Code generation completed");
        }
    };

    process::ExitCode::SUCCESS
}
