//! Model data for template generation

use openapi_nexus_spec::oas31::spec::{ObjectOrReference, ObjectSchema};

/// Model data
#[derive(Clone)]
pub struct ModelData {
    pub name: String,
    pub schema: ObjectOrReference<ObjectSchema>,
}
