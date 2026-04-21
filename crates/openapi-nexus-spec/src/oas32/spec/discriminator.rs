use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A discriminator object for serialization/deserialization when payloads may be one of several schemas.
#[derive(Debug, Clone, PartialEq, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Discriminator {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub property_name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub mapping: Option<BTreeMap<String, String>>,
}
