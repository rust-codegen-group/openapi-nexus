use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{Server, spec_extensions};

/// The Link object represents a possible design-time link for a response.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Link {
    /// A relative or absolute reference to an OAS operation.
    Ref {
        #[serde(rename = "operationRef")]
        operation_ref: String,

        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        parameters: BTreeMap<String, String>,

        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,

        #[serde(skip_serializing_if = "Option::is_none")]
        server: Option<Server>,

        #[serde(flatten, with = "spec_extensions")]
        extensions: BTreeMap<String, serde_json::Value>,
    },

    /// The name of an existing, resolvable OAS operation.
    Id {
        #[serde(rename = "operationId")]
        operation_id: String,

        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        parameters: BTreeMap<String, String>,

        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,

        #[serde(skip_serializing_if = "Option::is_none")]
        server: Option<Server>,

        #[serde(flatten, with = "spec_extensions")]
        extensions: BTreeMap<String, serde_json::Value>,
    },
}
