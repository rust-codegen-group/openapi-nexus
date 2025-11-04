//! Analysis utilities for OpenAPI specifications

use utoipa::openapi::request_body::RequestBody;
use utoipa::openapi::security::SecurityScheme;
use utoipa::openapi::{OpenApi, RefOr, Schema, path::Operation};

use crate::error::IrError;
use crate::utils::Utils;

/// Information about a circular reference detected in the schema
#[derive(Debug, Clone)]
pub struct CircularRef {
    /// The path of references that forms the cycle
    pub path: Vec<String>,
    /// The name of the schema where the cycle starts
    pub cycle_start: String,
}

/// Analyze an OpenAPI specification and extract useful information
pub struct Analyzer;

impl Analyzer {
    /// Get all schemas from the OpenAPI specification
    pub fn get_all_schemas(openapi: &OpenApi) -> Vec<(&String, &utoipa::openapi::RefOr<Schema>)> {
        openapi
            .components
            .as_ref()
            .map(|components| components.schemas.iter().collect())
            .unwrap_or_default()
    }

    /// Get all operations from the OpenAPI specification
    pub fn get_all_operations(
        openapi: &OpenApi,
    ) -> Vec<(&String, &utoipa::openapi::path::Operation)> {
        openapi
            .paths
            .paths
            .iter()
            .flat_map(|(path, path_item)| {
                // Access operations through individual HTTP methods
                let mut operations = Vec::new();
                if let Some(op) = &path_item.get {
                    operations.push((path, op));
                }
                if let Some(op) = &path_item.post {
                    operations.push((path, op));
                }
                if let Some(op) = &path_item.put {
                    operations.push((path, op));
                }
                if let Some(op) = &path_item.delete {
                    operations.push((path, op));
                }
                if let Some(op) = &path_item.patch {
                    operations.push((path, op));
                }
                if let Some(op) = &path_item.head {
                    operations.push((path, op));
                }
                if let Some(op) = &path_item.options {
                    operations.push((path, op));
                }
                if let Some(op) = &path_item.trace {
                    operations.push((path, op));
                }
                operations
            })
            .collect()
    }

    /// Get all response schemas from the OpenAPI specification
    pub fn get_all_responses(
        openapi: &OpenApi,
    ) -> Vec<(&String, &utoipa::openapi::RefOr<utoipa::openapi::Response>)> {
        openapi
            .components
            .as_ref()
            .map(|components| components.responses.iter().collect())
            .unwrap_or_default()
    }

    /// Get all parameters from the OpenAPI specification
    /// Note: Parameters are typically defined inline in operations, not in components
    pub fn get_all_parameters(
        _openapi: &OpenApi,
    ) -> Vec<(
        &String,
        &utoipa::openapi::RefOr<utoipa::openapi::path::Parameter>,
    )> {
        // TODO: Extract parameters from operations
        Vec::new()
    }

    /// Get all security schemes from the OpenAPI specification
    pub fn get_all_security_schemes(openapi: &OpenApi) -> Vec<(&String, &SecurityScheme)> {
        openapi
            .components
            .as_ref()
            .map(|components| components.security_schemes.iter().collect())
            .unwrap_or_default()
    }
}

/// Advanced schema analysis with dependency tracking and circular reference detection
pub struct SchemaAnalyzer<'a> {
    openapi: &'a OpenApi,
}

impl<'a> SchemaAnalyzer<'a> {
    /// Create a new schema analyzer for the given OpenAPI specification
    pub fn new(openapi: &'a OpenApi) -> Self {
        Self { openapi }
    }

    /// Find all schemas in the OpenAPI specification
    pub fn find_all_schemas(&self) -> Vec<(&String, &RefOr<Schema>)> {
        Analyzer::get_all_schemas(self.openapi)
    }

    /// Find all schemas referenced by an operation
    pub fn find_operation_schemas(&self, operation: &Operation) -> Result<Vec<&Schema>, IrError> {
        let mut schemas = Vec::new();

        // Extract schemas from request body
        if let Some(request_body) = &operation.request_body {
            schemas.extend(self.extract_schemas_from_request_body(request_body)?);
        }

        // Extract schemas from responses
        for response in operation.responses.responses.values() {
            if let RefOr::T(response) = response {
                schemas.extend(self.extract_schemas_from_response(response)?);
            }
        }

        // Convert schema names to actual schema references
        let mut schema_refs = Vec::new();
        if let Some(components) = &self.openapi.components {
            for schema_name in schemas {
                if let Some(RefOr::T(schema)) = components.schemas.get(&schema_name) {
                    schema_refs.push(schema);
                }
            }
        }

        Ok(schema_refs)
    }

    /// Extract schema names from a request body
    fn extract_schemas_from_request_body(
        &self,
        request_body: &RequestBody,
    ) -> Result<Vec<String>, IrError> {
        let mut schemas = Vec::new();

        for media_type in request_body.content.values() {
            if let Some(schema_ref) = &media_type.schema {
                match schema_ref {
                    RefOr::Ref(ref_ref) => {
                        if ref_ref.ref_location.starts_with("#/components/schemas/") {
                            let schema_name = ref_ref
                                .ref_location
                                .trim_start_matches("#/components/schemas/")
                                .to_string();
                            schemas.push(schema_name);
                        }
                    }
                    RefOr::T(schema) => {
                        schemas.extend(Utils::extract_schema_refs(schema));
                    }
                }
            }
        }

        Ok(schemas)
    }

    /// Extract schema names from a response
    fn extract_schemas_from_response(
        &self,
        response: &utoipa::openapi::Response,
    ) -> Result<Vec<String>, IrError> {
        let mut schemas = Vec::new();

        for (_, media_type) in &response.content {
            if let Some(schema_ref) = &media_type.schema {
                match schema_ref {
                    RefOr::Ref(ref_ref) => {
                        if ref_ref.ref_location.starts_with("#/components/schemas/") {
                            let schema_name = ref_ref
                                .ref_location
                                .trim_start_matches("#/components/schemas/")
                                .to_string();
                            schemas.push(schema_name);
                        }
                    }
                    RefOr::T(schema) => {
                        schemas.extend(Utils::extract_schema_refs(schema));
                    }
                }
            }
        }

        Ok(schemas)
    }

    /// Analyze dependencies for a specific schema
    pub fn analyze_schema_dependencies(&self, schema_name: &str) -> Result<Vec<String>, IrError> {
        let mut dependencies = Vec::new();
        let mut visited = std::collections::HashSet::new();

        self.collect_schema_dependencies_recursive(schema_name, &mut dependencies, &mut visited)?;

        Ok(dependencies)
    }

    /// Recursively collect all dependencies of a schema
    fn collect_schema_dependencies_recursive(
        &self,
        schema_name: &str,
        dependencies: &mut Vec<String>,
        visited: &mut std::collections::HashSet<String>,
    ) -> Result<(), IrError> {
        if visited.contains(schema_name) {
            return Ok(());
        }

        visited.insert(schema_name.to_string());

        let direct_deps = self.get_schema_dependencies(schema_name)?;

        for dep in direct_deps {
            if !dependencies.contains(&dep) {
                dependencies.push(dep.clone());
            }
            // Recursively collect dependencies of dependencies
            self.collect_schema_dependencies_recursive(&dep, dependencies, visited)?;
        }

        Ok(())
    }

    /// Detect circular references in the schema definitions
    pub fn detect_circular_references(&self) -> Result<Vec<CircularRef>, IrError> {
        let mut circular_refs = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut recursion_stack = std::collections::HashSet::new();

        // Get all schema names
        let schema_names: Vec<String> = self
            .openapi
            .components
            .as_ref()
            .map(|components| components.schemas.keys().cloned().collect())
            .unwrap_or_default();

        for schema_name in schema_names {
            if !visited.contains(&schema_name) {
                let mut path = Vec::new();
                if let Some(circular_ref) = self.detect_circular_ref_from_schema(
                    &schema_name,
                    &mut visited,
                    &mut recursion_stack,
                    &mut path,
                )? {
                    circular_refs.push(circular_ref);
                }
            }
        }

        Ok(circular_refs)
    }

    /// Helper method to detect circular references starting from a specific schema
    fn detect_circular_ref_from_schema(
        &self,
        schema_name: &str,
        visited: &mut std::collections::HashSet<String>,
        recursion_stack: &mut std::collections::HashSet<String>,
        path: &mut Vec<String>,
    ) -> Result<Option<CircularRef>, IrError> {
        // If we're already in the recursion stack, we found a cycle
        if recursion_stack.contains(schema_name) {
            // Find the start of the cycle
            let cycle_start = schema_name.to_string();
            let cycle_path = path.clone();
            return Ok(Some(CircularRef {
                cycle_start,
                path: cycle_path,
            }));
        }

        // If already visited, no cycle from this path
        if visited.contains(schema_name) {
            return Ok(None);
        }

        // Mark as visited and add to recursion stack
        visited.insert(schema_name.to_string());
        recursion_stack.insert(schema_name.to_string());
        path.push(schema_name.to_string());

        // Get schema dependencies
        let dependencies = self.get_schema_dependencies(schema_name)?;

        // Check each dependency for cycles
        for dep_ref in dependencies {
            if let Some(circular_ref) =
                self.detect_circular_ref_from_schema(&dep_ref, visited, recursion_stack, path)?
            {
                return Ok(Some(circular_ref));
            }
        }

        // Remove from recursion stack and path
        recursion_stack.remove(schema_name);
        path.pop();

        Ok(None)
    }

    /// Get direct dependencies of a schema
    fn get_schema_dependencies(&self, schema_name: &str) -> Result<Vec<String>, IrError> {
        let components = self.openapi.components.as_ref().ok_or_else(|| {
            let err = IrError::AnalysisError {
                message: "No components found".to_string(),
                location: openapi_nexus_common::SourceLocation::new(),
            };
            tracing::error!("{}", err);
            err
        })?;

        let schema = components.schemas.get(schema_name).ok_or_else(|| {
            let err = IrError::AnalysisError {
                message: format!("Schema '{}' not found", schema_name),
                location: openapi_nexus_common::SourceLocation::new(),
            };
            tracing::error!("{}", err);
            err
        })?;

        let mut dependencies = Vec::new();

        match schema {
            RefOr::T(schema) => {
                dependencies.extend(Utils::extract_schema_refs(schema));
            }
            RefOr::Ref(ref_ref) => {
                dependencies.push(ref_ref.ref_location.clone());
            }
        }

        // Convert references to schema names
        let schema_names: Vec<String> = dependencies
            .into_iter()
            .filter_map(|ref_location| {
                if ref_location.starts_with("#/components/schemas/") {
                    Some(
                        ref_location
                            .trim_start_matches("#/components/schemas/")
                            .to_string(),
                    )
                } else {
                    None
                }
            })
            .collect();

        Ok(schema_names)
    }
}

#[cfg(test)]
mod tests {
    use utoipa::openapi::schema::Object;
    use utoipa::openapi::{Components, Info, OpenApi, Paths, RefOr, Schema};

    use super::*;

    fn create_test_openapi() -> OpenApi {
        let mut components = Components::new();

        // Add a simple schema
        let user_schema = Object::new();
        components
            .schemas
            .insert("User".to_string(), RefOr::T(Schema::Object(user_schema)));

        let info = Info::new("Test API", "1.0.0");
        let paths = Paths::new();

        let mut openapi = OpenApi::new(info, paths);
        openapi.components = Some(components);
        openapi
    }

    #[test]
    fn test_find_all_schemas() {
        let openapi = create_test_openapi();
        let analyzer = SchemaAnalyzer::new(&openapi);

        let schemas = analyzer.find_all_schemas();
        assert_eq!(schemas.len(), 1);
        assert_eq!(schemas[0].0, "User");
    }

    #[test]
    fn test_analyzer_get_all_schemas() {
        let openapi = create_test_openapi();
        let schemas = Analyzer::get_all_schemas(&openapi);

        assert_eq!(schemas.len(), 1);
        assert_eq!(schemas[0].0, "User");
    }

    #[test]
    fn test_analyzer_get_all_operations() {
        let openapi = create_test_openapi();
        let operations = Analyzer::get_all_operations(&openapi);

        // Empty OpenAPI has no operations
        assert_eq!(operations.len(), 0);
    }

    #[test]
    fn test_analyzer_get_all_responses() {
        let openapi = create_test_openapi();
        let responses = Analyzer::get_all_responses(&openapi);

        // Empty OpenAPI has no responses
        assert_eq!(responses.len(), 0);
    }

    #[test]
    fn test_analyzer_get_all_security_schemes() {
        let openapi = create_test_openapi();
        let security_schemes = Analyzer::get_all_security_schemes(&openapi);

        // Empty OpenAPI has no security schemes
        assert_eq!(security_schemes.len(), 0);
    }

    #[test]
    fn test_schema_analyzer_new() {
        let openapi = create_test_openapi();
        let analyzer = SchemaAnalyzer::new(&openapi);

        // Just test that it can be created
        let schemas = analyzer.find_all_schemas();
        assert_eq!(schemas.len(), 1);
    }

    #[test]
    fn test_find_operation_schemas() {
        let openapi = create_test_openapi();
        let analyzer = SchemaAnalyzer::new(&openapi);

        // Create a mock operation (simplified)
        let operation = utoipa::openapi::path::Operation::new();
        let result = analyzer.find_operation_schemas(&operation);

        // Should return empty vec for now (simplified implementation)
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[test]
    fn test_analyze_schema_dependencies() {
        let openapi = create_test_openapi();
        let analyzer = SchemaAnalyzer::new(&openapi);

        let result = analyzer.analyze_schema_dependencies("User");

        // Should return empty vec for now (simplified implementation)
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[test]
    fn test_detect_circular_references() {
        let openapi = create_test_openapi();
        let analyzer = SchemaAnalyzer::new(&openapi);

        let result = analyzer.detect_circular_references();

        // Should return empty vec for now (simplified implementation)
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[test]
    fn test_analyzer_get_all_schemas_empty() {
        let openapi = OpenApi::new(Info::new("Test API", "1.0.0"), Paths::new());
        let schemas = Analyzer::get_all_schemas(&openapi);
        assert_eq!(schemas.len(), 0);
    }

    #[test]
    fn test_analyzer_get_all_schemas_multiple() {
        let mut components = Components::new();

        // Add multiple schemas
        let user_schema = Object::new();
        components
            .schemas
            .insert("User".to_string(), RefOr::T(Schema::Object(user_schema)));

        let product_schema = Object::new();
        components.schemas.insert(
            "Product".to_string(),
            RefOr::T(Schema::Object(product_schema)),
        );

        let order_schema = Object::new();
        components
            .schemas
            .insert("Order".to_string(), RefOr::T(Schema::Object(order_schema)));

        let mut openapi = OpenApi::new(Info::new("Test API", "1.0.0"), Paths::new());
        openapi.components = Some(components);

        let schemas = Analyzer::get_all_schemas(&openapi);
        assert_eq!(schemas.len(), 3);

        let schema_names: Vec<String> = schemas.iter().map(|(name, _)| (*name).clone()).collect();
        assert!(schema_names.contains(&"User".to_string()));
        assert!(schema_names.contains(&"Product".to_string()));
        assert!(schema_names.contains(&"Order".to_string()));
    }

    #[test]
    fn test_schema_analyzer_analyze_schema_dependencies_none() {
        let openapi = create_test_openapi();
        let analyzer = SchemaAnalyzer::new(&openapi);

        let dependencies = analyzer.analyze_schema_dependencies("User").unwrap();
        assert_eq!(dependencies.len(), 0); // User schema has no dependencies
    }

    #[test]
    fn test_schema_analyzer_analyze_schema_dependencies_with_refs() {
        let mut openapi = create_test_openapi();
        let mut components = openapi.components.take().unwrap_or_default();

        // Add schemas with references
        let mut profile_schema = Object::new();
        profile_schema
            .properties
            .insert("name".to_string(), RefOr::T(Schema::Object(Object::new())));
        components.schemas.insert(
            "Profile".to_string(),
            RefOr::T(Schema::Object(profile_schema)),
        );

        let mut user_schema = Object::new();
        user_schema.properties.insert(
            "profile".to_string(),
            RefOr::Ref(utoipa::openapi::Ref::new("#/components/schemas/Profile")),
        );
        components
            .schemas
            .insert("User".to_string(), RefOr::T(Schema::Object(user_schema)));

        openapi.components = Some(components);

        let analyzer = SchemaAnalyzer::new(&openapi);
        let dependencies = analyzer.analyze_schema_dependencies("User").unwrap();
        assert_eq!(dependencies.len(), 1);
        assert_eq!(dependencies[0], "Profile");
    }

    #[test]
    fn test_schema_analyzer_analyze_schema_dependencies_nonexistent() {
        let openapi = create_test_openapi();
        let analyzer = SchemaAnalyzer::new(&openapi);

        let result = analyzer.analyze_schema_dependencies("NonExistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_detect_circular_references_complex() {
        let mut components = Components::new();

        // Create a more complex circular reference: A -> B -> C -> A
        let mut schema_a = Object::new();
        schema_a.properties.insert(
            "b".to_string(),
            RefOr::Ref(utoipa::openapi::Ref::new("#/components/schemas/B")),
        );
        components
            .schemas
            .insert("A".to_string(), RefOr::T(Schema::Object(schema_a)));

        let mut schema_b = Object::new();
        schema_b.properties.insert(
            "c".to_string(),
            RefOr::Ref(utoipa::openapi::Ref::new("#/components/schemas/C")),
        );
        components
            .schemas
            .insert("B".to_string(), RefOr::T(Schema::Object(schema_b)));

        let mut schema_c = Object::new();
        schema_c.properties.insert(
            "a".to_string(),
            RefOr::Ref(utoipa::openapi::Ref::new("#/components/schemas/A")),
        );
        components
            .schemas
            .insert("C".to_string(), RefOr::T(Schema::Object(schema_c)));

        let mut openapi = OpenApi::new(Info::new("Test API", "1.0.0"), Paths::new());
        openapi.components = Some(components);

        let analyzer = SchemaAnalyzer::new(&openapi);
        let circular_refs = analyzer.detect_circular_references().unwrap();
        assert_eq!(circular_refs.len(), 1);
        assert_eq!(circular_refs[0].cycle_start, "A");
        assert!(circular_refs[0].path.contains(&"A".to_string()));
        assert!(circular_refs[0].path.contains(&"B".to_string()));
        assert!(circular_refs[0].path.contains(&"C".to_string()));
    }

    #[test]
    fn test_detect_circular_references_multiple_cycles() {
        let mut components = Components::new();

        // Create two separate circular references: A -> B -> A and C -> D -> C
        let mut schema_a = Object::new();
        schema_a.properties.insert(
            "b".to_string(),
            RefOr::Ref(utoipa::openapi::Ref::new("#/components/schemas/B")),
        );
        components
            .schemas
            .insert("A".to_string(), RefOr::T(Schema::Object(schema_a)));

        let mut schema_b = Object::new();
        schema_b.properties.insert(
            "a".to_string(),
            RefOr::Ref(utoipa::openapi::Ref::new("#/components/schemas/A")),
        );
        components
            .schemas
            .insert("B".to_string(), RefOr::T(Schema::Object(schema_b)));

        let mut schema_c = Object::new();
        schema_c.properties.insert(
            "d".to_string(),
            RefOr::Ref(utoipa::openapi::Ref::new("#/components/schemas/D")),
        );
        components
            .schemas
            .insert("C".to_string(), RefOr::T(Schema::Object(schema_c)));

        let mut schema_d = Object::new();
        schema_d.properties.insert(
            "c".to_string(),
            RefOr::Ref(utoipa::openapi::Ref::new("#/components/schemas/C")),
        );
        components
            .schemas
            .insert("D".to_string(), RefOr::T(Schema::Object(schema_d)));

        let mut openapi = OpenApi::new(Info::new("Test API", "1.0.0"), Paths::new());
        openapi.components = Some(components);

        let analyzer = SchemaAnalyzer::new(&openapi);
        let circular_refs = analyzer.detect_circular_references().unwrap();
        assert_eq!(circular_refs.len(), 2);
    }
}
