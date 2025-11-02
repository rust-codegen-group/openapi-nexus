use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use super::ts_enum_definition::TsEnumDefinition;
use super::ts_interface_definition::TsInterfaceDefinition;
use super::ts_type_alias_definition::TsTypeAliasDefinition;
use crate::emission::error::EmitError;
use crate::templating::Templates;
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
            TsTypeDefinition::Interface(interface) => {
                // Emit the interface itself
                let iface_doc = interface.to_rcdoc_with_context(context)?;

                // Emit helper functions using template-based generation
                let helpers_doc = emit_model_helpers_with_template(interface)?;

                Ok(RcDoc::intersperse(
                    vec![iface_doc, helpers_doc],
                    RcDoc::line().append(RcDoc::line()),
                ))
            }
            TsTypeDefinition::TypeAlias(type_alias) => type_alias.to_rcdoc_with_context(context),
            TsTypeDefinition::Enum(enum_def) => enum_def.to_rcdoc_with_context(context),
        }
    }
}

/// Emit model helper functions using the template engine
fn emit_model_helpers_with_template(
    interface: &TsInterfaceDefinition,
) -> Result<RcDoc<'static, ()>, EmitError> {
    // Prepare data for the template
    let required_props: Vec<&str> = interface
        .properties
        .iter()
        .filter(|p| !p.optional)
        .map(|p| p.name.as_str())
        .filter(|name| !name.starts_with('['))
        .collect();

    let properties: Vec<serde_json::Value> = interface
        .properties
        .iter()
        .map(|p| {
            serde_json::json!({
                "name": p.name,
                "optional": p.optional,
                "is_index_signature": p.name.starts_with('['),
            })
        })
        .collect();

    let data = serde_json::json!({
        "name": interface.signature.name,
        "required_props": required_props,
        "properties": properties,
    });

    let templating = Templates::new();
    let rendered = templating.emit_model_helpers(&data)?;
    Ok(RcDoc::text(rendered))
}
