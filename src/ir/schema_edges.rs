//! Shared raw schema-edge adapters and semantic edge projections.
//!
//! `Consumed` follows the lowering classifier and is used by reference policy.
//! `DependencyFacts` preserves every direct structural reference fact used by
//! boolean-component policy. The projections share version adapters without
//! sharing their policies.

use crate::ir::lower::LowerError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SchemaEdgeMode {
    Consumed,
    DependencyFacts,
}

pub(crate) trait SchemaEdges: Sized {
    type Member;

    fn visit_edges<'a>(
        &'a self,
        mode: SchemaEdgeMode,
        visitor: impl FnMut(&'a Self::Member) -> Result<(), LowerError>,
    ) -> Result<(), LowerError>;
    fn reference(member: &Self::Member) -> Option<&str>;
    fn object(member: &Self::Member) -> Option<&Self>;
}

macro_rules! adapt_schema_edges {
    ($version:ident, $schema:ident => $additional:expr, $object_branch:expr, $array_branch:expr) => {
        impl SchemaEdges for crate::spec::$version::spec::ObjectSchema {
            type Member = crate::spec::$version::spec::ObjectOrReference<Self>;

            fn visit_edges<'a>(
                &'a self,
                mode: SchemaEdgeMode,
                mut visitor: impl FnMut(&'a Self::Member) -> Result<(), LowerError>,
            ) -> Result<(), LowerError> {
                use crate::spec::$version::spec::Schema;

                let $schema = self;
                let additional: Option<&Schema> = $additional;
                match mode {
                    SchemaEdgeMode::Consumed => {
                        // Match lowering precedence: enum values and composition
                        // assertions own the node before container siblings.
                        if !self.enum_values.is_empty() {
                            return Ok(());
                        }
                        let composition = if !self.one_of.is_empty() {
                            Some(&self.one_of)
                        } else if !self.any_of.is_empty() {
                            Some(&self.any_of)
                        } else if !self.all_of.is_empty() {
                            Some(&self.all_of)
                        } else {
                            None
                        };
                        if let Some(members) = composition {
                            for member in members {
                                visitor(member)?;
                            }
                            return Ok(());
                        }

                        let object_branch = $object_branch;
                        let array_branch = $array_branch;
                        if object_branch {
                            for member in self.properties.values() {
                                visitor(member)?;
                            }
                            if let Some(Schema::Object(member)) = additional {
                                visitor(member)?;
                            }
                        }
                        if array_branch {
                            if let Some(Schema::Object(member)) = self.items.as_deref() {
                                visitor(member)?;
                            }
                        }
                    }
                    SchemaEdgeMode::DependencyFacts => {
                        for member in self
                            .all_of
                            .iter()
                            .chain(&self.any_of)
                            .chain(&self.one_of)
                            .chain(self.properties.values())
                        {
                            visitor(member)?;
                        }
                        for schema in self.items.as_deref().into_iter().chain(additional) {
                            if let Schema::Object(member) = schema {
                                visitor(member)?;
                            }
                        }
                    }
                }
                Ok(())
            }

            fn reference(member: &Self::Member) -> Option<&str> {
                match member {
                    Self::Member::Ref { ref_path, .. } => Some(ref_path.as_str()),
                    Self::Member::Object(_) => None,
                }
            }

            fn object(member: &Self::Member) -> Option<&Self> {
                match member {
                    Self::Member::Object(object) => Some(object),
                    Self::Member::Ref { .. } => None,
                }
            }
        }
    };
}

adapt_schema_edges!(
    oas30,
    schema => match &schema.additional_properties {
        Some(crate::spec::oas30::spec::AdditionalProperties::Schema(value)) => Some(value.as_ref()),
        _ => None,
    },
    schema
        .schema_type
        .as_deref()
        .is_none_or(|kind| kind == "object")
        && (!schema.properties.is_empty() || schema.additional_properties.is_some()),
    schema.schema_type.as_deref() == Some("array")
        || (schema.schema_type.is_none()
            && schema.properties.is_empty()
            && schema.additional_properties.is_none()
            && schema.items.is_some())
);
adapt_schema_edges!(
    oas31,
    schema => schema.additional_properties.as_ref(),
    schema
        .schema_type
        .as_ref()
        .is_none_or(|types| types.contains(crate::spec::oas31::spec::SchemaType::Object))
        && (!schema.properties.is_empty() || schema.additional_properties.is_some()),
    schema
        .schema_type
        .as_ref()
        .is_some_and(|types| types.contains(crate::spec::oas31::spec::SchemaType::Array))
        || (schema.schema_type.is_none()
            && schema.properties.is_empty()
            && schema.additional_properties.is_none()
            && schema.items.is_some())
);
adapt_schema_edges!(
    oas32,
    schema => schema.additional_properties.as_ref(),
    schema
        .schema_type
        .as_ref()
        .is_none_or(|types| types.contains(crate::spec::oas32::spec::SchemaType::Object))
        && (!schema.properties.is_empty() || schema.additional_properties.is_some()),
    schema
        .schema_type
        .as_ref()
        .is_some_and(|types| types.contains(crate::spec::oas32::spec::SchemaType::Array))
        || (schema.schema_type.is_none()
            && schema.properties.is_empty()
            && schema.additional_properties.is_none()
            && schema.items.is_some())
);
