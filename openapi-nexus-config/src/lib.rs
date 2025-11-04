//! Configuration system for OpenAPI Nexus
//!
//! Provides layered configuration support with CLI arguments, environment variables,
//! and TOML config files with proper precedence ordering.

pub mod cli;
pub mod config;
pub mod global_config;
pub mod language;
pub mod loader;
pub mod merger;
pub mod typescript_config;

pub use cli::{CliArgs, Commands};
pub use config::{ConfigFile, ResolvedConfig};
pub use global_config::GlobalConfig;
pub use language::Language;
pub use loader::{ConfigLoader, LoadError};
pub use merger::{ConfigMerger, MergeError};
pub use typescript_config::{TypeScriptConfig, TypeScriptModule};
