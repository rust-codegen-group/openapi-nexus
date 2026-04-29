use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use url::Url;

use super::spec_extensions;

/// License information for the exposed API.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct License {
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<Url>,

    #[serde(flatten, with = "spec_extensions")]
    pub extensions: BTreeMap<String, serde_json::Value>,
}
