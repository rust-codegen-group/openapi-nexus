//! Rust reqwest generator-specific configuration

use serde::{Deserialize, Serialize};
use tracing::error;

/// Rust reqwest generator-specific configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RustReqwestConfig {
    /// Crate name for the generated SDK (e.g., "my-api-client")
    #[serde(default)]
    pub crate_name: Option<String>,
}

impl From<toml::value::Table> for RustReqwestConfig {
    fn from(value: toml::value::Table) -> Self {
        use serde::Deserialize;
        match RustReqwestConfig::deserialize(value) {
            Ok(config) => config,
            Err(e) => {
                error!(
                    "Failed to parse Rust reqwest config: {}. Using default configuration.",
                    e
                );
                Self::default()
            }
        }
    }
}
