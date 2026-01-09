//! Error types for TypeScript code generation

use snafu::Snafu;

/// Error type for TypeScript code generation
#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum GeneratorError {
    /// Template rendering error
    #[snafu(display("Failed to render template '{}': {}", template_path, source))]
    TemplateRender {
        template_path: String,
        source: minijinja::Error,
    },

    /// Template not found error
    #[snafu(display("Template '{}' not found: {}", template_path, source))]
    TemplateNotFound {
        template_path: String,
        source: minijinja::Error,
    },

    /// API class generation error
    #[snafu(display("Failed to generate API class '{}': {}", class_name, source))]
    ApiClassGeneration {
        class_name: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// API class generation error for tag
    #[snafu(display("Failed to generate API class for tag '{}': {}", tag, source))]
    ApiClassGenerationForTag {
        tag: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Model interface generation error
    #[snafu(display("Failed to generate interface model '{}': {}", model_name, source))]
    ModelInterfaceGeneration {
        model_name: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Model type alias generation error
    #[snafu(display("Failed to generate type alias model '{}': {}", model_name, source))]
    ModelTypeAliasGeneration {
        model_name: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Model enum generation error
    #[snafu(display("Failed to generate enum model '{}': {}", model_name, source))]
    ModelEnumGeneration {
        model_name: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Runtime template generation error
    #[snafu(display("Failed to render runtime template: {}", source))]
    RuntimeTemplate {
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Index file generation error
    #[snafu(display("Failed to render index file '{}': {}", file_path, source))]
    IndexFileGeneration {
        file_path: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Unsupported HTTP method error
    #[snafu(display(
        "Unsupported HTTP method: {:?}. Only GET, POST, PUT, PATCH, and DELETE are supported.",
        method
    ))]
    UnsupportedHttpMethod { method: http::Method },

    /// Parameter extraction error
    #[snafu(display("Failed to extract parameters: {}", source))]
    ParameterExtraction {
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Return type generation error
    #[snafu(display("Failed to generate return type: {}", source))]
    ReturnTypeGeneration {
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// File I/O error
    #[snafu(display("File I/O error: {}", source))]
    Io { source: std::io::Error },

    /// Config parsing error
    #[snafu(display("Failed to parse config: {}", source))]
    ConfigParse { source: toml::de::Error },

    /// Generic error for cases that don't fit other categories
    #[snafu(display("Generator error: {}", message))]
    Generic { message: String },

    /// Conflicting schema names error
    ///
    /// This error occurs when multiple schema names map to the same PascalCase name,
    /// which would cause TypeScript compilation errors.
    #[snafu(display("{message}"))]
    ConflictingSchemaNames { message: String },
}

impl From<minijinja::Error> for GeneratorError {
    fn from(err: minijinja::Error) -> Self {
        GeneratorError::Generic {
            message: err.to_string(),
        }
    }
}

impl From<std::io::Error> for GeneratorError {
    fn from(err: std::io::Error) -> Self {
        GeneratorError::Io { source: err }
    }
}

impl From<toml::de::Error> for GeneratorError {
    fn from(err: toml::de::Error) -> Self {
        GeneratorError::ConfigParse { source: err }
    }
}

impl From<Box<dyn std::error::Error + Send + Sync>> for GeneratorError {
    fn from(err: Box<dyn std::error::Error + Send + Sync>) -> Self {
        GeneratorError::Generic {
            message: err.to_string(),
        }
    }
}
