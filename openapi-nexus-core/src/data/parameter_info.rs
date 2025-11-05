//! Parameter information with raw OpenAPI schema

use serde::{Deserialize, Serialize};
use utoipa::openapi;
use utoipa::openapi::RefOr;
use utoipa::openapi::schema::Schema;

/// Parameter location for conflict resolution
///
/// Indicates where a parameter is used in an HTTP request.
///
/// # Variants
///
/// - `Path`: URL path segment parameter (e.g., `{id}` in `/users/{id}`)
/// - `Query`: Query string parameter (e.g., `?page=1&limit=10`)
/// - `Header`: HTTP header parameter (e.g., `Authorization: Bearer ...`)
/// - `Body`: Request body parameter (for POST/PUT/PATCH requests)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ParameterLocation {
    /// URL path segment parameter
    Path,
    /// Query string parameter
    Query,
    /// HTTP header parameter
    Header,
    /// Request body parameter
    Body,
}

impl From<openapi::path::ParameterIn> for ParameterLocation {
    fn from(param_in: openapi::path::ParameterIn) -> Self {
        match param_in {
            openapi::path::ParameterIn::Path => ParameterLocation::Path,
            openapi::path::ParameterIn::Query => ParameterLocation::Query,
            openapi::path::ParameterIn::Header => ParameterLocation::Header,
            openapi::path::ParameterIn::Cookie => ParameterLocation::Header, // Treat cookie as header
        }
    }
}

/// Parameter information for code generation
///
/// This struct represents a parameter extracted from an OpenAPI operation
/// after name conflict resolution. It contains both the original parameter
/// name (used for HTTP requests) and the resolved parameter name (used in
/// generated code).
///
/// # Fields
///
/// - `original_name`: The original parameter name from the OpenAPI spec,
///   used when constructing HTTP requests (e.g., query strings, headers, path segments)
/// - `param_name`: The resolved parameter name after conflict resolution,
///   used in generated code (may have location prefixes if conflicts exist)
/// - `schema`: The OpenAPI schema definition for the parameter (if available)
/// - `required`: Whether the parameter is required
/// - `deprecated`: Whether the parameter is deprecated
/// - `description`: Parameter description from the OpenAPI spec (if available)
/// - `default_value`: Default value for the parameter (if specified in the schema)
/// - `location`: The parameter location (path, query, header, or body)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterInfo {
    /// Original parameter name (for HTTP usage)
    ///
    /// This is the parameter name as specified in the OpenAPI spec.
    /// It is used when constructing HTTP requests (e.g., in query strings,
    /// headers, or path segments).
    pub original_name: String,
    /// Resolved parameter name (for code generation)
    ///
    /// This is the parameter name after conflict resolution. If multiple
    /// parameters from different locations have the same camelCase name,
    /// they will be prefixed with their location (e.g., "pathId", "queryId").
    /// This name is used in generated code.
    pub param_name: String,
    /// OpenAPI schema definition for the parameter
    ///
    /// Contains the type information and validation rules from the OpenAPI spec.
    /// May be `None` if not needed for template rendering.
    pub schema: Option<RefOr<Schema>>,
    /// Whether the parameter is required
    ///
    /// If `true`, the parameter must be provided when calling the API.
    pub required: bool,
    /// Whether the parameter is deprecated
    ///
    /// If `true`, the parameter is marked as deprecated in the OpenAPI spec
    /// and should be avoided in new code.
    pub deprecated: bool,
    /// Parameter description
    ///
    /// Human-readable description of the parameter from the OpenAPI spec.
    /// May be `None` if no description is provided.
    pub description: Option<String>,
    /// Default value for the parameter
    ///
    /// The default value extracted from the parameter's schema as a JSON value.
    /// Preserves the original type from the OpenAPI spec (string, number, boolean, null, array, object).
    /// May be `None` if no default value is specified.
    pub default_value: Option<serde_json::Value>,
    /// Parameter location
    ///
    /// Indicates where the parameter is used in the HTTP request:
    /// - `Path`: URL path segment (e.g., `/users/{id}`)
    /// - `Query`: Query string parameter (e.g., `?page=1`)
    /// - `Header`: HTTP header (e.g., `Authorization: Bearer ...`)
    /// - `Body`: Request body (for POST/PUT/PATCH requests)
    pub location: ParameterLocation,
}
