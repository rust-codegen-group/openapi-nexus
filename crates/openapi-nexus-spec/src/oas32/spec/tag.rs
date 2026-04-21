use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::spec_extensions;

/// Adds metadata to a single tag that is used by the Operation Object.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Tag {
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(flatten, with = "spec_extensions")]
    pub extensions: BTreeMap<String, serde_json::Value>,
}
