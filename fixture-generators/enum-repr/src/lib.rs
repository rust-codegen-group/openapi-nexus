//! Enum representation examples using different kinds of enum representations

pub mod handlers;
pub mod models;
pub mod openapi;

// Re-export commonly used items
pub use handlers::{
    handle_adjacently_tagged, handle_externally_tagged, handle_internally_tagged, handle_untagged,
};
pub use models::{AdjacentlyTaggedEnum, ExternallyTaggedEnum, InternallyTaggedEnum, UntaggedEnum};
pub use openapi::ApiDoc;
