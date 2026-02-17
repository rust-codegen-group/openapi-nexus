use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::spec_extensions;

/// License information for the exposed API (OAS 3.0: name required, url optional).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct License {
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    #[serde(flatten, with = "spec_extensions")]
    pub extensions: BTreeMap<String, serde_json::Value>,
}
