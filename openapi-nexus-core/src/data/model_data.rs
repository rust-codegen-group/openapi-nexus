//! Model data for template generation

use utoipa::openapi::RefOr;
use utoipa::openapi::schema::Schema;

/// Model data
#[derive(Clone)]
pub struct ModelData {
    pub name: String,
    pub schema: RefOr<Schema>,
}
