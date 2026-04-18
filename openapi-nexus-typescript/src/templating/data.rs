//! Data structures for template generation

pub mod api_class_data;
pub mod api_class_signature_data;
pub mod api_import_specifier_data;
pub mod api_import_statement_data;
pub mod api_import_statements;
pub mod api_method_data;
pub mod api_operation_data;
pub mod common_file_header_data;
pub mod project_index_data;
pub mod runtime_runtime_data;

pub use api_class_data::ApiClassData;
pub use api_class_signature_data::ApiClassSignature;
pub use api_import_specifier_data::ApiImportSpecifier;
pub use api_import_statement_data::ApiImportStatement;
pub use api_import_statements::ApiImportStatements;
pub use api_method_data::ApiMethodData;
pub use api_operation_data::{
    ApiOperationData, HttpParamData, MethodTemplateData, ResponseTemplateData,
};
pub use common_file_header_data::CommonFileHeaderData;
pub use project_index_data::ProjectIndexData;
pub use runtime_runtime_data::RuntimeRuntimeData;
