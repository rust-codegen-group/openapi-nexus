use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{ErrorRef, FromRef, OpenApiV32Spec, Ref, RefType, spec_extensions};

/// Multi-purpose example objects.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct Example {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,

    #[serde(rename = "externalValue", skip_serializing_if = "Option::is_none")]
    pub external_value: Option<String>,

    #[serde(flatten, with = "spec_extensions")]
    pub extensions: BTreeMap<String, serde_json::Value>,
}

impl Example {
    /// Returns JSON-encoded bytes of this example's value.
    pub fn as_bytes(&self) -> Vec<u8> {
        match self.value {
            Some(ref val) => serde_json::to_string(val).unwrap().as_bytes().to_owned(),
            None => vec![],
        }
    }
}

impl FromRef for Example {
    fn from_ref(spec: &OpenApiV32Spec, path: &str) -> Result<Self, ErrorRef> {
        let refpath = path.parse::<Ref>()?;

        match refpath.kind {
            RefType::Example => spec
                .components
                .as_ref()
                .and_then(|cs| cs.examples.get(&refpath.name))
                .ok_or_else(|| ErrorRef::Unresolvable {
                    path: path.to_owned(),
                })
                .and_then(|oor| oor.resolve(spec)),

            typ => Err(ErrorRef::MismatchedType {
                expected: typ,
                actual: RefType::Example,
            }),
        }
    }
}
