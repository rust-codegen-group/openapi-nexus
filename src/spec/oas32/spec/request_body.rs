use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{ErrorRef, FromRef, MediaType, OpenApiV32Spec, Ref, RefType};

/// Describes a single request body.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct RequestBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    pub content: BTreeMap<String, MediaType>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

impl FromRef for RequestBody {
    fn from_ref(spec: &OpenApiV32Spec, path: &str) -> Result<Self, ErrorRef> {
        let refpath = path.parse::<Ref>()?;

        match refpath.kind {
            RefType::RequestBody => spec
                .components
                .as_ref()
                .and_then(|cs| cs.request_bodies.get(&refpath.name))
                .ok_or_else(|| ErrorRef::Unresolvable {
                    path: path.to_owned(),
                })
                .and_then(|oor| oor.resolve(spec)),

            typ => Err(ErrorRef::MismatchedType {
                expected: typ,
                actual: RefType::RequestBody,
            }),
        }
    }
}
