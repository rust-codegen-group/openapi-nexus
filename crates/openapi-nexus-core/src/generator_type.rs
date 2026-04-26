//! Supported generator frameworks for code generation

use serde::{Deserialize, Serialize};

use crate::language::Language;

/// Supported generator frameworks for code generation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GeneratorType {
    /// TypeScript client using Fetch API
    #[serde(rename = "typescript-fetch")]
    TypeScriptFetch,
    /// Go client using HTTP
    #[serde(rename = "go-http")]
    GoHttp,
    /// Rust client using reqwest
    #[serde(rename = "rust-reqwest")]
    RustReqwest,
    /// Rust client using ureq (synchronous)
    #[serde(rename = "rust-ureq")]
    RustUreq,
    /// Rust client using aioduct (hyper 1.x)
    #[serde(rename = "rust-aioduct")]
    RustAioduct,
    /// Python client using httpx
    #[serde(rename = "python-httpx")]
    PythonHttpx,
    /// Python client using requests
    #[serde(rename = "python-requests")]
    PythonRequests,
}

serde_plain::derive_display_from_serialize!(GeneratorType);
serde_plain::derive_fromstr_from_deserialize!(GeneratorType);

impl GeneratorType {
    /// Extract the language from the generator
    pub fn language(&self) -> Language {
        match self {
            GeneratorType::TypeScriptFetch => Language::TypeScript,
            GeneratorType::GoHttp => Language::Go,
            GeneratorType::RustReqwest => Language::Rust,
            GeneratorType::RustUreq => Language::Rust,
            GeneratorType::RustAioduct => Language::Rust,
            GeneratorType::PythonHttpx => Language::Python,
            GeneratorType::PythonRequests => Language::Python,
        }
    }

    /// Extract framework name from generator enum
    pub fn framework(&self) -> String {
        match self {
            GeneratorType::TypeScriptFetch => "fetch".to_string(),
            GeneratorType::GoHttp => "http".to_string(),
            GeneratorType::RustReqwest => "reqwest".to_string(),
            GeneratorType::RustUreq => "ureq".to_string(),
            GeneratorType::RustAioduct => "aioduct".to_string(),
            GeneratorType::PythonHttpx => "httpx".to_string(),
            GeneratorType::PythonRequests => "requests".to_string(),
        }
    }
}
