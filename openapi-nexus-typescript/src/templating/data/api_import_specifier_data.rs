//! API import specifier data for template rendering

use serde::{Deserialize, Serialize};

/// Import specifier for template rendering
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ApiImportSpecifier {
    pub name: String,
    pub alias: Option<String>,
    pub is_type: bool,
}
