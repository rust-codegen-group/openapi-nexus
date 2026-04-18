use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{
    Encoding, ErrorSpec, Example, MediaTypeExamples, ObjectOrReference, ObjectSchema,
    OpenApiV30Spec, spec_extensions,
};

/// Each Media Type Object provides schema and examples for the media type identified by its key.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct MediaType {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<ObjectOrReference<ObjectSchema>>,

    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub examples: Option<MediaTypeExamples>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub encoding: BTreeMap<String, Encoding>,

    #[serde(flatten, with = "spec_extensions")]
    pub extensions: BTreeMap<String, serde_json::Value>,
}

impl MediaType {
    /// Resolves and returns the JSON schema definition for this media type.
    pub fn schema(&self, spec: &OpenApiV30Spec) -> Result<Option<ObjectSchema>, ErrorSpec> {
        let Some(schema) = self.schema.as_ref() else {
            return Ok(None);
        };

        let schema = schema
            .resolve(spec)
            .map_err(|e| ErrorSpec::Ref { source: e })?;

        Ok(Some(schema))
    }

    /// Resolves and returns the provided examples for this media type.
    pub fn examples(&self, spec: &OpenApiV30Spec) -> BTreeMap<String, Example> {
        self.examples
            .as_ref()
            .map(|examples| examples.resolve_all(spec))
            .unwrap_or_default()
    }
}
