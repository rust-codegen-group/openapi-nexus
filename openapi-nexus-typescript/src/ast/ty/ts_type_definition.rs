use std::collections::BTreeSet;

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

impl TsTypeDefinition {
    /// Collect all referenced type names from this type definition
    ///
    /// Recursively traverses the type definition to find all type references
    /// that need to be imported. Returns a set of type names.
    pub fn referenced_types(&self) -> BTreeSet<String> {
        match self {
            TsTypeDefinition::Interface(interface) => {
                let mut references = BTreeSet::new();
                // Collect references from all properties
                for property in &interface.properties {
                    references.extend(property.type_expr.referenced_types());
                }
                references
            }
            TsTypeDefinition::TypeAlias(type_alias) => {
                // Collect references from the type expression
                type_alias.type_expr.referenced_types()
            }
            TsTypeDefinition::Enum(_) => {
                // Enums typically don't reference other types
                BTreeSet::new()
            }
        }
    }
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
