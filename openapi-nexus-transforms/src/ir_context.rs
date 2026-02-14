//! Intermediate representation context for transformation passes

use std::collections::HashMap;

use openapi_nexus_ir::OpenApi;

/// Analysis results from the IR layer
#[derive(Debug, Clone)]
pub struct SchemaAnalysis {
    pub dependencies: HashMap<String, Vec<String>>,
    pub circular_refs: Vec<Vec<String>>,
    pub schema_types: HashMap<String, String>,
}

/// Type mappings for code generation
#[derive(Debug, Clone)]
pub struct TypeMappings {
    pub openapi_to_language: HashMap<String, HashMap<String, String>>, // language -> (openapi_type -> lang_type)
    pub primitive_mappings: HashMap<String, String>,
}

/// Custom type definitions
#[derive(Debug, Clone)]
pub struct CustomTypes {
    pub types: HashMap<String, Vec<String>>, // type_name -> dependencies
}

/// Intermediate representation context
/// This context is passed through IR-level transformation passes
pub struct IrContext {
    pub openapi: OpenApi,
    pub schema_analysis: SchemaAnalysis,
    pub type_mappings: TypeMappings,
    pub custom_types: CustomTypes,
}

impl IrContext {
    /// Create a new IR context from an OpenAPI specification
    pub fn new(openapi: OpenApi) -> Self {
        Self {
            openapi,
            schema_analysis: SchemaAnalysis {
                dependencies: HashMap::new(),
                circular_refs: Vec::new(),
                schema_types: HashMap::new(),
            },
            type_mappings: TypeMappings {
                openapi_to_language: HashMap::new(),
                primitive_mappings: HashMap::new(),
            },
            custom_types: CustomTypes {
                types: HashMap::new(),
            },
        }
    }

    /// Get the OpenAPI specification
    pub fn openapi(&self) -> &OpenApi {
        &self.openapi
    }

    /// Get a mutable reference to the OpenAPI specification
    pub fn openapi_mut(&mut self) -> &mut OpenApi {
        &mut self.openapi
    }

    /// Get schema dependencies
    pub fn get_dependencies(&self, schema_name: &str) -> Option<&Vec<String>> {
        self.schema_analysis.dependencies.get(schema_name)
    }

    /// Check if a schema has circular references
    pub fn has_circular_refs(&self, schema_name: &str) -> bool {
        self.schema_analysis
            .circular_refs
            .iter()
            .any(|cycle| cycle.contains(&schema_name.to_string()))
    }
}

impl From<OpenApi> for IrContext {
    fn from(openapi: OpenApi) -> Self {
        Self::new(openapi)
    }
}
