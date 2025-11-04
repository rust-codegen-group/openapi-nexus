//! Supported languages for code generation

use clap::ValueEnum;

/// Supported languages for code generation
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Language {
    /// TypeScript/JavaScript
    #[value(name = "typescript", alias = "ts")]
    TypeScript,
}

impl Language {
    /// Get the primary language identifier string
    pub fn as_str(&self) -> &'static str {
        match self {
            Language::TypeScript => "typescript",
        }
    }
}
