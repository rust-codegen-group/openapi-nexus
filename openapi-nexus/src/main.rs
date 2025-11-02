//! OpenAPI Code Generator CLI

use clap::{Parser, Subcommand};
use tracing::{Level, info};

use openapi_nexus_core::OpenApiCodeGenerator;
use openapi_nexus_typescript::TsLangGenerator;

#[derive(Parser)]
#[command(name = "openapi-nexus")]
#[command(about = "Generate code from OpenAPI 3.1 specifications")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate code from an OpenAPI specification
    Generate {
        /// Path to the OpenAPI specification file
        #[arg(short, long)]
        input: String,

        /// Output directory for generated code
        #[arg(short, long, default_value = "generated")]
        output: String,

        /// Languages to generate code for
        #[arg(short, long, default_values = ["typescript"])]
        languages: Vec<String>,

        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
    },
    /// Validate an OpenAPI specification
    Validate {
        /// Path to the OpenAPI specification file
        #[arg(short, long)]
        input: String,

        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Initialize logging
    let log_level = if cli.command.is_verbose() {
        Level::DEBUG
    } else {
        Level::INFO
    };

    tracing_subscriber::fmt().with_max_level(log_level).init();

    match cli.command {
        Commands::Generate {
            input,
            output,
            languages,
            ..
        } => {
            info!("Starting code generation");
            info!("Input: {}", input);
            info!("Output: {}", output);
            info!("Languages: {:?}", languages);

            let mut generator = OpenApiCodeGenerator::new();

            // Register TypeScript generator with default configuration
            let ts_config = openapi_nexus_typescript::config::TsConfig::default();
            let ts_generator_typescript = TsLangGenerator::new(ts_config.clone());
            let ts_generator_ts = TsLangGenerator::new(ts_config);
            generator.register_language_generator("typescript", ts_generator_typescript)?;
            generator.register_language_generator("ts", ts_generator_ts)?;

            generator.generate_from_file(&input, &output, &languages)?;

            info!("Code generation completed successfully");
        }
        Commands::Validate { input, .. } => {
            info!("Validating OpenAPI specification: {}", input);

            // TODO: Implement validation command
            println!("Validation not yet implemented");
        }
    }

    Ok(())
}

trait Verbose {
    fn is_verbose(&self) -> bool;
}

impl Verbose for Commands {
    fn is_verbose(&self) -> bool {
        match self {
            Commands::Generate { verbose, .. } => *verbose,
            Commands::Validate { verbose, .. } => *verbose,
        }
    }
}
