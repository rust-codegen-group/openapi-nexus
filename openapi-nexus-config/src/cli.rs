//! Command-line argument definitions

use clap::Parser;

use crate::global_config::GlobalConfig;
use crate::typescript_config::TypeScriptConfig;

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
        /// Path to configuration file (overrides auto-discovery)
        #[arg(long, env = "OPENAPI_NEXUS_CONFIG")]
        config: Option<String>,

        /// Global configuration options
        #[command(flatten)]
        global: GlobalConfig,

        /// TypeScript configuration options
        #[command(flatten)]
        typescript: TypeScriptConfig,
    },
}
