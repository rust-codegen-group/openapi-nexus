//! Supported generator frameworks for code generation

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

use crate::language::Language;

/// Supported generator frameworks for code generation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ValueEnum)]
pub enum GeneratorType {
    /// TypeScript client using Fetch API
    #[serde(rename = "typescript-fetch")]
    #[value(name = "typescript-fetch")]
    TypeScriptFetch,
    /// Go client using HTTP
    #[serde(rename = "go-http")]
    #[value(name = "go-http")]
    GoHttp,
}

serde_plain::derive_display_from_serialize!(GeneratorType);
serde_plain::derive_fromstr_from_deserialize!(GeneratorType);

impl GeneratorType {
    /// Extract the language from the generator
    pub fn language(&self) -> Language {
        match self {
            GeneratorType::TypeScriptFetch => Language::TypeScript,
            GeneratorType::GoHttp => Language::Go,
        }
    }

    /// Extract framework name from generator enum
    pub fn framework(&self) -> String {
        match self {
            GeneratorType::TypeScriptFetch => "fetch".to_string(),
            GeneratorType::GoHttp => "http".to_string(),
        }
    }
}
