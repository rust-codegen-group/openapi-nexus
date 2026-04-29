use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{Example, ObjectOrReference, OpenApiV32Spec};

/// Examples for a media type.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum MediaTypeExamples {
    /// Examples of the media type.
    Examples {
        examples: BTreeMap<String, ObjectOrReference<Example>>,
    },

    /// Example of the media type.
    Example { example: serde_json::Value },
}

impl Default for MediaTypeExamples {
    fn default() -> Self {
        MediaTypeExamples::Examples {
            examples: BTreeMap::new(),
        }
    }
}

impl MediaTypeExamples {
    /// Returns true if no examples are provided.
    pub fn is_empty(&self) -> bool {
        match self {
            MediaTypeExamples::Example { .. } => false,
            MediaTypeExamples::Examples { examples } => examples.is_empty(),
        }
    }

    /// Resolves references and returns a map of provided examples keyed by name.
    pub fn resolve_all(&self, spec: &OpenApiV32Spec) -> BTreeMap<String, Example> {
        match self {
            Self::Example { example } => {
                let example = Example {
                    description: None,
                    summary: None,
                    value: Some(example.clone()),
                    external_value: None,
                    extensions: BTreeMap::default(),
                };

                let mut map = BTreeMap::new();
                map.insert("default".to_owned(), example);

                map
            }

            Self::Examples { examples } => examples
                .iter()
                .filter_map(|(name, oor)| oor.resolve(spec).map(|obj| (name.clone(), obj)).ok())
                .collect(),
        }
    }
}
