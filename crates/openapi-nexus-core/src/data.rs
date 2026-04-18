//! Data structures for template generation

pub mod api_method_data;
pub mod content_type;
pub mod header_data;
pub mod http_response;
pub mod model_data;
pub mod operation_info;
pub mod parameter_info;
pub mod readme_data;
pub mod runtime_data;
pub mod status_code;
pub mod template_path_info;

pub use api_method_data::ApiMethodData;
pub use content_type::ContentType;
pub use header_data::HeaderData;
pub use http_response::HttpResponse;
pub use model_data::ModelData;
pub use operation_info::{OperationInfo, collect_operations_by_tag};
pub use parameter_info::{ParameterInfo, ParameterLocation};
pub use readme_data::{ReadmeData, extract_readme_data};
pub use runtime_data::RuntimeData;
pub use status_code::StatusCode;
pub use template_path_info::TemplatePathInfo;
