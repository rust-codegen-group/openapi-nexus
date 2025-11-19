//! Unified Go type definition

use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use super::{GoStruct, GoTypeAlias};
use openapi_nexus_core::traits::ToRcDoc;

/// Unified Go type definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GoTypeDefinition {
    Struct(GoStruct),
    TypeAlias(GoTypeAlias),
}

impl GoTypeDefinition {
    /// Get the Go name of this type definition
    pub fn go_name(&self) -> &str {
        match self {
            GoTypeDefinition::Struct(s) => &s.name,
            GoTypeDefinition::TypeAlias(t) => &t.name,
        }
    }
}

impl ToRcDoc for GoTypeDefinition {
    fn to_rcdoc(&self) -> RcDoc<'static, ()> {
        match self {
            GoTypeDefinition::Struct(s) => s.to_rcdoc(),
            GoTypeDefinition::TypeAlias(t) => t.to_rcdoc(),
        }
    }
}
