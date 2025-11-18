//! Supported generator frameworks for code generation

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

use crate::language::Language;

/// Supported generator frameworks for code generation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ValueEnum)]
pub enum Generator {
    /// TypeScript client using Fetch API
    #[serde(rename = "typescript-fetch")]
    #[value(name = "typescript-fetch")]
    TypeScriptFetch,
}

serde_plain::derive_display_from_serialize!(Generator);
serde_plain::derive_fromstr_from_deserialize!(Generator);

impl Generator {
    /// Extract the language from the generator
    pub fn to_language(&self) -> Language {
        match self {
            Generator::TypeScriptFetch => Language::TypeScript,
        }
    }
}
