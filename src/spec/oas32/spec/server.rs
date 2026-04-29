use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::spec_extensions;

/// An object representing a Server.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Server {
    pub url: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, ServerVariable>,

    #[serde(flatten, with = "spec_extensions")]
    pub extensions: BTreeMap<String, serde_json::Value>,
}

/// An object representing a Server Variable for server URL template substitution.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ServerVariable {
    pub default: String,

    #[serde(rename = "enum", default, skip_serializing_if = "Vec::is_empty")]
    pub substitutions_enum: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(flatten, with = "spec_extensions")]
    pub extensions: BTreeMap<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{Server, ServerVariable};

    #[test]
    fn server_extensions_round_trip() {
        let payload = r#"{
            "url": "https://example.com",
            "x-test": "alpha"
        }"#;

        let server: Server = serde_json::from_str(payload).expect("server parses");
        assert_eq!(server.extensions.get("test"), Some(&json!("alpha")));

        let value = serde_json::to_value(server).expect("server serializes");
        assert_eq!(value.get("x-test"), Some(&json!("alpha")));
    }

    #[test]
    fn server_variable_extensions_round_trip() {
        let payload = r#"{
            "default": "example",
            "x-meta": {"enabled": true}
        }"#;

        let variable: ServerVariable = serde_json::from_str(payload).expect("variable parses");
        assert_eq!(
            variable.extensions.get("meta"),
            Some(&json!({"enabled": true}))
        );

        let value = serde_json::to_value(variable).expect("variable serializes");
        assert_eq!(value.get("x-meta"), Some(&json!({"enabled": true})));
    }
}
