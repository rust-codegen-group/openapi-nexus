use heck::ToPascalCase as _;
use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use crate::ast::TsDocComment;
use openapi_nexus_core::traits::ToRcDoc;

/// TypeScript enum value
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TsEnumValue {
    String(String),
    Number(String), // Stored as string to preserve precision and formatting
}

impl TsEnumValue {
    /// Create from serde_json::Value, preserving type information
    ///
    /// Note: Boolean values are converted to numbers (0 for false, 1 for true)
    /// since TypeScript enums do not support boolean values directly.
    pub fn from_json_value(value: &serde_json::Value) -> Self {
        match value {
            serde_json::Value::String(s) => TsEnumValue::String(s.clone()),
            serde_json::Value::Number(n) => TsEnumValue::Number(n.to_string()),
            serde_json::Value::Bool(b) => {
                // TypeScript enums don't support booleans, convert to number (0/1)
                let num_value = if *b { "1" } else { "0" };
                TsEnumValue::Number(num_value.to_string())
            }
            _ => TsEnumValue::String(value.to_string()),
        }
    }

    /// Generate a TypeScript enum variant name from the value
    ///
    /// - String values: Convert to PascalCase (e.g., "active" -> "Active")
    ///   If the string is all digits, prefix with underscore (e.g., "123" -> "_123")
    /// - Number values: Prefix with underscore (e.g., "1" -> "_1", "42" -> "_42")
    pub fn generate_enum_name(&self) -> String {
        match self {
            TsEnumValue::String(s) => {
                if s.chars().all(|c| c.is_ascii_digit()) {
                    format!("_{}", s)
                } else {
                    s.to_pascal_case()
                }
            }
            TsEnumValue::Number(n) => format!("_{}", n),
        }
    }
}

impl ToRcDoc for TsEnumValue {
    fn to_rcdoc(&self) -> RcDoc<'static, ()> {
        match self {
            TsEnumValue::String(s) => RcDoc::text(format!("\"{}\"", s)),
            TsEnumValue::Number(n) => RcDoc::text(n.clone()),
        }
    }
}

/// TypeScript enum variant definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsEnumVariant {
    pub name: String,
    pub value: Option<TsEnumValue>,
    pub documentation: Option<TsDocComment>,
}

impl ToRcDoc for TsEnumVariant {
    fn to_rcdoc(&self) -> RcDoc<'static, ()> {
        let mut doc = RcDoc::nil();

        // Add documentation comment if present
        if let Some(docs) = &self.documentation {
            doc = doc.append(docs.to_rcdoc()).append(RcDoc::line());
        }

        // Add variant name and value
        doc = doc.append(RcDoc::text(self.name.clone()));

        if let Some(value) = &self.value {
            doc = doc
                .append(RcDoc::space())
                .append(RcDoc::text("="))
                .append(RcDoc::space())
                .append(value.to_rcdoc());
        }

        doc
    }
}
