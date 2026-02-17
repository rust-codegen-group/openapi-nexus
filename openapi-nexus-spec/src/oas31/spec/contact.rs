use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use snafu::Snafu;
use url::Url;

use super::spec_extensions;

/// Error raised when contact info contains an email field which is not a valid email.
#[derive(Debug, Snafu)]
#[snafu(display("Email address is not valid"))]
#[non_exhaustive]
pub struct ErrorInvalidEmail;

/// Contact information for the exposed API.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct Contact {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<Url>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,

    #[serde(flatten, with = "spec_extensions")]
    pub extensions: BTreeMap<String, serde_json::Value>,
}

impl Contact {
    /// Validates email address field.
    pub fn validate_email(&self) -> Result<(), ErrorInvalidEmail> {
        let Some(email) = &self.email else {
            return Ok(());
        };

        if email.contains('@') {
            Ok(())
        } else {
            Err(ErrorInvalidEmail)
        }
    }
}
