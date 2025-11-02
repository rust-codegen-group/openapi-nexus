use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use super::ts_enum_definition::TsEnumDefinition;
use super::ts_interface_definition::TsInterfaceDefinition;
use super::ts_type_alias_definition::TsTypeAliasDefinition;
use crate::emission::error::EmitError;
use openapi_nexus_core::traits::{EmissionContext, ToRcDocWithContext};

/// Unified TypeScript type definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TsTypeDefinition {
    Interface(TsInterfaceDefinition),
    TypeAlias(TsTypeAliasDefinition),
    Enum(TsEnumDefinition),
}

impl ToRcDocWithContext for TsTypeDefinition {
    type Error = EmitError;

    fn to_rcdoc_with_context(
        &self,
        context: &EmissionContext,
    ) -> Result<RcDoc<'static, ()>, EmitError> {
        match self {
            TsTypeDefinition::Interface(interface) => interface.to_rcdoc_with_context(context),
            TsTypeDefinition::TypeAlias(type_alias) => type_alias.to_rcdoc_with_context(context),
            TsTypeDefinition::Enum(enum_def) => enum_def.to_rcdoc_with_context(context),
        }
    }
}
