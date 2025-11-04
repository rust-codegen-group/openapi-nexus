//! Configuration structures for OpenAPI Nexus

use serde::Deserialize;

use crate::global_config::GlobalConfig;
use crate::typescript_config::TypeScriptConfig;

/// Configuration file structure
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ConfigFile {
    /// Global settings
    #[serde(default)]
    pub global: GlobalConfig,
    /// TypeScript-specific settings
    #[serde(default)]
    pub typescript: TypeScriptConfig,
}

/// Fully resolved configuration with all defaults applied
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    /// Global settings
    pub global: GlobalConfig,
    /// TypeScript settings (if language is TypeScript)
    pub typescript: TypeScriptConfig,
}
