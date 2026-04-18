use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::spec_extensions;

/// Allows configuration of the supported OAuth Flows (OAS 3.0).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Flows {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub implicit: Option<ImplicitFlow>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<PasswordFlow>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_credentials: Option<ClientCredentialsFlow>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization_code: Option<AuthorizationCodeFlow>,

    #[serde(flatten, with = "spec_extensions")]
    pub extensions: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImplicitFlow {
    pub authorization_url: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_url: Option<String>,

    #[serde(default)]
    pub scopes: BTreeMap<String, String>,

    #[serde(flatten, with = "spec_extensions")]
    pub extensions: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PasswordFlow {
    pub token_url: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_url: Option<String>,

    #[serde(default)]
    pub scopes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientCredentialsFlow {
    pub token_url: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_url: Option<String>,

    #[serde(default)]
    pub scopes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizationCodeFlow {
    pub authorization_url: String,

    pub token_url: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_url: Option<String>,

    #[serde(default)]
    pub scopes: BTreeMap<String, String>,
}
