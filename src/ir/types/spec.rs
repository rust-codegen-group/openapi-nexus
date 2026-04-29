//! Top-level IR spec types.

use indexmap::IndexMap;
use serde::Serialize;

use super::operation::{IrOperation, IrSecurityRequirement};
use super::schema::IrSchema;

/// The top-level intermediate representation of an OpenAPI specification.
/// Version-agnostic — OAS 3.0, 3.1, and 3.2 all lower into this same type.
#[derive(Debug, Clone, Serialize)]
pub struct IrSpec {
    pub info: IrInfo,
    pub servers: Vec<IrServer>,
    pub schemas: IndexMap<String, IrSchema>,
    pub operations: Vec<IrOperation>,
    pub security_schemes: IndexMap<String, IrSecurityScheme>,
    pub security: Vec<IrSecurityRequirement>,
}

/// API metadata.
#[derive(Debug, Clone, Serialize)]
pub struct IrInfo {
    pub title: String,
    pub description: Option<String>,
    pub version: String,
    pub terms_of_service: Option<String>,
    pub contact: Option<IrContact>,
    pub license: Option<IrLicense>,
}

/// Contact information.
#[derive(Debug, Clone, Serialize)]
pub struct IrContact {
    pub name: Option<String>,
    pub url: Option<String>,
    pub email: Option<String>,
}

/// License information.
#[derive(Debug, Clone, Serialize)]
pub struct IrLicense {
    pub name: String,
    pub url: Option<String>,
    pub identifier: Option<String>,
}

/// Server definition.
#[derive(Debug, Clone, Serialize)]
pub struct IrServer {
    pub url: String,
    pub description: Option<String>,
}

/// Security scheme definition.
#[derive(Debug, Clone, Serialize)]
pub enum IrSecurityScheme {
    ApiKey {
        name: String,
        location: ApiKeyLocation,
        description: Option<String>,
    },
    Http {
        scheme: String,
        bearer_format: Option<String>,
        description: Option<String>,
    },
    OAuth2 {
        flows: Box<IrOAuth2Flows>,
        description: Option<String>,
    },
    OpenIdConnect {
        open_id_connect_url: String,
        description: Option<String>,
    },
    MutualTls {
        description: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ApiKeyLocation {
    Query,
    Header,
    Cookie,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct IrOAuth2Flows {
    pub implicit: Option<IrOAuth2Flow>,
    pub password: Option<IrOAuth2Flow>,
    pub client_credentials: Option<IrOAuth2Flow>,
    pub authorization_code: Option<IrOAuth2Flow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IrOAuth2Flow {
    pub authorization_url: Option<String>,
    pub token_url: Option<String>,
    pub refresh_url: Option<String>,
    pub scopes: IndexMap<String, String>,
}
