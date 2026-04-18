use std::convert::Infallible;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Represents commonly-used HTTP content types encountered while
/// generating client/server code. We keep a conservative set here so the
/// rest of the codebase can branch on a value instead of raw strings.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum ContentType {
    Unsupported(String),
    Json,
    Text,
    Html,
    Xml,
    FormUrlEncoded,
    MultipartFormData,
    OctetStream,
    TextEventStream,
}

impl fmt::Display for ContentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            ContentType::Json => "application/json",
            ContentType::Text => "text/plain",
            ContentType::Html => "text/html",
            ContentType::Xml => "application/xml",
            ContentType::FormUrlEncoded => "application/x-www-form-urlencoded",
            ContentType::MultipartFormData => "multipart/form-data",
            ContentType::OctetStream => "application/octet-stream",
            ContentType::TextEventStream => "text/event-stream",
            ContentType::Unsupported(value) => value,
        };

        write!(f, "{}", value)
    }
}

impl FromStr for ContentType {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(ContentType::from(s))
    }
}

impl From<&str> for ContentType {
    fn from(value: &str) -> Self {
        let normalized = value.trim().to_ascii_lowercase();
        let mime = normalized.split(';').next().unwrap_or(&normalized);

        if mime == "application/json" || mime == "text/json" || mime.ends_with("+json") {
            return ContentType::Json;
        }

        match mime {
            "text/plain" => ContentType::Text,
            "text/html" => ContentType::Html,
            "application/xml" | "text/xml" => ContentType::Xml,
            "application/x-www-form-urlencoded" => ContentType::FormUrlEncoded,
            "multipart/form-data" => ContentType::MultipartFormData,
            "application/octet-stream" => ContentType::OctetStream,
            "text/event-stream" => ContentType::TextEventStream,
            other => ContentType::Unsupported(other.to_string()),
        }
    }
}
