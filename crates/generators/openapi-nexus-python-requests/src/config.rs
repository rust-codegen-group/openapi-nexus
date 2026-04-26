//! Python requests generator-specific configuration.

use serde::{Deserialize, Serialize};
use tracing::error;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PythonRequestsConfig {
    #[serde(default)]
    pub package_name: Option<String>,
}

impl From<toml::value::Table> for PythonRequestsConfig {
    fn from(value: toml::value::Table) -> Self {
        use serde::Deserialize;
        match PythonRequestsConfig::deserialize(value) {
            Ok(config) => config,
            Err(e) => {
                error!(
                    "Failed to parse Python requests config: {}. Using default configuration.",
                    e
                );
                Self::default()
            }
        }
    }
}
