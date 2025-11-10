use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utoipa::openapi;

use super::content_type::ContentType;
use super::status_code::StatusCode;

/// Normalized representation of an OpenAPI response for template generation.
///
/// `HttpResponse` captures the status code, body schemas grouped by content type,
/// a human-readable description, and response headers. It is consumed by the
/// TypeScript generators to drive return-type and transformer creation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpResponse {
    pub status: StatusCode,
    pub description: String,
    pub headers: BTreeMap<String, openapi::Header>,
    pub contents: BTreeMap<ContentType, Option<openapi::RefOr<openapi::schema::Schema>>>,
}

impl HttpResponse {
    /// Construct an `HttpResponse` from the resolved OpenAPI `Response`.
    ///
    /// The method copies metadata we need during code generation while preserving
    /// any referenced schemas for later resolution.
    pub fn from_openapi(status: StatusCode, response: &openapi::Response) -> HttpResponse {
        let contents = response
            .content
            .iter()
            .map(|(content_type, media_type)| {
                let parsed_type = content_type.parse::<ContentType>().unwrap();
                (parsed_type, media_type.schema.clone())
            })
            .collect();

        HttpResponse {
            status,
            description: response.description.clone(),
            headers: response.headers.clone(),
            contents,
        }
    }

    pub fn is_success(&self) -> bool {
        self.status.is_success()
    }

    pub fn is_default(&self) -> bool {
        self.status.is_default()
    }

    pub fn json_schema(&self) -> Option<&openapi::RefOr<openapi::schema::Schema>> {
        self.contents
            .get(&ContentType::Json)
            .and_then(|schema| schema.as_ref())
    }

    pub fn content_types(&self) -> impl Iterator<Item = &ContentType> {
        self.contents.keys()
    }

    pub fn has_body(&self) -> bool {
        !self.contents.is_empty()
    }

    pub fn has_json_body(&self) -> bool {
        self.contents.contains_key(&ContentType::Json)
    }
}
