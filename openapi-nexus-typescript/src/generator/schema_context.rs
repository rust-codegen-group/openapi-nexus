//! Schema context for reference resolution and circular dependency tracking
//!
//! This module provides the `SchemaContext` struct that enables proper schema reference
//! resolution with circular dependency detection during TypeScript code generation.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::ast::TsTypeDefinition;
use utoipa::openapi::{RefOr, Schema};

/// Context for schema resolution with reference tracking
///
/// This context provides access to all available schemas and tracks visited schemas
/// to prevent circular dependency issues during reference resolution.
pub struct SchemaContext<'a> {
    /// All available schemas from components. Key is the original name of the schema.
    pub schemas: &'a BTreeMap<String, RefOr<Schema>>,
    /// Track visited schemas to prevent circular dependencies
    pub visited: &'a mut HashSet<String>,
    /// Current resolution depth (for debugging)
    pub depth: usize,
    /// Inline interfaces generated from nested inline objects. Key is the TypeScript name (PascalCase).
    pub inline_interfaces: &'a mut HashMap<String, TsTypeDefinition>,
    /// Enum discriminator values for tagged enum variants. Key is interface name, value is (property_name, enum_value).
    pub enum_discriminators: &'a mut HashMap<String, (String, String)>,
}

impl<'a> SchemaContext<'a> {
    /// Create a new schema context with empty visited set
    pub fn new(
        schemas: &'a BTreeMap<String, RefOr<Schema>>,
        visited: &'a mut HashSet<String>,
        inline_interfaces: &'a mut HashMap<String, TsTypeDefinition>,
        enum_discriminators: &'a mut HashMap<String, (String, String)>,
    ) -> Self {
        Self {
            schemas,
            visited,
            depth: 0,
            inline_interfaces,
            enum_discriminators,
        }
    }

    /// Register an enum discriminator for a tagged enum variant interface
    pub fn register_enum_discriminator(
        &mut self,
        interface_name: String,
        property_name: String,
        enum_value: String,
    ) {
        self.enum_discriminators
            .insert(interface_name, (property_name, enum_value));
    }

    /// Get enum discriminator info for an interface
    pub fn get_enum_discriminator(&self, interface_name: &str) -> Option<&(String, String)> {
        self.enum_discriminators.get(interface_name)
    }

    /// Register a generated inline interface
    pub fn register_inline_interface(&mut self, ts_name: String, type_def: TsTypeDefinition) {
        self.inline_interfaces.insert(ts_name, type_def);
    }

    /// Check if an inline interface with the given name already exists
    pub fn has_inline_interface(&self, ts_name: &str) -> bool {
        self.inline_interfaces.contains_key(ts_name)
    }

    /// Get all generated inline interfaces
    pub fn get_inline_interfaces(&self) -> &HashMap<String, TsTypeDefinition> {
        self.inline_interfaces
    }

    /// Check if a schema has been visited (circular dependency detection)
    pub fn is_visited(&self, schema_name: &str) -> bool {
        self.visited.contains(schema_name)
    }

    /// Mark a schema as visited
    pub fn mark_visited(&mut self, schema_name: String) {
        self.visited.insert(schema_name);
    }

    /// Remove a schema from visited set (cleanup after resolution)
    pub fn unmark_visited(&mut self, schema_name: &str) {
        self.visited.remove(schema_name);
    }

    /// Increment depth for debugging
    pub fn increment_depth(&mut self) {
        self.depth += 1;
    }

    /// Decrement depth for debugging
    pub fn decrement_depth(&mut self) {
        self.depth -= 1;
    }
}
