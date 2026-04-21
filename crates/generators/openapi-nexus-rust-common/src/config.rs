//! Base configuration shared across all Rust generators.

use serde::{Deserialize, Serialize};
use tracing::error;

/// Base configuration for Rust generators.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RustGeneratorConfig {
    #[serde(default)]
    pub crate_name: Option<String>,
}

impl From<toml::value::Table> for RustGeneratorConfig {
    fn from(value: toml::value::Table) -> Self {
        use serde::Deserialize;
        match RustGeneratorConfig::deserialize(value) {
            Ok(config) => config,
            Err(e) => {
                error!(
                    "Failed to parse Rust generator config: {}. Using default configuration.",
                    e
                );
                Self::default()
            }
        }
    }
}
