use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use url::Url;

use super::{Contact, License, spec_extensions};

/// General information about the API.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Info {
    pub title: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(rename = "termsOfService", skip_serializing_if = "Option::is_none")]
    pub terms_of_service: Option<Url>,

    pub version: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact: Option<Contact>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<License>,

    #[serde(flatten, with = "spec_extensions")]
    pub extensions: BTreeMap<String, serde_json::Value>,
}
