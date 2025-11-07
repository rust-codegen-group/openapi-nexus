//! Model import collection for API operations

use std::collections::{BTreeMap, BTreeSet};

use utoipa::openapi;

use crate::generator::response_transformer::ResponseTransformer;
use crate::templating::data::ApiImportStatement;
use openapi_nexus_core::data::OperationInfo;

/// Collected model dependencies for an API class
#[derive(Debug, Clone)]
pub struct ModelDependencies {
    pub type_names: BTreeSet<String>,
    pub function_names: BTreeSet<String>,
}

impl ModelDependencies {
    pub fn new() -> Self {
        Self {
            type_names: BTreeSet::new(),
            function_names: BTreeSet::new(),
        }
    }
}

impl Default for ModelDependencies {
    fn default() -> Self {
        Self::new()
    }
}

/// Collects model dependencies from API operations and builds import statements
#[derive(Debug, Clone)]
pub struct ModelImportCollector;

impl ModelImportCollector {
    /// Extract model name from request body schema reference
    ///
    /// Returns the model name if the operation has an `application/json` request body
    /// with a schema reference to `#/components/schemas/{name}`.
    pub fn extract_request_body_model_name(
        &self,
        operation: &openapi::path::Operation,
    ) -> Option<String> {
        let request_body = operation.request_body.as_ref()?;
        let json_content = request_body.content.get("application/json")?;
        let schema_ref = json_content.schema.as_ref()?;

        if let openapi::RefOr::Ref(reference) = schema_ref {
            reference
                .ref_location
                .strip_prefix("#/components/schemas/")
                .map(|name| name.to_string())
        } else {
            None
        }
    }

    /// Determine if a schema is an interface (needs ToJSON function)
    ///
    /// Interfaces are schemas with properties or additionalProperties.
    /// Type aliases (arrays, primitives, etc.) don't have ToJSON functions.
    ///
    /// Returns `true` for unresolved references (safe default).
    pub fn is_schema_interface(
        &self,
        model_name: &str,
        components: Option<&openapi::Components>,
    ) -> bool {
        let Some(components) = components else {
            // No components available, default to true to be safe
            return true;
        };

        let Some(schema_ref) = components.schemas.get(model_name) else {
            // Schema not found, default to true to be safe
            return true;
        };

        match schema_ref {
            openapi::RefOr::T(schema) => match schema {
                openapi::schema::Schema::Object(obj_schema) => {
                    // Interfaces have properties or additionalProperties
                    !obj_schema.properties.is_empty() || obj_schema.additional_properties.is_some()
                }
                _ => {
                    // Arrays and other types are type aliases
                    false
                }
            },
            openapi::RefOr::Ref(_) => {
                // Can't determine without resolving, default to true to be safe
                true
            }
        }
    }

    /// Collect all model dependencies from operations
    ///
    /// Collects model type names and function names (FromJSON/ToJSON) from:
    /// - Response models (all operations)
    /// - Request body models (operations with request bodies)
    pub fn collect_model_dependencies(
        &self,
        operations: &[OperationInfo],
        components: Option<&openapi::Components>,
        response_transformer: &ResponseTransformer,
    ) -> ModelDependencies {
        let mut dependencies = ModelDependencies::new();

        // Collect from response models
        for op_info in operations {
            if let Some(model_name) =
                response_transformer.compute_model_name(&op_info.method, &op_info.operation)
            {
                dependencies.type_names.insert(model_name.clone());
                dependencies
                    .function_names
                    .insert(format!("{}FromJSON", model_name));
            }
        }

        // Collect from request body models
        for op_info in operations {
            if let Some(model_name) = self.extract_request_body_model_name(&op_info.operation) {
                dependencies.type_names.insert(model_name.clone());

                // Only add ToJSON import if the schema is an interface
                // Type aliases don't have ToJSON functions
                if self.is_schema_interface(&model_name, components) {
                    dependencies
                        .function_names
                        .insert(format!("{}ToJSON", model_name));
                }
            }
        }

        dependencies
    }

    /// Build organized import statements from collected dependencies
    ///
    /// Groups imports by file, separates type imports from function imports,
    /// and ensures proper ordering (types first, then functions from the same file).
    pub fn build_model_imports(&self, dependencies: &ModelDependencies) -> Vec<ApiImportStatement> {
        // Group model imports by file and separate types from functions
        let mut models_by_file: BTreeMap<String, (Vec<String>, Vec<String>)> = BTreeMap::new();

        // Group type names by file
        for model_name in &dependencies.type_names {
            let filename = format!("../models/{}", model_name);
            let entry = models_by_file
                .entry(filename)
                .or_insert_with(|| (Vec::new(), Vec::new()));
            entry.0.push(model_name.clone());
        }

        // Group function names by file
        for func_name in &dependencies.function_names {
            // Extract model name from function name (e.g., "PetFromJSON" -> "Pet")
            if let Some(model_name) = func_name
                .strip_suffix("FromJSON")
                .or_else(|| func_name.strip_suffix("ToJSON"))
            {
                let filename = format!("../models/{}", model_name);
                if let Some(entry) = models_by_file.get_mut(&filename) {
                    entry.1.push(func_name.clone());
                } else {
                    // Function without a type import (shouldn't happen, but handle gracefully)
                    // This case is handled below when building imports
                }
            }
        }

        // Build import statements
        let mut imports = Vec::new();
        let mut processed_files = BTreeSet::new();

        for (file_path, (type_names, func_names)) in models_by_file {
            processed_files.insert(file_path.clone());
            if !type_names.is_empty() && !func_names.is_empty() {
                // Mixed imports: create single import with both types and functions
                let import_stmt = ApiImportStatement::new(file_path.clone())
                    .with_type_imports(type_names)
                    .with_imports(func_names);
                imports.push(import_stmt);
            } else if !type_names.is_empty() {
                // Only types (will auto-detect and use `import type { ... }`)
                let type_import =
                    ApiImportStatement::new(file_path.clone()).with_type_imports(type_names);
                imports.push(type_import);
            } else if !func_names.is_empty() {
                // Only functions
                let func_import =
                    ApiImportStatement::new(file_path.clone()).with_imports(func_names);
                imports.push(func_import);
            }
        }

        // Handle functions without corresponding type imports (edge case)
        for func_name in &dependencies.function_names {
            if let Some(model_name) = func_name
                .strip_suffix("FromJSON")
                .or_else(|| func_name.strip_suffix("ToJSON"))
            {
                let filename = format!("../models/{}", model_name);
                // Only add if we didn't already add this file
                if !processed_files.contains(&filename) {
                    imports.push(
                        ApiImportStatement::new(filename).with_import(func_name.clone(), None),
                    );
                }
            }
        }

        imports
    }
}
