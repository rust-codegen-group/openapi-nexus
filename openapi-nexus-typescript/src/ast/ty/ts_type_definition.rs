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
    /// Get the TypeScript name of this type definition
    ///
    /// Returns the name that will be used in the generated TypeScript code.
    pub fn ts_name(&self) -> &str {
        match self {
            TsTypeDefinition::Interface(interface) => &interface.signature.ts_name,
            TsTypeDefinition::TypeAlias(type_alias) => &type_alias.ts_name,
            TsTypeDefinition::Enum(enum_def) => &enum_def.ts_name,
        }
    }

    /// Get the original schema name from the OpenAPI spec
    ///
    /// Returns the original name from the OpenAPI specification.
    pub fn original_name(&self) -> &str {
        match self {
            TsTypeDefinition::Interface(interface) => &interface.signature.original_name,
            TsTypeDefinition::TypeAlias(type_alias) => &type_alias.original_name,
            TsTypeDefinition::Enum(enum_def) => &enum_def.original_name,
        }
    }

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

    /// Check if this type definition is an intersection type (allOf)
    ///
    /// Returns `true` if this is a type alias with intersection members.
    pub fn is_intersection_type(&self) -> bool {
        matches!(
            self,
            TsTypeDefinition::TypeAlias(alias) if alias.intersection_members.is_some()
        )
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
