//! Plugin system traits for OpenAPI code generation

use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::collections::HashMap;
use utoipa::openapi::OpenApi;

/// Error type for plugin operations
#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum PluginError {
    #[snafu(display("Plugin error: {}", message))]
    Generic { message: String },

    #[snafu(display("Plugin not found: {}", name))]
    PluginNotFound { name: String },

    #[snafu(display("Plugin initialization failed: {}", message))]
    InitializationFailed { message: String },

    #[snafu(display("Plugin execution failed: {}", message))]
    ExecutionFailed { message: String },
}

/// Plugin metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub license: Option<String>,
    pub capabilities: Vec<PluginCapability>,
}

/// Plugin capabilities
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PluginCapability {
    LanguageGenerator,
    TransformPass,
    Emitter,
    Validator,
    Formatter,
}

/// Plugin configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    pub name: String,
    pub enabled: bool,
    pub settings: HashMap<String, serde_json::Value>,
}

/// Base trait for all plugins
pub trait Plugin: Send + Sync {
    /// Get plugin metadata
    fn metadata(&self) -> &PluginMetadata;

    /// Initialize the plugin with configuration
    fn initialize(&mut self, config: &PluginConfig) -> Result<(), PluginError>;

    /// Check if the plugin is compatible with the given OpenAPI version
    fn is_compatible(&self, openapi: &OpenApi) -> bool;
}

/// Trait for language generators
pub trait LanguageGenerator: Plugin {
    /// Get the supported language name
    fn language(&self) -> &str;

    /// Get the file extension for generated files
    fn file_extension(&self) -> &str;

    /// Generate code for the given OpenAPI specification
    fn generate(&self, openapi: &OpenApi) -> Result<Vec<GeneratedFile>, PluginError>;

    /// Get the generator's configuration schema
    fn config_schema(&self) -> Option<serde_json::Value>;
}

/// Trait for transformation passes
pub trait TransformPass: Plugin {
    /// Get the pass name
    fn name(&self) -> &str;

    /// Get the pass description
    fn description(&self) -> &str;

    /// Get the pass priority (lower numbers run first)
    fn priority(&self) -> i32;

    /// Apply the transformation to the OpenAPI specification
    fn transform(&self, openapi: &mut OpenApi) -> Result<(), PluginError>;

    /// Check if the pass should run for the given OpenAPI spec
    fn should_run(&self, openapi: &OpenApi) -> bool;
}

/// Trait for code emitters
pub trait Emitter: Plugin {
    /// Get the emitter name
    fn name(&self) -> &str;

    /// Get the emitter description
    fn description(&self) -> &str;

    /// Emit the generated files
    fn emit(&self, files: &[GeneratedFile], output_dir: &str) -> Result<(), PluginError>;

    /// Get the emitter's configuration schema
    fn config_schema(&self) -> Option<serde_json::Value>;
}

/// Trait for validators
pub trait Validator: Plugin {
    /// Get the validator name
    fn name(&self) -> &str;

    /// Get the validator description
    fn description(&self) -> &str;

    /// Validate the OpenAPI specification
    fn validate(&self, openapi: &OpenApi) -> Result<ValidationResult, PluginError>;
}

/// Trait for formatters
pub trait Formatter: Plugin {
    /// Get the formatter name
    fn name(&self) -> &str;

    /// Get the formatter description
    fn description(&self) -> &str;

    /// Format the generated code
    fn format(&self, content: &str, language: &str) -> Result<String, PluginError>;
}

/// Represents a generated file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedFile {
    pub path: String,
    pub content: String,
    pub language: String,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
}

/// Validation error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub message: String,
    pub path: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

/// Validation warning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationWarning {
    pub message: String,
    pub path: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

/// Plugin registry for managing plugins
pub trait PluginRegistry: Send + Sync {
    /// Register a plugin
    fn register_plugin(&mut self, plugin: Box<dyn Plugin>) -> Result<(), PluginError>;

    /// Get a plugin by name
    fn get_plugin(&self, name: &str) -> Option<&dyn Plugin>;

    /// Get all registered plugins
    fn list_plugins(&self) -> Vec<&dyn Plugin>;

    /// Get plugins by capability
    fn get_plugins_by_capability(&self, capability: &PluginCapability) -> Vec<&dyn Plugin>;

    /// Unregister a plugin
    fn unregister_plugin(&mut self, name: &str) -> Result<(), PluginError>;
}
