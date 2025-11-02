//! Configuration types for TypeScript code generation

use std::fmt;
use std::str::FromStr;

use clap::Parser;
use openapi_nexus_core::NamingConvention;

/// Maximum line width for pretty printing TypeScript code
pub const MAX_LINE_WIDTH: usize = 80;

/// Unified configuration for TypeScript code generation
#[derive(Debug, Clone, Parser)]
#[command(name = "ts-config")]
#[command(about = "TypeScript code generation configuration")]
pub struct TsConfig {
    // File configuration
    /// Output directory for generated files
    #[arg(long, default_value = "generated")]
    pub output_dir: String,

    /// File naming convention (camelCase, kebab-case, snake_case, PascalCase)
    #[arg(long, default_value = "PascalCase")]
    pub naming_convention: NamingConvention,

    // Emission configuration
    /// Whether to include JSDoc comments
    #[arg(long, default_value_t = true)]
    pub include_documentation: bool,

    /// Whether to use prettier formatting
    #[arg(long, default_value_t = false)]
    pub use_prettier: bool,

    /// Indentation style: "Tabs" or "Spaces(n)" where n is the number of spaces (e.g., "Spaces(2)")
    #[arg(long, default_value = "Spaces(2)")]
    pub indentation: IndentationStyle,

    // Package configuration
    /// Package scope/prefix
    #[arg(long)]
    pub scope: Option<String>,

    /// Whether to generate npm package files
    #[arg(long, default_value_t = true)]
    pub generate_package: bool,

    /// TypeScript compiler target
    #[arg(long, default_value = "es6")]
    pub typescript_target: String,

    /// TypeScript module system (commonjs, esnext, es2020, es2022)
    #[arg(long, default_value = "commonjs")]
    pub typescript_module: TypeScriptModule,

    /// Whether to generate ESM configuration
    #[arg(long, default_value_t = true)]
    pub generate_esm_config: bool,

    /// Whether to include build scripts in package.json
    #[arg(long, default_value_t = false)]
    pub include_build_scripts: bool,
}

impl Default for TsConfig {
    fn default() -> Self {
        Self {
            output_dir: "generated".to_string(),
            naming_convention: NamingConvention::PascalCase,
            include_documentation: true,
            use_prettier: false,
            indentation: IndentationStyle::Spaces(2),
            scope: None,
            generate_package: true,
            typescript_target: "es6".to_string(),
            typescript_module: TypeScriptModule::CommonJS,
            generate_esm_config: true,
            include_build_scripts: false,
        }
    }
}

/// Indentation styles
#[derive(Debug, Clone)]
pub enum IndentationStyle {
    Spaces(usize),
    Tabs,
}

impl FromStr for IndentationStyle {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "Tabs" || s == "tabs" {
            return Ok(Self::Tabs);
        }

        if let Some(count) = s.strip_prefix("Spaces(").and_then(|s| s.strip_suffix(')'))
            && let Ok(n) = count.parse::<usize>() {
                return Ok(Self::Spaces(n));
            }

        // Try parsing just the number
        if let Ok(n) = s.parse::<usize>() {
            return Ok(Self::Spaces(n));
        }

        Err(format!(
            "Invalid indentation style: '{}'. Expected 'Tabs' or 'Spaces(n)' where n is a number",
            s
        ))
    }
}

impl fmt::Display for IndentationStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spaces(n) => write!(f, "Spaces({})", n),
            Self::Tabs => write!(f, "Tabs"),
        }
    }
}

/// TypeScript module systems
#[derive(Debug, Clone)]
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
