//! OpenAPI 3.1 root specification document.

use std::{collections::BTreeMap, iter::Iterator};

use http::Method;
use serde::{Deserialize, Serialize};

use super::spec_extensions;
use super::{
    Components, ErrorSpec, ExternalDoc, Info, Operation, PathItem, SecurityRequirement, Server, Tag,
};

const OPENAPI_SUPPORTED_VERSION_RANGE: &str = "~3.1";

/// A complete OpenAPI 3.1 specification.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct OpenApiV31Spec {
    /// This string MUST be the [semantic version number](https://semver.org/spec/v2.0.0.html)
    /// of the
    /// [OpenAPI Specification version](https://spec.openapis.org/oas/v3.1.1#versions)
    /// that the OpenAPI document uses.
    pub openapi: String,

    /// Provides metadata about the API.
    pub info: Info,

    /// An array of Server Objects, which provide connectivity information to a target server.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub servers: Vec<Server>,

    /// Holds the relative paths to the individual endpoints and their operations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paths: Option<BTreeMap<String, PathItem>>,

    /// An element to hold various schemas for the specification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<Components>,

    /// A declaration of which security mechanisms can be used across the API.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub security: Vec<SecurityRequirement>,

    /// A list of tags used by the specification with additional metadata.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<Tag>,

    /// The incoming webhooks that MAY be received as part of this API.
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub webhooks: BTreeMap<String, PathItem>,

    /// Additional external documentation.
    #[serde(skip_serializing_if = "Option::is_none", rename = "externalDocs")]
    pub external_docs: Option<ExternalDoc>,

    /// Specification extensions.
    #[serde(flatten, with = "spec_extensions")]
    pub extensions: BTreeMap<String, serde_json::Value>,
}

impl OpenApiV31Spec {
    /// Validates spec version field.
    pub fn validate_version(&self) -> Result<semver::Version, ErrorSpec> {
        let spec_version = &self.openapi;
        let sem_ver = semver::Version::parse(spec_version)?;
        let required_version = semver::VersionReq::parse(OPENAPI_SUPPORTED_VERSION_RANGE).unwrap();

        if required_version.matches(&sem_ver) {
            Ok(sem_ver)
        } else {
            Err(ErrorSpec::UnsupportedSpecFileVersion { version: sem_ver })
        }
    }

    /// Returns a reference to the operation with given `operation_id`, or `None` if not found.
    pub fn operation_by_id(&self, operation_id: &str) -> Option<&Operation> {
        self.operations()
            .find(|(_, _, op)| {
                op.operation_id
                    .as_deref()
                    .is_some_and(|id| id == operation_id)
            })
            .map(|(_, _, op)| op)
    }

    /// Returns a reference to the operation with given `method` and `path`, or `None` if not found.
    pub fn operation(&self, method: &Method, path: &str) -> Option<&Operation> {
        let resource = self.paths.as_ref()?.get(path)?;

        match *method {
            Method::GET => resource.get.as_ref(),
            Method::POST => resource.post.as_ref(),
            Method::PUT => resource.put.as_ref(),
            Method::PATCH => resource.patch.as_ref(),
            Method::DELETE => resource.delete.as_ref(),
            Method::HEAD => resource.head.as_ref(),
            Method::OPTIONS => resource.options.as_ref(),
            Method::TRACE => resource.trace.as_ref(),
            _ => None,
        }
    }

    /// Returns an iterator over all the operations defined in this spec.
    pub fn operations(&self) -> impl Iterator<Item = (String, Method, &Operation)> {
        let paths = &self.paths;

        let ops = paths
            .iter()
            .flatten()
            .flat_map(|(path, item)| {
                item.methods()
                    .into_iter()
                    .map(move |(method, op)| (path.to_owned(), method, op))
            })
            .collect::<Vec<_>>();

        ops.into_iter()
    }

    /// Returns a reference to the primary (first) server definition.
    pub fn primary_server(&self) -> Option<&Server> {
        self.servers.first()
    }
}
