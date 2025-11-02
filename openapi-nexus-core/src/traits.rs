//! Core traits for language generators

pub mod code_generator;
pub mod emission;
pub mod file_writer;
pub mod openapi_parameter_ext;
pub mod operation_info_ext;
pub mod types;

pub use code_generator::LanguageCodeGenerator;
pub use emission::{EmissionContext, ToRcDocWithContext};
pub use file_writer::{FileCategory, FileInfo, FileWriter};
pub use openapi_parameter_ext::OpenApiParameterExt;
pub use operation_info_ext::OperationInfoExt;
