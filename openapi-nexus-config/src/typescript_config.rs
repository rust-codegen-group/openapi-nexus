//! TypeScript-specific configuration

use std::fmt;
use std::str::FromStr;

use clap::Args;
use serde::{Deserialize, Serialize};

use openapi_nexus_core::NamingConvention;

/// TypeScript module systems
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TypeScriptModule {
    CommonJS,
    ESNext,
    ES2020,
    ES2022,
}

impl FromStr for TypeScriptModule {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "commonjs" | "cjs" => Ok(Self::CommonJS),
            "esnext" => Ok(Self::ESNext),
            "es2020" => Ok(Self::ES2020),
            "es2022" => Ok(Self::ES2022),
            _ => Err(format!(
                "Invalid TypeScript module: '{}'. Expected one of: commonjs, esnext, es2020, es2022",
                s
            )),
        }
    }
}

impl fmt::Display for TypeScriptModule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeScriptModule::CommonJS => write!(f, "commonjs"),
            TypeScriptModule::ESNext => write!(f, "esnext"),
            TypeScriptModule::ES2020 => write!(f, "es2020"),
            TypeScriptModule::ES2022 => write!(f, "es2022"),
        }
    }
}

/// TypeScript-specific configuration (supports CLI args, env vars, and config files)
#[derive(Debug, Clone, Args, Serialize, Deserialize)]
#[command(next_help_heading = "TypeScript Options")]
pub struct TypeScriptConfig {
    /// File naming convention (camelCase, kebab-case, snake_case, PascalCase)
    #[arg(long = "ts-file-naming-convention", env = "OPENAPI_NEXUS_TS_FILE_NAMING_CONVENTION", default_value_t = default_file_naming_convention())]
    #[serde(default = "default_file_naming_convention")]
    pub file_naming_convention: NamingConvention,

    /// NPM package scope/prefix (e.g., "@myorg")
    #[arg(long = "ts-package-scope", env = "OPENAPI_NEXUS_TS_PACKAGE_SCOPE")]
    #[serde(default)]
    pub package_scope: Option<String>,

    /// Override package name (defaults to OpenAPI title in kebab-case)
    #[arg(long = "ts-package-name", env = "OPENAPI_NEXUS_TS_PACKAGE_NAME")]
    #[serde(default)]
    pub package_name: Option<String>,

    /// Whether to generate npm package files
    #[arg(long = "ts-generate-package", env = "OPENAPI_NEXUS_TS_GENERATE_PACKAGE", default_value_t = default_generate_package())]
    #[serde(default = "default_generate_package")]
    pub generate_package: bool,

    /// TypeScript compiler target
    #[arg(long = "ts-target", env = "OPENAPI_NEXUS_TS_TARGET", default_value_t = default_typescript_target())]
    #[serde(default = "default_typescript_target")]
    pub ts_target: String,

    /// TypeScript module system (commonjs, esnext, es2020, es2022)
    #[arg(long = "ts-module", env = "OPENAPI_NEXUS_TS_MODULE", value_parser = TypeScriptModule::from_str, default_value_t = default_typescript_module())]
    #[serde(default = "default_typescript_module")]
    pub ts_module: TypeScriptModule,

    /// TypeScript compiler lib array (e.g., "ES2020,DOM" or ["ES2020", "DOM"] in TOML)
    #[arg(long = "ts-lib", env = "OPENAPI_NEXUS_TS_LIB", value_delimiter = ',')]
    #[serde(default, deserialize_with = "deserialize_string_vec")]
    pub ts_lib: Option<Vec<String>>,

    /// Whether to generate ESM configuration
    #[arg(long = "ts-generate-esm-config", env = "OPENAPI_NEXUS_TS_GENERATE_ESM_CONFIG", default_value_t = default_generate_esm_config())]
    #[serde(default = "default_generate_esm_config")]
    pub generate_esm_config: bool,

    /// Whether to include build scripts in package.json
    #[arg(long = "ts-include-build-scripts", env = "OPENAPI_NEXUS_TS_INCLUDE_BUILD_SCRIPTS", default_value_t = default_include_build_scripts())]
    #[serde(default = "default_include_build_scripts")]
    pub include_build_scripts: bool,
}

fn default_file_naming_convention() -> NamingConvention {
    NamingConvention::PascalCase
}

fn default_generate_package() -> bool {
    true
}

fn default_typescript_target() -> String {
    "es6".to_string()
}

fn default_typescript_module() -> TypeScriptModule {
    TypeScriptModule::CommonJS
}

fn default_generate_esm_config() -> bool {
    true
}

fn default_include_build_scripts() -> bool {
    true
}

/// Helper to deserialize string vec from TOML array or comma-separated string
fn deserialize_string_vec<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringVec {
        Array(Vec<String>),
        String(String),
    }

    match StringVec::deserialize(deserializer)? {
        StringVec::Array(vec) => Ok(Some(vec)),
        StringVec::String(s) => {
            if s.is_empty() {
                Ok(None)
            } else {
                Ok(Some(s.split(',').map(|s| s.trim().to_string()).collect()))
            }
        }
    }
}

impl Default for TypeScriptConfig {
    fn default() -> Self {
        Self {
            file_naming_convention: default_file_naming_convention(),
            package_scope: None,
            package_name: None,
            generate_package: default_generate_package(),
            ts_target: default_typescript_target(),
            ts_module: default_typescript_module(),
            ts_lib: None,
            generate_esm_config: default_generate_esm_config(),
            include_build_scripts: default_include_build_scripts(),
        }
    }
}
