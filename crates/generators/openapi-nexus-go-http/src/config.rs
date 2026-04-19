//! Go HTTP generator-specific configuration

use serde::{Deserialize, Serialize};
use tracing::error;

use openapi_nexus_core::NamingConvention;

/// Go HTTP generator-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoHttpConfig {
    /// File naming convention (camelCase, kebab-case, snake_case, PascalCase)
    #[serde(default = "default_file_naming_convention")]
    pub file_naming_convention: NamingConvention,

    /// Go module path (e.g., "github.com/example/sdk")
    #[serde(default)]
    pub module_path: Option<String>,

    /// Package name (defaults to OpenAPI title in lowercase)
    #[serde(default)]
    pub package_name: Option<String>,
}

fn default_file_naming_convention() -> NamingConvention {
    NamingConvention::SnakeCase
}

impl Default for GoHttpConfig {
    fn default() -> Self {
        Self {
            file_naming_convention: default_file_naming_convention(),
            module_path: None,
            package_name: None,
        }
    }
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
