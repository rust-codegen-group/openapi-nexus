//! Supported languages for code generation

use std::fmt;

use clap::ValueEnum;

/// Supported languages for code generation
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Language {
    /// TypeScript/JavaScript
    #[value(name = "typescript", alias = "ts")]
    TypeScript,
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Language::TypeScript => write!(f, "typescript"),
        }
    }
}
