//! File naming conventions for code generation

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// File naming conventions for generated files
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NamingConvention {
    /// Use camelCase for file names
    CamelCase,
    /// Use kebab-case for file names
    KebabCase,
    /// Use snake_case for file names
    SnakeCase,
    /// Use PascalCase for file names
    PascalCase,
}

impl fmt::Display for NamingConvention {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CamelCase => write!(f, "camelCase"),
            Self::KebabCase => write!(f, "kebab-case"),
            Self::SnakeCase => write!(f, "snake_case"),
            Self::PascalCase => write!(f, "PascalCase"),
        }
    }
}

impl FromStr for NamingConvention {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "camelcase" | "camel-case" => Ok(Self::CamelCase),
            "kebabcase" | "kebab-case" => Ok(Self::KebabCase),
            "snakecase" | "snake_case" => Ok(Self::SnakeCase),
            "pascalcase" | "pascal-case" => Ok(Self::PascalCase),
            _ => Err(format!(
                "Invalid naming convention: '{}'. Expected one of: camelCase, kebab-case, snake_case, PascalCase",
                s
            )),
        }
    }
}
