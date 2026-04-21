use std::{str::FromStr, sync::OnceLock};

use derive_more::Display;
use regex::Regex;
use serde::{Deserialize, Serialize};
use snafu::Snafu;

use super::OpenApiV32Spec;

fn re_ref() -> &'static Regex {
    static RE_REF: OnceLock<Regex> = OnceLock::new();
    RE_REF.get_or_init(|| {
        Regex::new("^(?P<source>[^#]*)#/components/(?P<type>[^/]+)/(?P<name>.+)$").unwrap()
    })
}

/// Container for a type of OpenAPI object, or a reference to one.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ObjectOrReference<T> {
    /// Object reference.
    ///
    /// See <https://spec.openapis.org/oas/v3.2.0#reference-object>.
    Ref {
        /// Path, file reference, or URL pointing to object.
        #[serde(rename = "$ref")]
        ref_path: String,

        /// Summary override.
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<String>,

        /// Description override.
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },

    /// Inline object.
    Object(T),
}

impl<T> ObjectOrReference<T>
where
    T: FromRef,
{
    /// Resolves the object (if needed) from the given `spec` and returns it.
    pub fn resolve(&self, spec: &OpenApiV32Spec) -> Result<T, ErrorRef> {
        match self {
            Self::Object(component) => Ok(component.clone()),
            Self::Ref { ref_path, .. } => T::from_ref(spec, ref_path),
        }
    }
}

/// Object reference error.
#[derive(Debug, Clone, PartialEq, Snafu)]
#[snafu(visibility(pub))]
pub enum ErrorRef {
    /// Referenced object has unknown type.
    #[snafu(display("Invalid type: {}", type_name))]
    UnknownType { type_name: String },

    /// Referenced object was not of expected type.
    #[snafu(display("Mismatched type: cannot reference a {} as a {}", expected, actual))]
    MismatchedType { expected: RefType, actual: RefType },

    /// Reference path points outside the given spec file.
    #[snafu(display("Unresolvable path: {}", path))]
    Unresolvable { path: String },
}

/// Component type of a reference.
#[derive(Debug, Clone, Copy, PartialEq, Display)]
pub enum RefType {
    /// Schema component type.
    Schema,

    /// Response component type.
    Response,

    /// Parameter component type.
    Parameter,

    /// Example component type.
    Example,

    /// Request body component type.
    RequestBody,

    /// Header component type.
    Header,

    /// Security scheme component type.
    SecurityScheme,

    /// Link component type.
    Link,

    /// Callback component type.
    Callback,
}

impl FromStr for RefType {
    type Err = ErrorRef;

    fn from_str(typ: &str) -> Result<Self, Self::Err> {
        Ok(match typ {
            "schemas" => Self::Schema,
            "responses" => Self::Response,
            "parameters" => Self::Parameter,
            "examples" => Self::Example,
            "requestBodies" => Self::RequestBody,
            "headers" => Self::Header,
            "securitySchemes" => Self::SecurityScheme,
            "links" => Self::Link,
            "callbacks" => Self::Callback,
            typ => {
                return Err(ErrorRef::UnknownType {
                    type_name: typ.to_owned(),
                });
            }
        })
    }
}

/// Parsed reference path.
#[derive(Debug, Clone)]
pub struct Ref {
    /// Source file of the object being references.
    pub source: String,

    /// Type of object being referenced.
    pub kind: RefType,

    /// Name of object being referenced.
    pub name: String,
}

impl FromStr for Ref {
    type Err = ErrorRef;

    fn from_str(path: &str) -> Result<Self, Self::Err> {
        let parts = re_ref()
            .captures(path)
            .ok_or_else(|| ErrorRef::Unresolvable {
                path: path.to_owned(),
            })?;

        Ok(Self {
            source: parts["source"].to_owned(),
            kind: parts["type"].parse()?,
            name: parts["name"].to_owned(),
        })
    }
}

/// Find an object from a reference path (`$ref`).
///
/// Implemented for object types which can be shared via a spec's `components` object.
pub trait FromRef: Clone {
    /// Finds an object in `spec` using the given `path`.
    fn from_ref(spec: &OpenApiV32Spec, path: &str) -> Result<Self, ErrorRef>;
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::ObjectOrReference;

    #[test]
    fn ref_serialization_omits_empty_overrides() {
        let reference = ObjectOrReference::<()>::Ref {
            ref_path: "#/components/examples/RustMascot".to_owned(),
            summary: None,
            description: None,
        };

        let serialized = serde_json::to_value(reference).expect("serializing ref");

        assert_eq!(
            serialized,
            json!({
                "$ref": "#/components/examples/RustMascot",
            })
        );
    }

    #[test]
    fn ref_serialization_includes_present_overrides() {
        let reference = ObjectOrReference::<()>::Ref {
            ref_path: "#/components/examples/RustMascot".to_owned(),
            summary: Some("Rust mascot override".to_owned()),
            description: Some("Let Ferris do the talking.".to_owned()),
        };

        let serialized = serde_json::to_value(reference).expect("serializing ref");

        assert_eq!(
            serialized,
            json!({
                "$ref": "#/components/examples/RustMascot",
                "summary": "Rust mascot override",
                "description": "Let Ferris do the talking.",
            })
        );
    }
}
