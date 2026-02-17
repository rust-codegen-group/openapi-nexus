use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{
    ErrorRef, Example, FromRef, MediaType, ObjectOrReference, ObjectSchema, OpenApiV30Spec,
    ParameterStyle, Ref, RefType, spec_extensions,
};

/// Describes a single header for HTTP responses.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<ParameterStyle>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub explode: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<ObjectOrReference<ObjectSchema>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<serde_json::Value>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub examples: BTreeMap<String, ObjectOrReference<Example>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<BTreeMap<String, MediaType>>,

    #[serde(flatten, with = "spec_extensions")]
    pub extensions: BTreeMap<String, serde_json::Value>,
}

impl FromRef for Header {
    fn from_ref(spec: &OpenApiV30Spec, path: &str) -> Result<Self, ErrorRef> {
        let refpath = path.parse::<Ref>()?;

        match refpath.kind {
            RefType::Header => spec
                .components
                .as_ref()
                .and_then(|cs| cs.headers.get(&refpath.name))
                .ok_or_else(|| ErrorRef::Unresolvable {
                    path: path.to_owned(),
                })
                .and_then(|oor| oor.resolve(spec)),

            typ => Err(ErrorRef::MismatchedType {
                expected: typ,
                actual: RefType::Header,
            }),
        }
    }
}
