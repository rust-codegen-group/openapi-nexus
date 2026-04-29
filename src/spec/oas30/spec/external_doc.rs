use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::spec_extensions;

/// Allows referencing an external resource for extended documentation.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ExternalDoc {
    pub url: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(flatten, with = "spec_extensions")]
    pub extensions: BTreeMap<String, serde_json::Value>,
}
