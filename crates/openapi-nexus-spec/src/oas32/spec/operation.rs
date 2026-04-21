use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{
    Callback, ErrorSpec, ExternalDoc, ObjectOrReference, OpenApiV32Spec, Parameter, RequestBody,
    Response, SecurityRequirement, Server, spec_extensions,
};

/// Describes a single API operation on a path.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct Operation {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(rename = "externalDocs", skip_serializing_if = "Option::is_none")]
    pub external_docs: Option<ExternalDoc>,

    #[serde(rename = "operationId", skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<ObjectOrReference<Parameter>>,

    #[serde(rename = "requestBody", skip_serializing_if = "Option::is_none")]
    pub request_body: Option<ObjectOrReference<RequestBody>>,

    pub responses: Option<BTreeMap<String, ObjectOrReference<Response>>>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub callbacks: BTreeMap<String, Callback>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<bool>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub security: Vec<SecurityRequirement>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub servers: Vec<Server>,

    #[serde(flatten, with = "spec_extensions")]
    pub extensions: BTreeMap<String, serde_json::Value>,
}

impl Operation {
    /// Resolves and returns this operation's request body.
    pub fn request_body(&self, spec: &OpenApiV32Spec) -> Result<Option<RequestBody>, ErrorSpec> {
        let Some(req_body) = self.request_body.as_ref() else {
            return Ok(None);
        };

        let req_body = req_body
            .resolve(spec)
            .map_err(|e| ErrorSpec::Ref { source: e })?;

        Ok(Some(req_body))
    }

    /// Resolves and returns map of this operation's responses, keyed by status code.
    pub fn responses(&self, spec: &OpenApiV32Spec) -> BTreeMap<String, Response> {
        self.responses
            .iter()
            .flatten()
            .filter_map(|(name, oor)| oor.resolve(spec).map(|obj| (name.clone(), obj)).ok())
            .collect()
    }

    /// Resolves and returns list of this operation's parameters.
    pub fn parameters(&self, spec: &OpenApiV32Spec) -> Result<Vec<Parameter>, ErrorSpec> {
        let params = self
            .parameters
            .iter()
            .filter_map(|oor| oor.resolve(spec).ok())
            .collect();

        Ok(params)
    }

    /// Finds, resolves, and returns one of this operation's parameters by name.
    pub fn parameter(
        &self,
        search: &str,
        spec: &OpenApiV32Spec,
    ) -> Result<Option<Parameter>, ErrorSpec> {
        let param = self
            .parameters(spec)?
            .iter()
            .find(|param| param.name == search)
            .cloned();

        Ok(param)
    }
}
