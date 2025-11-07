//! TypeScript primitive type definitions

use std::fmt;

use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use openapi_nexus_core::traits::ToRcDoc;

/// TypeScript primitive types
#[derive(Debug, Clone, Ord, PartialOrd, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum TsPrimitive {
    String,
    Number,
    Boolean,
    Null,
    Undefined,
    Any,
    Unknown,
    Void,
    Never,
}

impl fmt::Display for TsPrimitive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            TsPrimitive::String => "string",
            TsPrimitive::Number => "number",
            TsPrimitive::Boolean => "boolean",
            TsPrimitive::Null => "null",
            TsPrimitive::Undefined => "undefined",
            TsPrimitive::Any => "any",
            TsPrimitive::Unknown => "unknown",
            TsPrimitive::Void => "void",
            TsPrimitive::Never => "never",
        };
        f.write_str(s)
    }
}

impl ToRcDoc for TsPrimitive {
    fn to_rcdoc(&self) -> RcDoc<'static, ()> {
        RcDoc::text(self.to_string())
    }
}
