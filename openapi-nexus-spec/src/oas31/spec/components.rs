use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{
    Callback, Example, Header, Link, ObjectOrReference, Parameter, PathItem, RequestBody, Response,
    SecurityScheme, schema::ObjectSchema, spec_extensions,
};

/// Holds a set of reusable objects for different aspects of the OAS.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct Components {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub schemas: BTreeMap<String, ObjectOrReference<ObjectSchema>>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub responses: BTreeMap<String, ObjectOrReference<Response>>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<String, ObjectOrReference<Parameter>>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub examples: BTreeMap<String, ObjectOrReference<Example>>,

    #[serde(
        rename = "requestBodies",
        default,
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub request_bodies: BTreeMap<String, ObjectOrReference<RequestBody>>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, ObjectOrReference<Header>>,

    #[serde(
        rename = "pathItems",
        default,
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub path_items: BTreeMap<String, ObjectOrReference<PathItem>>,

    #[serde(
        rename = "securitySchemes",
        default,
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub security_schemes: BTreeMap<String, ObjectOrReference<SecurityScheme>>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub links: BTreeMap<String, ObjectOrReference<Link>>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub callbacks: BTreeMap<String, ObjectOrReference<Callback>>,

    #[serde(flatten, with = "spec_extensions")]
    pub extensions: BTreeMap<String, serde_json::Value>,
}
