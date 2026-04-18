//! Custom serde serialization/deserialization helpers

use std::str::FromStr;

use serde::{Deserialize, Serializer};

/// Module for serializing/deserializing `http::Method` as a string
pub mod http_method {
    use super::*;

    /// Serialize `http::Method` as a string
    pub fn serialize<S>(method: &http::Method, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(method.as_str())
    }

    /// Deserialize `http::Method` from a string
    pub fn deserialize<'de, D>(deserializer: D) -> Result<http::Method, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        http::Method::from_str(&s).map_err(serde::de::Error::custom)
    }
}
