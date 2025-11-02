//! Template functions module

pub mod file_header;
pub mod model_helpers;

pub use file_header::file_header;
pub use model_helpers::{from_json_line, instance_guard_line, to_json_line};
