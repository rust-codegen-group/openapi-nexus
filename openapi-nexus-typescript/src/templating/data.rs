//! Data structures for template generation

pub mod api_operation_data;
pub mod common_file_header_data;
pub mod model_enum_data;
pub mod model_interface_data;
pub mod model_type_alias_data;
pub mod project_index_data;
pub mod runtime_runtime_data;

pub use api_operation_data::{ApiOperationData, HttpParamData, MethodTemplateData};
pub use common_file_header_data::CommonFileHeaderData;
pub use model_enum_data::ModelEnumData;
pub use model_interface_data::{ModelInterfaceData, PropertyMetadata};
pub use model_type_alias_data::ModelTypeAliasData;
pub use project_index_data::ProjectIndexData;
pub use runtime_runtime_data::RuntimeRuntimeData;
