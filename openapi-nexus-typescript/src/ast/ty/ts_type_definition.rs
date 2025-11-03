use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use super::ts_enum_definition::TsEnumDefinition;
use super::ts_interface_definition::TsInterfaceDefinition;
use super::ts_type_alias_definition::TsTypeAliasDefinition;
use openapi_nexus_core::traits::ToRcDoc;

/// Unified TypeScript type definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TsTypeDefinition {
    Interface(TsInterfaceDefinition),
    TypeAlias(TsTypeAliasDefinition),
    Enum(TsEnumDefinition),
}

impl ToRcDoc for TsTypeDefinition {
    fn to_rcdoc(&self) -> RcDoc<'static, ()> {
        match self {
            TsTypeDefinition::Interface(interface) => interface.to_rcdoc(),
            TsTypeDefinition::TypeAlias(type_alias) => type_alias.to_rcdoc(),
            TsTypeDefinition::Enum(enum_def) => enum_def.to_rcdoc(),
        }
    }
}
