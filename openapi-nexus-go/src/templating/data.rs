//! Data structures for template generation

pub mod api_operation_data;
pub mod common_file_header_data;
pub mod main_sdk_data;
pub mod model_struct_data;
pub mod model_type_alias_data;
pub mod operations_data;

pub use api_operation_data::{ApiOperationData, GoApiMethodData, GoParameterInfo};
pub use common_file_header_data::CommonFileHeaderData;
pub use main_sdk_data::{MainSdkData, SdkOption, SubClientInfo};
pub use model_struct_data::ModelStructData;
pub use model_type_alias_data::ModelTypeAliasData;
pub use operations_data::{OperationResponse, OperationsData};
