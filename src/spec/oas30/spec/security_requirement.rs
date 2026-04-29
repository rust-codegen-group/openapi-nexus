use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Lists the required security schemes to execute this operation.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct SecurityRequirement(pub BTreeMap<String, Vec<String>>);
