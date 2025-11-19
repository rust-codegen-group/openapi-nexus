//! Supported languages for code generation

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// Supported languages for code generation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ValueEnum)]
pub enum Language {
    /// TypeScript/JavaScript
    #[serde(rename = "TypeScript")]
    #[value(name = "TypeScript", aliases = ["typescript", "ts"])]
    TypeScript,
    /// Go
    #[serde(rename = "Go")]
    #[value(name = "Go", aliases = ["go", "golang"])]
    Go,
}

serde_plain::derive_display_from_serialize!(Language);
serde_plain::derive_fromstr_from_deserialize!(Language);
