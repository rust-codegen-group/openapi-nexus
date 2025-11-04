//! Core orchestration for OpenAPI code generation

pub mod data;
pub mod error;
pub mod generator;
pub mod generator_registry;
pub mod naming_convention;
pub mod openapi_code_generator;
pub mod serde;
pub mod traits;

// Re-export the main struct for convenience
pub use generator_registry::GeneratorRegistry;
pub use naming_convention::NamingConvention;
pub use openapi_code_generator::OpenApiCodeGenerator;
