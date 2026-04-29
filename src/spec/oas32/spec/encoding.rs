use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{Header, ObjectOrReference};

/// A single encoding definition applied to a single schema property.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct Encoding {
    #[serde(skip_serializing_if = "Option::is_none", rename = "contentType")]
    pub content_type: Option<String>,

    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, ObjectOrReference<Header>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub explode: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none", rename = "allowReserved")]
    pub allow_reserved: Option<bool>,
}
