//! Utility functions for working with OpenAPI specifications

use crate::error::IrError;
use crate::{ObjectOrReference, ObjectSchema, OpenApi, Parameter, RefOr, Response, Schema};
use openapi_nexus_common::SourceLocation;

/// Utility functions for OpenAPI processing
pub struct Utils;

impl Utils {
    /// Check if a schema is a reference
    pub fn is_reference(schema: &RefOr<Schema>) -> bool {
        matches!(schema, ObjectOrReference::Ref { .. })
    }

    /// Get the reference name if this is a reference
    pub fn get_reference_name(schema: &RefOr<Schema>) -> Option<&str> {
        match schema {
            ObjectOrReference::Ref { ref_path, .. } => Some(ref_path),
            _ => None,
        }
    }

    /// Extract all schema references from a schema (utoipa compatibility - deprecated)
    pub fn extract_schema_refs(schema: &utoipa::openapi::Schema) -> Vec<String> {
        let mut refs = Vec::new();

        match schema {
            utoipa::openapi::Schema::Object(object_schema) => {
                for prop_schema in object_schema.properties.values() {
                    if let utoipa::openapi::RefOr::Ref(ref_ref) = prop_schema {
                        refs.push(ref_ref.ref_location.clone());
                    } else if let utoipa::openapi::RefOr::T(prop_schema) = prop_schema {
                        refs.extend(Self::extract_schema_refs(prop_schema));
                    }
                }
            }
            utoipa::openapi::Schema::Array(_array_schema) => {
                // Note: Array items handling is complex in utoipa
                // For now, we'll skip array item reference extraction
            }
            _ => {} // Other schema types don't contain references
        }

        refs
    }

    /// Extract all schema references from an object schema
    pub fn extract_schema_refs_from_object_schema(object_schema: &ObjectSchema) -> Vec<String> {
        let mut refs = Vec::new();

        // Extract from properties
        for prop_schema in object_schema.properties.values() {
            match prop_schema {
                ObjectOrReference::Ref { ref_path, .. } => {
                    refs.push(ref_path.clone());
                }
                ObjectOrReference::Object(object_schema) => {
                    // In properties, schema is directly ObjectSchema, not wrapped in Schema enum
                    refs.extend(Self::extract_schema_refs_from_object_schema(object_schema));
                }
            }
        }

        refs
    }
}

/// Resolves OpenAPI references ($ref) within a specification
pub struct ReferenceResolver<'a> {
    openapi: &'a OpenApi,
}

impl<'a> ReferenceResolver<'a> {
    /// Create a new reference resolver for the given OpenAPI specification
    pub fn new(openapi: &'a OpenApi) -> Self {
        Self { openapi }
    }

    /// Resolve a schema reference to the actual schema
    pub fn resolve_schema_ref(&self, reference: &str) -> Result<&ObjectSchema, IrError> {
        if self.is_external_reference(reference) {
            let err = IrError::ExternalReference {
                reference: reference.to_string(),
                location: SourceLocation::new(),
            };
            tracing::error!("{}", err);
            return Err(err);
        }

        let (component_type, name) = self.parse_component_reference(reference)?;

        if component_type != "schemas" {
            let err = IrError::InvalidReference {
                reference: reference.to_string(),
                reason: format!("Expected 'schemas' component, got '{}'", component_type),
                location: SourceLocation::new(),
            };
            tracing::error!("{}", err);
            return Err(err);
        }

        self.openapi
            .components
            .as_ref()
            .and_then(|components| components.schemas.get(&name))
            .and_then(|schema_ref| match schema_ref {
                ObjectOrReference::Object(object_schema) => Some(object_schema),
                ObjectOrReference::Ref { .. } => None,
            })
            .ok_or_else(|| {
                let err = IrError::UnresolvedReference {
                    reference: reference.to_string(),
                    location: SourceLocation::new(),
                };
                tracing::error!("{}", err);
                err
            })
    }

    /// Resolve a response reference to the actual response
    pub fn resolve_response_ref(&self, reference: &str) -> Result<&Response, IrError> {
        if self.is_external_reference(reference) {
            let err = IrError::ExternalReference {
                reference: reference.to_string(),
                location: SourceLocation::new(),
            };
            tracing::error!("{}", err);
            return Err(err);
        }

        let (component_type, name) = self.parse_component_reference(reference)?;

        if component_type != "responses" {
            let err = IrError::InvalidReference {
                reference: reference.to_string(),
                reason: format!("Expected 'responses' component, got '{}'", component_type),
                location: SourceLocation::new(),
            };
            tracing::error!("{}", err);
            return Err(err);
        }

        self.openapi
            .components
            .as_ref()
            .and_then(|components| components.responses.get(&name))
            .and_then(|response_ref| match response_ref {
                ObjectOrReference::Object(response) => Some(response),
                ObjectOrReference::Ref { .. } => None,
            })
            .ok_or_else(|| {
                let err = IrError::UnresolvedReference {
                    reference: reference.to_string(),
                    location: SourceLocation::new(),
                };
                tracing::error!("{}", err);
                err
            })
    }

    /// Resolve a parameter reference to the actual parameter
    pub fn resolve_parameter_ref(&self, reference: &str) -> Result<&Parameter, IrError> {
        if self.is_external_reference(reference) {
            let err = IrError::ExternalReference {
                reference: reference.to_string(),
                location: SourceLocation::new(),
            };
            tracing::error!("{}", err);
            return Err(err);
        }

        let (_component_type, _name) = self.parse_component_reference(reference)?;

        // Note: utoipa Components doesn't have a parameters field
        // Parameters are typically defined inline in operations
        let err = IrError::UnresolvedReference {
            reference: reference.to_string(),
            location: SourceLocation::new(),
        };
        tracing::error!("{}", err);
        Err(err)
    }

    /// Check if a reference is external (starts with http:// or https://)
    pub fn is_external_reference(&self, reference: &str) -> bool {
        reference.starts_with("http://") || reference.starts_with("https://")
    }

    /// Parse a component reference like "#/components/schemas/Name" into ("schemas", "Name")
    pub fn parse_component_reference(&self, reference: &str) -> Result<(String, String), IrError> {
        if !reference.starts_with("#/components/") {
            let err = IrError::InvalidReference {
                reference: reference.to_string(),
                reason: "Reference must start with '#/components/'".to_string(),
                location: SourceLocation::new(),
            };
            tracing::error!("{}", err);
            return Err(err);
        }

        let parts: Vec<&str> = reference.split('/').collect();
        if parts.len() != 4 {
            let err = IrError::InvalidReference {
                reference: reference.to_string(),
                reason: "Reference must be in format '#/components/type/name'".to_string(),
                location: SourceLocation::new(),
            };
            tracing::error!("{}", err);
            return Err(err);
        }

        Ok((parts[2].to_string(), parts[3].to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ObjectOrReference, OpenApi, Schema};

    fn create_test_openapi() -> OpenApi {
        let yaml = r#"
openapi: 3.0.0
info:
  title: Test API
  version: 1.0.0
components:
  schemas:
    User:
      type: object
"#;
        openapi_nexus_parser::parse_content_yaml(yaml).unwrap()
    }

    #[test]
    fn test_utils_functions() {
        let schema_ref = ObjectOrReference::<Schema>::Ref {
            ref_path: "#/components/schemas/User".to_string(),
            summary: None,
            description: None,
        };
        assert!(Utils::is_reference(&schema_ref));

        let schema_obj = ObjectOrReference::Object(Schema::Boolean(
            openapi_nexus_spec::oas31::spec::BooleanSchema(false),
        ));
        assert!(!Utils::is_reference(&schema_obj));

        let name = Utils::get_reference_name(&schema_ref);
        assert_eq!(name, Some("#/components/schemas/User"));

        let name = Utils::get_reference_name(&schema_obj);
        assert_eq!(name, None);
    }

    #[test]
    fn test_extract_schema_refs() {
        let schema = utoipa::openapi::Schema::Object(utoipa::openapi::schema::Object::new());
        let refs = Utils::extract_schema_refs(&schema);
        assert_eq!(refs.len(), 0);
    }

    #[test]
    fn test_extract_schema_refs_with_properties() {
        let mut object_schema = utoipa::openapi::schema::Object::new();
        object_schema.properties.insert(
            "user".to_string(),
            utoipa::openapi::RefOr::Ref(utoipa::openapi::Ref::new("#/components/schemas/User")),
        );
        object_schema.properties.insert(
            "profile".to_string(),
            utoipa::openapi::RefOr::Ref(utoipa::openapi::Ref::new("#/components/schemas/Profile")),
        );

        let schema = utoipa::openapi::Schema::Object(object_schema);
        let refs = Utils::extract_schema_refs(&schema);
        assert_eq!(refs.len(), 2);
        assert!(refs.contains(&"#/components/schemas/User".to_string()));
        assert!(refs.contains(&"#/components/schemas/Profile".to_string()));
    }

    #[test]
    fn test_extract_schema_refs_nested() {
        let mut inner_schema = utoipa::openapi::schema::Object::new();
        inner_schema.properties.insert(
            "nested".to_string(),
            utoipa::openapi::RefOr::Ref(utoipa::openapi::Ref::new("#/components/schemas/Nested")),
        );

        let mut outer_schema = utoipa::openapi::schema::Object::new();
        outer_schema.properties.insert(
            "inner".to_string(),
            utoipa::openapi::RefOr::T(utoipa::openapi::Schema::Object(inner_schema)),
        );
        outer_schema.properties.insert(
            "direct".to_string(),
            utoipa::openapi::RefOr::Ref(utoipa::openapi::Ref::new("#/components/schemas/Direct")),
        );

        let schema = utoipa::openapi::Schema::Object(outer_schema);
        let refs = Utils::extract_schema_refs(&schema);
        assert_eq!(refs.len(), 2);
        assert!(refs.contains(&"#/components/schemas/Nested".to_string()));
        assert!(refs.contains(&"#/components/schemas/Direct".to_string()));
    }

    #[test]
    fn test_resolve_schema_reference_valid() {
        let openapi = create_test_openapi();
        let resolver = ReferenceResolver::new(&openapi);
        let result = resolver.resolve_schema_ref("#/components/schemas/User");
        assert!(result.is_ok());
    }

    #[test]
    fn test_resolve_schema_reference_invalid_format() {
        let openapi = create_test_openapi();
        let resolver = ReferenceResolver::new(&openapi);

        let result = resolver.resolve_schema_ref("invalid-ref");
        assert!(result.is_err());

        if let Err(IrError::InvalidReference { reference, .. }) = result {
            assert_eq!(reference, "invalid-ref");
        } else {
            panic!("Expected InvalidReference error");
        }
    }

    #[test]
    fn test_resolve_schema_reference_nonexistent() {
        let openapi = create_test_openapi();
        let resolver = ReferenceResolver::new(&openapi);

        let result = resolver.resolve_schema_ref("#/components/schemas/NonExistent");
        assert!(result.is_err());

        if let Err(IrError::UnresolvedReference { reference, .. }) = result {
            assert_eq!(reference, "#/components/schemas/NonExistent");
        } else {
            panic!("Expected UnresolvedReference error");
        }
    }

    #[test]
    fn test_resolve_schema_reference_wrong_component_type() {
        let openapi = create_test_openapi();
        let resolver = ReferenceResolver::new(&openapi);

        let result = resolver.resolve_schema_ref("#/components/responses/NotFound");
        assert!(result.is_err());

        if let Err(IrError::InvalidReference { reference, .. }) = result {
            assert_eq!(reference, "#/components/responses/NotFound");
        } else {
            panic!("Expected InvalidReference error");
        }
    }

    #[test]
    fn test_resolve_response_reference_valid() {
        let yaml = r#"
openapi: 3.0.0
info:
  title: Test API
  version: 1.0.0
components:
  responses:
    NotFound:
      description: Not Found
"#;
        let openapi: OpenApi = openapi_nexus_parser::parse_content_yaml(yaml).unwrap();
        let resolver = ReferenceResolver::new(&openapi);
        let result = resolver.resolve_response_ref("#/components/responses/NotFound");
        assert!(result.is_ok());
    }

    #[test]
    fn test_resolve_response_reference_nonexistent() {
        let openapi = create_test_openapi();
        let resolver = ReferenceResolver::new(&openapi);

        let result = resolver.resolve_response_ref("#/components/responses/NonExistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_parameter_reference() {
        let openapi = create_test_openapi();
        let resolver = ReferenceResolver::new(&openapi);

        // Parameters are not supported in utoipa Components, so this should always fail
        let result = resolver.resolve_parameter_ref("#/components/parameters/Limit");
        assert!(result.is_err());
    }

    #[test]
    fn test_is_external_reference() {
        let openapi = create_test_openapi();
        let resolver = ReferenceResolver::new(&openapi);

        assert!(resolver.is_external_reference("http://example.com/schema.json"));
        assert!(resolver.is_external_reference("https://api.example.com/schema.json"));
        assert!(!resolver.is_external_reference("#/components/schemas/User"));
        assert!(!resolver.is_external_reference("relative/path"));
        assert!(!resolver.is_external_reference("./local/schema.json"));
    }

    #[test]
    fn test_parse_component_reference_valid() {
        let openapi = create_test_openapi();
        let resolver = ReferenceResolver::new(&openapi);

        let result = resolver.parse_component_reference("#/components/schemas/User");
        assert!(result.is_ok());
        let (component_type, name) = result.unwrap();
        assert_eq!(component_type, "schemas");
        assert_eq!(name, "User");
    }

    #[test]
    fn test_parse_component_reference_responses() {
        let openapi = create_test_openapi();
        let resolver = ReferenceResolver::new(&openapi);

        let result = resolver.parse_component_reference("#/components/responses/NotFound");
        assert!(result.is_ok());
        let (component_type, name) = result.unwrap();
        assert_eq!(component_type, "responses");
        assert_eq!(name, "NotFound");
    }

    #[test]
    fn test_parse_component_reference_invalid_format() {
        let openapi = create_test_openapi();
        let resolver = ReferenceResolver::new(&openapi);

        let result = resolver.parse_component_reference("invalid-ref");
        assert!(result.is_err());

        if let Err(IrError::InvalidReference { reference, .. }) = result {
            assert_eq!(reference, "invalid-ref");
        } else {
            panic!("Expected InvalidReference error");
        }
    }

    #[test]
    fn test_parse_component_reference_wrong_prefix() {
        let openapi = create_test_openapi();
        let resolver = ReferenceResolver::new(&openapi);

        let result = resolver.parse_component_reference("/components/schemas/User");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_component_reference_too_short() {
        let openapi = create_test_openapi();
        let resolver = ReferenceResolver::new(&openapi);

        let result = resolver.parse_component_reference("#/components/schemas");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_component_reference_too_long() {
        let openapi = create_test_openapi();
        let resolver = ReferenceResolver::new(&openapi);

        let result = resolver.parse_component_reference("#/components/schemas/User/extra");
        assert!(result.is_err());
    }
}
