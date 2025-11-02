//! Parameter information with raw OpenAPI schema

use serde::{Deserialize, Serialize};
use utoipa::openapi;
use utoipa::openapi::RefOr;
use utoipa::openapi::schema::Schema;

/// Parameter information with raw OpenAPI schema
#[derive(Clone, Serialize, Deserialize)]
pub struct ParameterInfo {
    pub name: String,
    pub schema: Option<RefOr<Schema>>,
    pub required: bool,
    pub deprecated: bool,
    pub location: openapi::path::ParameterIn,
}
