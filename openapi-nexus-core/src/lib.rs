//! Core orchestration for OpenAPI code generation

pub mod data;
pub mod error;
pub mod naming_convention;
pub mod serde;
pub mod tagged_enum_pattern;
pub mod traits;

pub use naming_convention::NamingConvention;
pub use tagged_enum_pattern::TaggedEnumPattern;
pub use traits::CombinedGenerator;
