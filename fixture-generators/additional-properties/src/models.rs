//! Multi-level structs using HashMap for additional properties (OpenAPI additionalProperties).
//! Each HashMap has a different value kind: string, integer, boolean, array, object ref, free-form.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

/// Leaf level: fixed fields plus maps with string and integer value kinds.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LeafValue {
    /// Numeric value.
    pub value: i32,
    /// Optional label.
    pub label: Option<String>,
    /// Additional string attributes (additionalProperties: string).
    pub attributes: HashMap<String, String>,
    /// Scores by name (additionalProperties: integer).
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub scores: HashMap<String, i32>,
    /// Free-form additional properties (additionalProperties: true).
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    #[schema(additional_properties = true)]
    pub extras: HashMap<String, serde_json::Value>,
}

/// Middle level: fixed fields plus maps with object ref and boolean value kinds.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MiddleLevel {
    /// Key identifying this middle node.
    pub key: String,
    /// Count.
    pub count: i32,
    /// Nested leaf objects by name (additionalProperties: LeafValue).
    pub nested: HashMap<String, LeafValue>,
    /// Flags by name (additionalProperties: boolean).
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub flags: HashMap<String, bool>,
    /// Free-form additional properties (additionalProperties: true).
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    #[schema(additional_properties = true)]
    pub extras: HashMap<String, serde_json::Value>,
}

/// Root level: fixed fields plus maps with object ref and array value kinds.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RootLevel {
    /// Root id.
    pub id: i64,
    /// Root name.
    pub name: String,
    /// Child nodes by name (additionalProperties: MiddleLevel).
    pub children: HashMap<String, MiddleLevel>,
    /// Labels by key (additionalProperties: array of string).
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub labels: HashMap<String, Vec<String>>,
    /// Free-form additional properties (additionalProperties: true).
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    #[schema(additional_properties = true)]
    pub extras: HashMap<String, serde_json::Value>,
}
