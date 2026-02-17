//! Fixture generator for OpenAPI additionalProperties using multi-level structs with HashMap.

pub mod handlers;
pub mod models;
pub mod openapi;

pub use handlers::{post_leaf, post_middle, post_root};
pub use models::{LeafValue, MiddleLevel, RootLevel};
pub use openapi::ApiDoc;
