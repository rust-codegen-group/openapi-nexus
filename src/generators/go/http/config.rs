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
    /// Codec policy for generalized inferred internally tagged unions.
    #[serde(default)]
    pub tagged_union_strategy: TaggedUnionStrategy,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaggedUnionStrategy {
    Dynamic,
    Raw,
    #[default]
    FirstMatch,
    Strict,
}

impl From<TaggedUnionStrategy> for crate::generators::union_wire::Strategy {
    fn from(value: TaggedUnionStrategy) -> Self {
        match value {
            // Dynamic skips adapter adoption; its shared index is never queried.
            TaggedUnionStrategy::Dynamic | TaggedUnionStrategy::FirstMatch => Self::FirstMatch,
            TaggedUnionStrategy::Strict => Self::Strict,
            TaggedUnionStrategy::Raw => Self::Raw,
        }
    }
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
