pub mod codegen;
pub mod config;
pub mod error;
pub mod generators;
pub mod ir;
pub mod parser;
pub mod spec;

pub use codegen::traits::{
    CodeGenerator, CombinedGenerator, FileCategory, FileInfo, FileWriter, WriteError,
};
pub use codegen::{GeneratorType, Language, NamingConvention};
pub use error::{GenerationFailure, GenerationReport, PipelineError};
pub use generators::{ArtifactKind, GeneratorError};
pub use generators::{GeneratorRegistry, OpenApiCodeGenerator};
pub use ir::types::IrSpec;
pub use parser::ParsedSpec;

#[cfg(any(test, feature = "test-utils"))]
#[doc(hidden)]
pub mod test_utils;
