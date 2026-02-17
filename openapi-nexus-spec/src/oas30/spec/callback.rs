use std::{collections::BTreeMap, error::Error as StdError};

use serde::{Deserialize, Serialize};

use super::{ErrorRef, FromRef, OpenApiV30Spec, PathItem, Ref, RefType};

/// Map of possible out-of band callbacks related to the parent operation.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(try_from = "CallbackSerde", into = "CallbackSerde")]
pub struct Callback {
    /// Map of Path Item Objects for the callback.
    pub paths: BTreeMap<String, PathItem>,

    /// Specification extensions (keys with "x-" prefix as in the document).
    pub extensions: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(transparent)]
struct CallbackSerde(serde_json::Map<String, serde_json::Value>);

impl TryFrom<CallbackSerde> for Callback {
    type Error = Box<dyn StdError>;

    fn try_from(CallbackSerde(map): CallbackSerde) -> Result<Self, Self::Error> {
        let (extensions, paths) = bisect_map(map, |key| key.starts_with("x-"));

        let paths = paths
            .into_iter()
            .map(|(key, value)| serde_json::from_value(value).map(|v| (key, v)))
            .collect::<Result<_, _>>()?;

        Ok(Self {
            paths,
            extensions: extensions.into_iter().collect(),
        })
    }
}

fn bisect_map(
    map: serde_json::Map<String, serde_json::Value>,
    predicate: fn(&String) -> bool,
) -> (
    serde_json::Map<String, serde_json::Value>,
    serde_json::Map<String, serde_json::Value>,
) {
    let mut first = map;
    let mut second = first.clone();

    first.retain(|key, _| predicate(key));
    second.retain(|key, _| !predicate(key));

    (first, second)
}

impl From<Callback> for CallbackSerde {
    fn from(val: Callback) -> Self {
        let Callback { paths, extensions } = val;

        CallbackSerde(
            paths
                .into_iter()
                .map(|(key, val)| {
                    (
                        key,
                        serde_json::to_value(val).expect("path item serialization should not fail"),
                    )
                })
                .chain(extensions)
                .collect(),
        )
    }
}

impl FromRef for Callback {
    fn from_ref(spec: &OpenApiV30Spec, path: &str) -> Result<Self, ErrorRef> {
        let refpath = path.parse::<Ref>()?;

        match refpath.kind {
            RefType::Callback => spec
                .components
                .as_ref()
                .and_then(|cs| cs.callbacks.get(&refpath.name))
                .ok_or_else(|| ErrorRef::Unresolvable {
                    path: path.to_owned(),
                })
                .and_then(|oor| oor.resolve(spec)),

            typ => Err(ErrorRef::MismatchedType {
                expected: typ,
                actual: RefType::Callback,
            }),
        }
    }
}
