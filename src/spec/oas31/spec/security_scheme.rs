use serde::{Deserialize, Serialize};

use super::{ErrorRef, Flows, FromRef, OpenApiV31Spec, Ref, RefType};

/// Defines a security scheme that can be used by the operations.
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

    #[serde(rename = "mutualTLS")]
    MutualTls {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
}

impl FromRef for SecurityScheme {
    fn from_ref(spec: &OpenApiV31Spec, path: &str) -> Result<Self, ErrorRef> {
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

#[cfg(test)]
mod tests {
    use url::Url;

    use super::SecurityScheme;

    #[test]
    fn test_http_basic_deser() {
        const HTTP_BASIC_SAMPLE: &str = r#"{"type": "http", "scheme": "basic"}"#;
        let obj: SecurityScheme = serde_json::from_str(HTTP_BASIC_SAMPLE).unwrap();

        assert!(matches!(
            obj,
            SecurityScheme::Http {
                description: None,
                scheme,
                bearer_format: None,
            } if scheme == "basic"
        ));
    }

    #[test]
    fn test_security_scheme_oauth_deser() {
        const IMPLICIT_OAUTH2_SAMPLE: &str = r#"{
          "type": "oauth2",
          "flows": {
            "implicit": {
              "authorizationUrl": "https://example.com/api/oauth/dialog",
              "scopes": {
                "write:pets": "modify pets in your account",
                "read:pets": "read your pets"
              }
            },
            "authorizationCode": {
              "authorizationUrl": "https://example.com/api/oauth/dialog",
              "tokenUrl": "https://example.com/api/oauth/token",
              "scopes": {
                "write:pets": "modify pets in your account",
                "read:pets": "read your pets"
              }
            }
          }
        }"#;

        let obj: SecurityScheme = serde_json::from_str(IMPLICIT_OAUTH2_SAMPLE).unwrap();
        match obj {
            SecurityScheme::OAuth2 {
                description: _,
                flows,
            } => {
                assert!(flows.implicit.is_some());
                let implicit = flows.implicit.unwrap();
                assert_eq!(
                    implicit.authorization_url,
                    Url::parse("https://example.com/api/oauth/dialog").unwrap()
                );
                assert!(implicit.scopes.contains_key("write:pets"));
                assert!(implicit.scopes.contains_key("read:pets"));

                assert!(flows.authorization_code.is_some());
                let auth_code = flows.authorization_code.unwrap();
                assert_eq!(
                    auth_code.authorization_url,
                    Url::parse("https://example.com/api/oauth/dialog").unwrap()
                );
                assert_eq!(
                    auth_code.token_url,
                    Url::parse("https://example.com/api/oauth/token").unwrap()
                );
            }
            _ => panic!("wrong security scheme type"),
        }
    }
}
