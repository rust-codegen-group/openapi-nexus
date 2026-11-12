//! Go HTTP generator-specific configuration

use serde::{Deserialize, Serialize};
use tracing::error;

/// Go HTTP generator-specific configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GoHttpConfig {
    /// Go module path (e.g., "github.com/example/sdk")
    #[serde(default)]
    pub module_path: Option<String>,
    /// Compose object members by default, or retain every allOf as raw JSON.
    #[serde(default)]
    pub all_of_strategy: AllOfStrategy,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AllOfStrategy {
    #[default]
    Composed,
    Raw,
}

impl From<toml::value::Table> for GoHttpConfig {
    fn from(value: toml::value::Table) -> Self {
        use serde::Deserialize;
        match GoHttpConfig::deserialize(value) {
            Ok(config) => config,
            Err(e) => {
                error!(
                    "Failed to parse Go HTTP config: {}. Using default configuration.",
                    e
                );
                Self::default()
            }
        }
    }
}
