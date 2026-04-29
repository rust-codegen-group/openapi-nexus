//! Tagged enum pattern types for OpenAPI schema representation
//!
//! This module defines the different ways enums can be represented in OpenAPI/JSON Schema,
//! which correspond to Serde's tagged enum representations.

use crate::spec::oas30::spec::{
    ObjectOrReference as ObjectOrReference30, ObjectSchema as ObjectSchema30,
};
use crate::spec::oas31::spec::{ObjectOrReference, ObjectSchema};
use crate::spec::oas32::spec::{
    ObjectOrReference as ObjectOrReference32, ObjectSchema as ObjectSchema32,
};
use heck::ToPascalCase as _;

/// Tagged enum pattern types
///
/// These represent the four different ways enums can be tagged in JSON:
/// - **ExternallyTagged**: The variant name is the key, content is the value
///   (e.g., `{"VariantA": {"field1": "value"}}`)
/// - **AdjacentlyTagged**: The tag and content are separate fields
///   (e.g., `{"ty": "VariantA", "data": {"field1": "value"}}`)
/// - **InternallyTagged**: The tag is a field inside the object alongside other fields
///   (e.g., `{"ty": "VariantA", "field1": "value"}`)
/// - **Untagged**: No tag is used, variants are distinguished by their structure
///   (e.g., `{"field1": "value"}` or `{"field2": 123}`)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaggedEnumPattern {
    /// Externally tagged enum (default Serde representation)
    ///
    /// The variant name is the key and the content is the value:
    /// `{"VariantA": {"field1": "...", "field2": 123}}`
    ExternallyTagged {
        /// The variant name in PascalCase
        variant_name: String,
    },
    /// Adjacently tagged enum
    ///
    /// The tag and content are separate fields:
    /// `{"ty": "VariantA", "data": {"field1": "...", "field2": 123}}`
    AdjacentlyTagged {
        /// The variant name in PascalCase
        variant_name: String,
        /// The tag field name (e.g., "ty")
        tag_field: String,
        /// The content field name (e.g., "data")
        content_field: String,
    },
    /// Internally tagged enum
    ///
    /// The tag is a field inside the object alongside other fields:
    /// `{"ty": "VariantA", "field1": "...", "field2": 123}`
    InternallyTagged {
        /// The variant name in PascalCase
        variant_name: String,
        /// The tag field name (e.g., "ty")
        tag_field: String,
    },
    /// Untagged enum
    ///
    /// No tag is used, variants are distinguished by their structure:
    /// `{"field1": "...", "field2": 123}` or `{"field3": true, "field4": 1.23}`
    Untagged {
        /// The variant name in PascalCase (from the referenced schema)
        variant_name: String,
    },
}

impl TaggedEnumPattern {
    /// Detect tagged enum pattern from a oneOf schema item
    ///
    /// Returns the detected pattern with variant name and field names embedded.
    ///
    /// # Arguments
    ///
    /// * `schema_ref` - The schema reference to analyze
    ///
    /// # Returns
    ///
    /// Returns `Some(TaggedEnumPattern)` if a tagged enum pattern is detected,
    /// with the variant name and field names embedded in the enum variant.
    /// Returns `None` if no pattern is detected.
    ///
    /// # Examples
    ///
    /// - Externally tagged: Single required property becomes the variant name
    /// - Adjacently tagged: Object with exactly 2 properties - one string enum (tag) and one object/ref (content)
    /// - Internally tagged: allOf schema with a string enum property (tag field)
    /// - Untagged: Schema reference to a component schema
    pub fn detect_from_schema(schema_ref: &ObjectOrReference<ObjectSchema>) -> Option<Self> {
        match schema_ref {
            // ExternallyTagged: object with exactly 1 required property whose value is either:
            // - a string enum (inline discriminator), OR
            // - a $ref to another schema (the property name IS the variant discriminator)
            ObjectOrReference::Object(obj_schema) if obj_schema.properties.len() == 1 => {
                if let Some((prop_name, prop_schema)) = obj_schema.properties.iter().next()
                    && obj_schema.required.contains(prop_name)
                {
                    let is_externally_tagged = match prop_schema {
                        // Inline object with string enum values
                        ObjectOrReference::Object(prop_obj) => {
                            !prop_obj.enum_values.is_empty()
                                && prop_obj
                                    .enum_values
                                    .iter()
                                    .any(|v| matches!(v, serde_json::Value::String(_)))
                        }
                        // $ref to another schema (property name is the variant key)
                        ObjectOrReference::Ref { .. } => true,
                    };
                    if is_externally_tagged {
                        let variant_name = prop_name.to_pascal_case();
                        return Some(TaggedEnumPattern::ExternallyTagged { variant_name });
                    }
                }
            }
            ObjectOrReference::Object(obj_schema) if obj_schema.properties.len() == 2 => {
                let mut tag_field: Option<String> = None;
                let mut content_field: Option<String> = None;
                let mut enum_value: Option<String> = None;

                for (prop_name, prop_schema) in &obj_schema.properties {
                    if let ObjectOrReference::Object(prop_obj) = prop_schema
                        && !prop_obj.enum_values.is_empty()
                        && let Some(serde_json::Value::String(enum_val)) =
                            prop_obj.enum_values.first()
                    {
                        tag_field = Some(prop_name.clone());
                        enum_value = Some(enum_val.clone());
                        continue;
                    }
                    match prop_schema {
                        ObjectOrReference::Object(content_obj) => {
                            if content_obj.enum_values.is_empty()
                                && !content_obj.properties.is_empty()
                            {
                                content_field = Some(prop_name.clone());
                            }
                        }
                        ObjectOrReference::Ref { .. } => {
                            content_field = Some(prop_name.clone());
                        }
                    }
                }

                if let (Some(tag), Some(content)) = (tag_field, content_field)
                    && let Some(enum_val) = enum_value
                {
                    let variant_name = enum_val.to_pascal_case();
                    return Some(TaggedEnumPattern::AdjacentlyTagged {
                        variant_name,
                        tag_field: tag,
                        content_field: content,
                    });
                }
            }
            ObjectOrReference::Object(obj_schema) if !obj_schema.all_of.is_empty() => {
                for item in &obj_schema.all_of {
                    if let ObjectOrReference::Object(item_schema) = item {
                        for (prop_name, prop_schema) in &item_schema.properties {
                            if let ObjectOrReference::Object(prop_obj) = prop_schema
                                && !prop_obj.enum_values.is_empty()
                                && let Some(serde_json::Value::String(enum_val)) =
                                    prop_obj.enum_values.first()
                            {
                                let variant_name = enum_val.to_pascal_case();
                                return Some(TaggedEnumPattern::InternallyTagged {
                                    variant_name,
                                    tag_field: prop_name.clone(),
                                });
                            }
                        }
                    }
                }
            }
            ObjectOrReference::Ref { ref_path, .. } => {
                if let Some(schema_name) = ref_path.as_str().strip_prefix("#/components/schemas/") {
                    return Some(TaggedEnumPattern::Untagged {
                        variant_name: schema_name.to_pascal_case(),
                    });
                }
            }
            _ => {}
        }
        None
    }

    /// Get the variant name from the pattern
    pub fn variant_name(&self) -> &str {
        match self {
            TaggedEnumPattern::ExternallyTagged { variant_name }
            | TaggedEnumPattern::AdjacentlyTagged { variant_name, .. }
            | TaggedEnumPattern::InternallyTagged { variant_name, .. }
            | TaggedEnumPattern::Untagged { variant_name } => variant_name,
        }
    }

    /// Get the tag field name if this pattern has one
    pub fn tag_field(&self) -> Option<&str> {
        match self {
            TaggedEnumPattern::AdjacentlyTagged { tag_field, .. }
            | TaggedEnumPattern::InternallyTagged { tag_field, .. } => Some(tag_field),
            _ => None,
        }
    }

    /// Same as [`detect_from_schema`] but for OAS 3.2 types.
    pub fn detect_from_schema_v32(
        schema_ref: &ObjectOrReference32<ObjectSchema32>,
    ) -> Option<Self> {
        match schema_ref {
            ObjectOrReference32::Object(obj_schema) if obj_schema.properties.len() == 1 => {
                if let Some((prop_name, prop_schema)) = obj_schema.properties.iter().next()
                    && obj_schema.required.contains(prop_name)
                {
                    let is_externally_tagged = match prop_schema {
                        ObjectOrReference32::Object(prop_obj) => {
                            !prop_obj.enum_values.is_empty()
                                && prop_obj
                                    .enum_values
                                    .iter()
                                    .any(|v| matches!(v, serde_json::Value::String(_)))
                        }
                        ObjectOrReference32::Ref { .. } => true,
                    };
                    if is_externally_tagged {
                        let variant_name = prop_name.to_pascal_case();
                        return Some(TaggedEnumPattern::ExternallyTagged { variant_name });
                    }
                }
            }
            ObjectOrReference32::Object(obj_schema) if obj_schema.properties.len() == 2 => {
                let mut tag_field: Option<String> = None;
                let mut content_field: Option<String> = None;
                let mut enum_value: Option<String> = None;

                for (prop_name, prop_schema) in &obj_schema.properties {
                    if let ObjectOrReference32::Object(prop_obj) = prop_schema
                        && !prop_obj.enum_values.is_empty()
                        && let Some(serde_json::Value::String(enum_val)) =
                            prop_obj.enum_values.first()
                    {
                        tag_field = Some(prop_name.clone());
                        enum_value = Some(enum_val.clone());
                        continue;
                    }
                    match prop_schema {
                        ObjectOrReference32::Object(content_obj) => {
                            if content_obj.enum_values.is_empty()
                                && !content_obj.properties.is_empty()
                            {
                                content_field = Some(prop_name.clone());
                            }
                        }
                        ObjectOrReference32::Ref { .. } => {
                            content_field = Some(prop_name.clone());
                        }
                    }
                }

                if let (Some(tag), Some(content)) = (tag_field, content_field)
                    && let Some(enum_val) = enum_value
                {
                    let variant_name = enum_val.to_pascal_case();
                    return Some(TaggedEnumPattern::AdjacentlyTagged {
                        variant_name,
                        tag_field: tag,
                        content_field: content,
                    });
                }
            }
            ObjectOrReference32::Object(obj_schema) if !obj_schema.all_of.is_empty() => {
                for item in &obj_schema.all_of {
                    if let ObjectOrReference32::Object(item_schema) = item {
                        for (prop_name, prop_schema) in &item_schema.properties {
                            if let ObjectOrReference32::Object(prop_obj) = prop_schema
                                && !prop_obj.enum_values.is_empty()
                                && let Some(serde_json::Value::String(enum_val)) =
                                    prop_obj.enum_values.first()
                            {
                                let variant_name = enum_val.to_pascal_case();
                                return Some(TaggedEnumPattern::InternallyTagged {
                                    variant_name,
                                    tag_field: prop_name.clone(),
                                });
                            }
                        }
                    }
                }
            }
            ObjectOrReference32::Ref { ref_path, .. } => {
                if let Some(schema_name) = ref_path.as_str().strip_prefix("#/components/schemas/") {
                    return Some(TaggedEnumPattern::Untagged {
                        variant_name: schema_name.to_pascal_case(),
                    });
                }
            }
            _ => {}
        }
        None
    }

    /// Same as [`detect_from_schema`] but for OAS 3.0 types.
    pub fn detect_from_schema_v30(
        schema_ref: &ObjectOrReference30<ObjectSchema30>,
    ) -> Option<Self> {
        match schema_ref {
            ObjectOrReference30::Object(obj_schema) if obj_schema.properties.len() == 1 => {
                if let Some((prop_name, prop_schema)) = obj_schema.properties.iter().next()
                    && obj_schema.required.contains(prop_name)
                {
                    let is_externally_tagged = match prop_schema {
                        ObjectOrReference30::Object(prop_obj) => {
                            !prop_obj.enum_values.is_empty()
                                && prop_obj
                                    .enum_values
                                    .iter()
                                    .any(|v| matches!(v, serde_json::Value::String(_)))
                        }
                        ObjectOrReference30::Ref { .. } => true,
                    };
                    if is_externally_tagged {
                        let variant_name = prop_name.to_pascal_case();
                        return Some(TaggedEnumPattern::ExternallyTagged { variant_name });
                    }
                }
            }
            ObjectOrReference30::Object(obj_schema) if obj_schema.properties.len() == 2 => {
                let mut tag_field: Option<String> = None;
                let mut content_field: Option<String> = None;
                let mut enum_value: Option<String> = None;

                for (prop_name, prop_schema) in &obj_schema.properties {
                    if let ObjectOrReference30::Object(prop_obj) = prop_schema
                        && !prop_obj.enum_values.is_empty()
                        && let Some(serde_json::Value::String(enum_val)) =
                            prop_obj.enum_values.first()
                    {
                        tag_field = Some(prop_name.clone());
                        enum_value = Some(enum_val.clone());
                        continue;
                    }
                    match prop_schema {
                        ObjectOrReference30::Object(content_obj) => {
                            if content_obj.enum_values.is_empty()
                                && !content_obj.properties.is_empty()
                            {
                                content_field = Some(prop_name.clone());
                            }
                        }
                        ObjectOrReference30::Ref { .. } => {
                            content_field = Some(prop_name.clone());
                        }
                    }
                }

                if let (Some(tag), Some(content)) = (tag_field, content_field)
                    && let Some(enum_val) = enum_value
                {
                    let variant_name = enum_val.to_pascal_case();
                    return Some(TaggedEnumPattern::AdjacentlyTagged {
                        variant_name,
                        tag_field: tag,
                        content_field: content,
                    });
                }
            }
            ObjectOrReference30::Object(obj_schema) if !obj_schema.all_of.is_empty() => {
                for item in &obj_schema.all_of {
                    if let ObjectOrReference30::Object(item_schema) = item {
                        for (prop_name, prop_schema) in &item_schema.properties {
                            if let ObjectOrReference30::Object(prop_obj) = prop_schema
                                && !prop_obj.enum_values.is_empty()
                                && let Some(serde_json::Value::String(enum_val)) =
                                    prop_obj.enum_values.first()
                            {
                                let variant_name = enum_val.to_pascal_case();
                                return Some(TaggedEnumPattern::InternallyTagged {
                                    variant_name,
                                    tag_field: prop_name.clone(),
                                });
                            }
                        }
                    }
                }
            }
            ObjectOrReference30::Ref { ref_path, .. } => {
                if let Some(schema_name) = ref_path.as_str().strip_prefix("#/components/schemas/") {
                    return Some(TaggedEnumPattern::Untagged {
                        variant_name: schema_name.to_pascal_case(),
                    });
                }
            }
            _ => {}
        }
        None
    }
}
