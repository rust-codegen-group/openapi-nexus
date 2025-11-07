//! Schema context for reference resolution and circular dependency tracking
//!
//! This module provides the `SchemaContext` struct that enables proper schema reference
//! resolution with circular dependency detection during TypeScript code generation.

use std::collections::{BTreeMap, HashSet};

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
}

impl<'a> SchemaContext<'a> {
    /// Create a new schema context with empty visited set
    pub fn new(
        schemas: &'a BTreeMap<String, RefOr<Schema>>,
        visited: &'a mut HashSet<String>,
    ) -> Self {
        Self {
            schemas,
            visited,
            depth: 0,
        }
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
