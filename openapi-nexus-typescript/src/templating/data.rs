//! Data structures for template generation

pub mod api_file_data;
pub mod api_method_body_data;
pub mod interface_data;
pub mod model_file_data;
pub mod project_file_data;

pub use api_file_data::ApiFileData;
pub use api_method_body_data::ApiMethodBodyData;
pub use interface_data::InterfaceData;
pub use model_file_data::ModelFileData;
pub use project_file_data::ProjectFileData;
