//! Python httpx generator-specific configuration.

use serde::{Deserialize, Serialize};
use tracing::error;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PythonHttpxConfig {
    #[serde(default)]
    pub package_name: Option<String>,
}

impl From<toml::value::Table> for PythonHttpxConfig {
    fn from(value: toml::value::Table) -> Self {
        use serde::Deserialize;
        match PythonHttpxConfig::deserialize(value) {
            Ok(config) => config,
            Err(e) => {
                error!(
                    "Failed to parse Python httpx config: {}. Using default configuration.",
                    e
                );
                Self::default()
            }
        }
    }
}
