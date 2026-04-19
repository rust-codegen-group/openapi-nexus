//! Supported languages for code generation

use serde::{Deserialize, Serialize};

/// Supported languages for code generation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Language {
    /// TypeScript/JavaScript
    #[serde(rename = "TypeScript")]
    TypeScript,
    /// Go
    #[serde(rename = "Go")]
    Go,
}

serde_plain::derive_display_from_serialize!(Language);
serde_plain::derive_fromstr_from_deserialize!(Language);
