use serde::{Deserialize, Serialize};

use super::{ErrorRef, Flows, FromRef, OpenApiV30Spec, Ref, RefType};

/// Defines a security scheme that can be used by the operations (OAS 3.0: no mutualTLS).
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum SecurityScheme {
    #[serde(rename = "apiKey")]
    ApiKey {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,

        name: String,

        #[serde(rename = "in")]
        location: String,
    },

    #[serde(rename = "http")]
    Http {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,

        scheme: String,

        #[serde(rename = "bearerFormat")]
        bearer_format: Option<String>,
    },

    #[serde(rename = "oauth2")]
    OAuth2 {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,

        flows: Flows,
    },

    #[serde(rename = "openIdConnect")]
    OpenIdConnect {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,

        #[serde(rename = "openIdConnectUrl")]
        open_id_connect_url: String,
    },
}

impl FromRef for SecurityScheme {
    fn from_ref(spec: &OpenApiV30Spec, path: &str) -> Result<Self, ErrorRef> {
        let refpath = path.parse::<Ref>()?;

        match refpath.kind {
            RefType::SecurityScheme => spec
                .components
                .as_ref()
                .and_then(|cs| cs.security_schemes.get(&refpath.name))
                .ok_or_else(|| ErrorRef::Unresolvable {
                    path: path.to_owned(),
                })
                .and_then(|oor| oor.resolve(spec)),
            typ => Err(ErrorRef::MismatchedType {
                expected: typ,
                actual: RefType::SecurityScheme,
            }),
        }
    }
}
