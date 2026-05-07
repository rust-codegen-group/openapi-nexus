//! TypeScript Fetch generator-specific configuration

use serde::{Deserialize, Serialize};
use tracing::error;

use super::module::TypeScriptModule;
use crate::codegen::NamingConvention;

/// TypeScript Fetch generator-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeScriptFetchConfig {
    /// File naming convention (camelCase, kebab-case, snake_case, PascalCase)
    #[serde(default = "default_file_naming_convention")]
    pub file_naming_convention: NamingConvention,

    /// NPM package scope/prefix (e.g., "@myorg")
    #[serde(default)]
    pub package_scope: Option<String>,

    /// Override package name (defaults to OpenAPI title in kebab-case)
    #[serde(default)]
    pub package_name: Option<String>,

    /// Whether to generate npm package files
    #[serde(default = "default_generate_package")]
    pub generate_package: bool,

    /// TypeScript compiler target
    #[serde(default = "default_typescript_target")]
    pub ts_target: String,

    /// TypeScript module system (commonjs, esnext, es2020, es2022)
    #[serde(default = "default_typescript_module")]
    pub ts_module: TypeScriptModule,

    /// TypeScript compiler lib array (e.g., "ES2020,DOM" or ["ES2020", "DOM"] in TOML)
    #[serde(
        default = "default_typescript_lib",
        deserialize_with = "deserialize_string_vec"
    )]
    pub ts_lib: Vec<String>,

    /// Whether to generate ESM configuration
    #[serde(default = "default_generate_esm_config")]
    pub generate_esm_config: bool,

    /// Whether to include build scripts in package.json
    #[serde(default = "default_include_build_scripts")]
    pub include_build_scripts: bool,

    /// Emit companion const objects alongside enum type aliases.
    #[serde(default)]
    pub emit_enum_constants: bool,

    /// Emit `is*` type guard functions alongside tagged union type aliases.
    #[serde(default)]
    pub emit_type_guards: bool,
}

fn default_file_naming_convention() -> NamingConvention {
    NamingConvention::PascalCase
}

fn default_generate_package() -> bool {
    true
}

fn default_typescript_target() -> String {
    "ES2020".to_string()
}

fn default_typescript_module() -> TypeScriptModule {
    TypeScriptModule::ES2020
}

fn default_generate_esm_config() -> bool {
    true
}

fn default_include_build_scripts() -> bool {
    true
}

fn default_typescript_lib() -> Vec<String> {
    vec!["ES2020".to_string(), "DOM".to_string()]
}

/// Helper to deserialize string vec from TOML array or comma-separated string
/// Never fails - returns default value on error
fn deserialize_string_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringVec {
        Array(Vec<String>),
        String(String),
    }

    match StringVec::deserialize(deserializer) {
        Ok(StringVec::Array(vec)) => Ok(vec),
        Ok(StringVec::String(s)) => {
            if s.is_empty() {
                Ok(default_typescript_lib())
            } else {
                Ok(s.split(',').map(|s| s.trim().to_string()).collect())
            }
        }
        Err(_) => {
            // On deserialization error, return default value instead of failing
            error!("Failed to deserialize Vec<String> from toml: returning default value");
            Ok(default_typescript_lib())
        }
    }
}

impl Default for TypeScriptFetchConfig {
    fn default() -> Self {
        Self {
            file_naming_convention: default_file_naming_convention(),
            package_scope: None,
            package_name: None,
            generate_package: default_generate_package(),
            ts_target: default_typescript_target(),
            ts_module: default_typescript_module(),
            ts_lib: default_typescript_lib(),
            generate_esm_config: default_generate_esm_config(),
            include_build_scripts: default_include_build_scripts(),
            emit_enum_constants: false,
            emit_type_guards: false,
        }
    }
}

impl From<toml::value::Table> for TypeScriptFetchConfig {
    fn from(value: toml::value::Table) -> Self {
        use serde::Deserialize;
        match TypeScriptFetchConfig::deserialize(value) {
            Ok(config) => config,
            Err(e) => {
                error!(
                    "Failed to parse TypeScript Fetch config: {}. Using default configuration.",
                    e
                );
                Self::default()
            }
        }
    }
}
