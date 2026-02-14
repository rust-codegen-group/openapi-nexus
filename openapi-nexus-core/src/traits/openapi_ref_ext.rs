use openapi_nexus_spec::oas31::spec::{Components, ObjectOrReference, ObjectSchema, Response};

pub const COMPONENTS_PREFIX: &str = "#/components/";

/// Extension trait for `ObjectOrReference::Ref` providing helpers to extract component names.
pub trait OpenApiRefExt {
    /// Returns the referenced schema name if the reference points to `#/components/schemas/...`.
    fn schema_name(&self) -> Option<&str> {
        self.component_name("schemas")
    }

    /// Returns the referenced response name if the reference points to `#/components/responses/...`.
    fn response_name(&self) -> Option<&str> {
        self.component_name("responses")
    }

    /// Returns the referenced parameter name if the reference points to `#/components/parameters/...`.
    fn parameter_name(&self) -> Option<&str> {
        self.component_name("parameters")
    }

    /// Returns the referenced example name if the reference points to `#/components/examples/...`.
    fn example_name(&self) -> Option<&str> {
        self.component_name("examples")
    }

    /// Returns the referenced request body name if the reference points to `#/components/requestBodies/...`.
    fn request_body_name(&self) -> Option<&str> {
        self.component_name("requestBodies")
    }

    /// Returns the referenced header name if the reference points to `#/components/headers/...`.
    fn header_name(&self) -> Option<&str> {
        self.component_name("headers")
    }

    /// Returns the referenced security scheme name if the reference points to `#/components/securitySchemes/...`.
    fn security_scheme_name(&self) -> Option<&str> {
        self.component_name("securitySchemes")
    }

    /// Returns the referenced link name if the reference points to `#/components/links/...`.
    fn link_name(&self) -> Option<&str> {
        self.component_name("links")
    }

    /// Returns the referenced callback name if the reference points to `#/components/callbacks/...`.
    fn callback_name(&self) -> Option<&str> {
        self.component_name("callbacks")
    }

    /// Returns the referenced path item name if the reference points to `#/components/pathItems/...`.
    fn path_item_name(&self) -> Option<&str> {
        self.component_name("pathItems")
    }

    /// Returns the component name for the given component type if the reference points inside `#/components/{component}/...`.
    fn component_name(&self, component: &str) -> Option<&str>;

    /// Resolve the referenced response from the supplied OpenAPI components.
    fn resolve_response<'a>(&self, components: Option<&'a Components>) -> Option<&'a Response>;
}

impl OpenApiRefExt for ObjectOrReference<Response> {
    fn component_name(&self, component: &str) -> Option<&str> {
        match self {
            ObjectOrReference::Ref { ref_path, .. } => extract_component_name(ref_path, component),
            ObjectOrReference::Object(_) => None,
        }
    }

    fn resolve_response<'a>(&self, components: Option<&'a Components>) -> Option<&'a Response> {
        let components = components?;
        let response_name = self.response_name()?;
        match components.responses.get(response_name)? {
            ObjectOrReference::Object(response) => Some(response),
            ObjectOrReference::Ref { .. } => {
                // TODO: Handle nested references
                None
            }
        }
    }
}

impl OpenApiRefExt for ObjectOrReference<ObjectSchema> {
    fn component_name(&self, component: &str) -> Option<&str> {
        match self {
            ObjectOrReference::Ref { ref_path, .. } => extract_component_name(ref_path, component),
            ObjectOrReference::Object(_) => None,
        }
    }

    fn resolve_response<'a>(&self, _components: Option<&'a Components>) -> Option<&'a Response> {
        None
    }
}

fn extract_component_name<'a>(reference: &'a str, component: &str) -> Option<&'a str> {
    let remainder = reference.strip_prefix(COMPONENTS_PREFIX)?;
    let remainder = remainder.strip_prefix(component)?;
    remainder.strip_prefix('/')
}
