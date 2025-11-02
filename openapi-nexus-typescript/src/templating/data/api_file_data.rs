//! API file data for template generation

use std::sync::Arc;

use minijinja::value::{Object, ObjectRepr, Value};

use crate::ast::TsInterfaceDefinition;
use crate::ast::class_definition::TsClassDefinition;
use crate::ast::class_definition::TsImportStatement;

/// API file data for template context
#[derive(Debug, Clone)]
pub struct ApiFileData {
    pub class: TsClassDefinition,
    pub imports: Vec<TsImportStatement>,
    pub api_interface: TsInterfaceDefinition,
    pub title: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
}

impl ApiFileData {
    /// Create new API file data
    pub fn new(
        class: TsClassDefinition,
        imports: Vec<TsImportStatement>,
        api_interface: TsInterfaceDefinition,
        title: Option<String>,
        description: Option<String>,
        version: Option<String>,
    ) -> Self {
        Self {
            class,
            imports,
            api_interface,
            title,
            description,
            version,
        }
    }
}

impl Object for ApiFileData {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Map
    }

    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        let key_str = key.as_str()?;
        match key_str {
            "class" => Some(Value::from_serialize(&self.class)),
            "imports" => Some(Value::from_serialize(&self.imports)),
            "api_interface" => Some(Value::from_serialize(&self.api_interface)),
            "title" => self.title.as_ref().map(|s| Value::from(s.clone())),
            "description" => self.description.as_ref().map(|s| Value::from(s.clone())),
            "version" => self.version.as_ref().map(|s| Value::from(s.clone())),
            _ => None,
        }
    }
}
