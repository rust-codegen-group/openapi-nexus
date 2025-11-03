//! API class data for template rendering

use serde::Serialize;

use crate::ast::{TsDocComment, TsGeneric};
use crate::templating::data::{ApiClassSignature, ApiMethodData};

/// API class data for template rendering
#[derive(Debug, Clone, Serialize)]
pub struct ApiClassData {
    pub is_export: bool,
    pub name: String,
    pub generics: Vec<TsGeneric>,
    pub extends: Option<String>,
    pub implements: Vec<String>,
    pub signature: ApiClassSignature,
    pub methods: Vec<ApiMethodData>,
    pub documentation: Option<TsDocComment>,
}
