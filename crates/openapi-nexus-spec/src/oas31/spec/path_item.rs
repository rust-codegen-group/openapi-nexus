use std::collections::BTreeMap;

use http::Method;
use serde::{Deserialize, Serialize};

use super::{ObjectOrReference, Operation, Parameter, Server, spec_extensions};

/// Describes the operations available on a single path.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct PathItem {
    #[serde(skip_serializing_if = "Option::is_none", rename = "$ref")]
    pub reference: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub get: Option<Operation>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub put: Option<Operation>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub post: Option<Operation>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete: Option<Operation>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Operation>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<Operation>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<Operation>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<Operation>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub servers: Vec<Server>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<ObjectOrReference<Parameter>>,

    #[serde(flatten, with = "spec_extensions")]
    pub extensions: BTreeMap<String, serde_json::Value>,
}

impl PathItem {
    /// Returns iterator over this path's provided operations, keyed by method.
    pub fn methods(&self) -> impl IntoIterator<Item = (Method, &Operation)> {
        let mut methods = vec![];

        macro_rules! push_method {
            ($field:ident, $method:ident) => {{
                if let Some(ref op) = self.$field {
                    methods.push((Method::$method, op))
                }
            }};
        }

        push_method!(get, GET);
        push_method!(put, PUT);
        push_method!(post, POST);
        push_method!(delete, DELETE);
        push_method!(options, OPTIONS);
        push_method!(head, HEAD);
        push_method!(patch, PATCH);
        push_method!(trace, TRACE);

        methods
    }
}
