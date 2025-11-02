//! Project file data for template generation

use std::sync::Arc;

use minijinja::value::{Object, ObjectRepr, Value};

use openapi_nexus_core::data::ReadmeData;

/// Project file data for template context
#[derive(Debug, Clone)]
pub struct ProjectFileData {
    pub readme_data: Option<ReadmeData>,
    pub package_name: String,
    pub version: String,
}

impl ProjectFileData {
    /// Create new project file data
    pub fn new(readme_data: Option<ReadmeData>, package_name: String, version: String) -> Self {
        Self {
            readme_data,
            package_name,
            version,
        }
    }
}

impl Object for ProjectFileData {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Map
    }

    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        let key_str = key.as_str()?;
        match key_str {
            "readme_data" => self.readme_data.as_ref().map(Value::from_serialize),
            "package_name" => Some(Value::from(self.package_name.clone())),
            "version" => Some(Value::from(self.version.clone())),
            _ => None,
        }
    }
}
