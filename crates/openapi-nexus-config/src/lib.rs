//! Configuration system for OpenAPI Nexus
//!
//! Provides layered configuration support with CLI arguments, environment variables,
//! and TOML config files with proper precedence ordering.

pub mod cli;
pub mod config;
pub mod config_file;
pub mod errors;
pub mod global_config;
pub mod loader;

pub use cli::{CliArgs, Commands};
pub use config::Config;
pub use config_file::ConfigFile;
pub use errors::ConfigError;
pub use global_config::GlobalConfig;
pub use loader::ConfigLoader;
