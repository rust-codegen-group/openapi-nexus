//! Enum representation models demonstrating different kinds of enum representations

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Variant A with string and integer fields
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VariantA {
    /// First field of variant A
    pub field1: String,
    /// Second field of variant A
    pub field2: i32,
}

/// Variant B with boolean and float fields
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VariantB {
    /// First field of variant B
    pub field3: bool,
    /// Second field of variant B
    pub field4: f64,
}

/// Externally tagged enum (default Serde representation)
///
/// This is the default representation where the variant name is the key
/// and the content is the value: `{"VariantA": {"field1": "...", "field2": 123}}`
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub enum ExternallyTaggedEnum {
    /// Variant A with string and integer fields
    #[serde(rename = "EXTERNALLY_TAGGED_ENUM_VARIANT_A")]
    VariantA(VariantA),
    /// Variant B with boolean and float fields
    #[serde(rename = "EXTERNALLY_TAGGED_ENUM_VARIANT_B")]
    VariantB(VariantB),
}

/// Internally tagged enum
///
/// The tag is a field inside the object alongside other fields:
/// `{"ty": "VariantA", "field1": "...", "field2": 123}`
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "ty")]
pub enum InternallyTaggedEnum {
    /// Variant A with string and integer fields
    #[serde(rename = "INTERNALLY_TAGGED_ENUM_VARIANT_A")]
    VariantA(VariantA),
    /// Variant B with boolean and float fields
    #[serde(rename = "INTERNALLY_TAGGED_ENUM_VARIANT_B")]
    VariantB(VariantB),
}

/// Adjacently tagged enum
///
/// The tag and content are separate fields:
/// `{"ty": "VariantA", "data": {"field1": "...", "field2": 123}}`
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "ty", content = "data")]
pub enum AdjacentlyTaggedEnum {
    /// Variant A with string and integer fields
    #[serde(rename = "ADJACENTLY_TAGGED_ENUM_VARIANT_A")]
    VariantA(VariantA),
    /// Variant B with boolean and float fields
    #[serde(rename = "ADJACENTLY_TAGGED_ENUM_VARIANT_B")]
    VariantB(VariantB),
}

/// Untagged enum
///
/// No tag is used, variants are distinguished by their structure:
/// `{"field1": "...", "field2": 123}` or `{"field3": true, "field4": 1.23}`
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum UntaggedEnum {
    /// Variant A with string and integer fields
    #[serde(rename = "UNTAGGED_ENUM_VARIANT_A")]
    VariantA(VariantA),
    /// Variant B with boolean and float fields
    #[serde(rename = "UNTAGGED_ENUM_VARIANT_B")]
    VariantB(VariantB),
}

/// Mixed enum with unit variants (SimpleA, SimpleB) and tuple variants (VariantA(VariantA), VariantB(VariantB))
///
/// Serializes as externally tagged: `{"SimpleA": null}`, `{"VariantA": {"field1": "...", "field2": 123}}`,
/// `{"SimpleB": null}`, `{"VariantB": {"field3": true, "field4": 1.23}}`
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub enum MixedEnum {
    /// Unit variant SimpleA
    SimpleA,
    /// Tuple variant with VariantA payload
    VariantA(VariantA),
    /// Unit variant SimpleB
    SimpleB,
    /// Tuple variant with VariantB payload
    VariantB(VariantB),
}
