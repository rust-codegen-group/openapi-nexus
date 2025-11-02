//! Model file data for template generation

use std::sync::Arc;

use minijinja::value::{Object, ObjectRepr, Value};

use crate::ast::TsTypeDefinition;
use crate::ast::class_definition::TsImportStatement;

/// Model file data for template context
#[derive(Debug, Clone)]
pub struct ModelFileData {
    pub type_definition: TsTypeDefinition,
    pub title: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    pub imports: Vec<TsImportStatement>,
}

impl ModelFileData {
    /// Create new model file data
    pub fn new(
        type_definition: TsTypeDefinition,
        title: Option<String>,
        description: Option<String>,
        version: Option<String>,
        imports: Vec<TsImportStatement>,
    ) -> Self {
        Self {
            type_definition,
            title,
            description,
            version,
            imports,
        }
    }
}

impl Object for ModelFileData {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Map
    }

    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        let key_str = key.as_str()?;
        match key_str {
            "type_definition" => Some(Value::from_serialize(&self.type_definition)),
            "title" => self.title.as_ref().map(|s| Value::from(s.clone())),
            "description" => self.description.as_ref().map(|s| Value::from(s.clone())),
            "version" => self.version.as_ref().map(|s| Value::from(s.clone())),
            "imports" => Some(Value::from_serialize(&self.imports)),
            _ => None,
        }
    }
}
