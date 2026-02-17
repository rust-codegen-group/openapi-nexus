use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};

/// A discriminator object for serialization/deserialization when payloads may be one of several schemas.
/// OAS 3.0 defines this as an object with propertyName; some specs use a string shorthand (property name only).
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Discriminator {
    pub property_name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub mapping: Option<BTreeMap<String, String>>,
}

impl<'de> Deserialize<'de> for Discriminator {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct DiscriminatorObject {
            property_name: String,
            #[serde(default)]
            mapping: Option<BTreeMap<String, String>>,
        }

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum StringOrObject {
            String(String),
            Object(DiscriminatorObject),
        }

        match StringOrObject::deserialize(deserializer)? {
            StringOrObject::String(property_name) => Ok(Discriminator {
                property_name,
                mapping: None,
            }),
            StringOrObject::Object(obj) => Ok(Discriminator {
                property_name: obj.property_name,
                mapping: obj.mapping,
            }),
        }
    }
}
