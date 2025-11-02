//! Data structures for template generation

pub mod api_method_data;
pub mod header_data;
pub mod model_data;
pub mod operation_info;
pub mod parameter_info;
pub mod readme_data;
pub mod runtime_data;
pub mod template_path_info;

pub use api_method_data::ApiMethodData;
pub use header_data::HeaderData;
pub use model_data::ModelData;
pub use operation_info::OperationInfo;
pub use parameter_info::ParameterInfo;
pub use readme_data::ReadmeData;
pub use runtime_data::RuntimeData;
pub use template_path_info::TemplatePathInfo;
