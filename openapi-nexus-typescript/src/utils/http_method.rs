//! HTTP method serialization utilities

use http::Method;
use serde::{Deserialize, Deserializer, Serializer};

/// Serialize HTTP Method to string
pub fn serialize<S>(method: &Method, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(method.as_str())
}

/// Deserialize HTTP Method from string
pub fn deserialize<'de, D>(deserializer: D) -> Result<Method, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Method::try_from(s.as_str()).map_err(serde::de::Error::custom)
}
