use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{
    ErrorRef, Example, FromRef, MediaType, ObjectOrReference, ObjectSchema, OpenApiV31Spec, Ref,
    RefType, spec_extensions,
};

/// Parameter location.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ParameterIn {
    Path,
    Query,
    Header,
    Cookie,
}

/// Parameter style.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ParameterStyle {
    Matrix,
    Label,
    Form,
    Simple,
    SpaceDelimited,
    PipeDelimited,
    DeepObject,
}

/// Describes a single operation parameter.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Parameter {
    pub name: String,

    #[serde(rename = "in")]
    pub location: ParameterIn,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_empty_value: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<ParameterStyle>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub explode: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_reserved: Option<bool>,

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

impl FromRef for Parameter {
    fn from_ref(spec: &OpenApiV31Spec, path: &str) -> Result<Self, ErrorRef> {
        let refpath = path.parse::<Ref>()?;

        match refpath.kind {
            RefType::Parameter => spec
                .components
                .as_ref()
                .and_then(|cs| cs.parameters.get(&refpath.name))
                .ok_or_else(|| ErrorRef::Unresolvable {
                    path: path.to_owned(),
                })
                .and_then(|oor| oor.resolve(spec)),

            typ => Err(ErrorRef::MismatchedType {
                expected: typ,
                actual: RefType::Parameter,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Parameter;

    #[test]
    fn deserialization() {
        let spec = r#"{
            "name": "foo",
            "in": "query",
            "description": "bar",
            "required": false,
            "schema": {
                "type": "string"
            }
        }"#;

        let parameter = serde_json::from_str::<Parameter>(spec).unwrap();
        assert_eq!(parameter.name, "foo");
    }
}
