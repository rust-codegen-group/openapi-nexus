//! TypeScript module system definitions

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// TypeScript module systems
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TypeScriptModule {
    CommonJS,
    ES2020,
    ES2022,
    ESNext,
}

impl FromStr for TypeScriptModule {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "commonjs" | "cjs" => Ok(Self::CommonJS),
            "esnext" => Ok(Self::ESNext),
            "es2020" => Ok(Self::ES2020),
            "es2022" => Ok(Self::ES2022),
            _ => Err(format!(
                "Invalid TypeScript module: '{}'. Expected one of: commonjs, esnext, es2020, es2022",
                s
            )),
        }
    }
}

impl fmt::Display for TypeScriptModule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeScriptModule::CommonJS => write!(f, "CommonJS"),
            TypeScriptModule::ES2020 => write!(f, "ES2020"),
            TypeScriptModule::ES2022 => write!(f, "ES2022"),
            TypeScriptModule::ESNext => write!(f, "ESNext"),
        }
    }
}
