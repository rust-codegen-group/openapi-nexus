use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{
    ErrorRef, FromRef, Header, Link, MediaType, ObjectOrReference, OpenApiV30Spec, Ref, RefType,
    spec_extensions,
};

/// Describes a single response from an API Operation.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct Response {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, ObjectOrReference<Header>>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub content: BTreeMap<String, MediaType>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub links: BTreeMap<String, ObjectOrReference<Link>>,

    #[serde(flatten, with = "spec_extensions")]
    pub extensions: BTreeMap<String, serde_json::Value>,
}

impl FromRef for Response {
    fn from_ref(spec: &OpenApiV30Spec, path: &str) -> Result<Self, ErrorRef> {
        let refpath = path.parse::<Ref>()?;

        match refpath.kind {
            RefType::Response => spec
                .components
                .as_ref()
                .and_then(|cs| cs.responses.get(&refpath.name))
                .ok_or_else(|| ErrorRef::Unresolvable {
                    path: path.to_owned(),
                })
                .and_then(|oor| oor.resolve(spec)),

            typ => Err(ErrorRef::MismatchedType {
                expected: typ,
                actual: RefType::Response,
            }),
        }
    }
}
