//! Intermediate representation for OpenAPI code generation
//!
//! This crate provides utilities for working with OpenAPI types
//! as our intermediate representation, including traversal, analysis,
//! and transformation helpers.
//!
//! The IR layer follows the design principles outlined in RFD 0002, providing:
//! - Schema analysis and dependency tracking
//! - Reference resolution with circular reference detection
//! - Visitor pattern for traversing OpenAPI specifications
//! - Comprehensive error handling with source location tracking
//!
//! # Example
//!
//! ```rust
//! use openapi_nexus_ir::{SchemaAnalyzer, ReferenceResolver, OpenApiTraverser, OpenApi};
//!
//! // Parse an OpenAPI specification (e.g. from YAML or JSON)
//! let yaml = r#"
//! openapi: 3.0.0
//! info:
//!   title: Test API
//!   version: 1.0.0
//! components:
//!   schemas:
//!     User:
//!       type: object
//! "#;
//! let openapi: OpenApi = openapi_nexus_parser::parse_content_yaml(yaml).unwrap();
//!
//! // Analyze an OpenAPI specification
//! let analyzer = SchemaAnalyzer::new(&openapi);
//! let schemas = analyzer.find_all_schemas();
//! let circular_refs = analyzer.detect_circular_references().unwrap();
//!
//! // Resolve references
//! let resolver = ReferenceResolver::new(&openapi);
//! let _schema = resolver.resolve_schema_ref("#/components/schemas/User").unwrap();
//!
//! // Traverse with visitor pattern
//! struct MyVisitor;
//! impl openapi_nexus_ir::OpenApiVisitor for MyVisitor {
//!     type Error = openapi_nexus_ir::IrError;
//! }
//! let mut visitor = MyVisitor;
//! OpenApiTraverser::traverse(&openapi, &mut visitor).unwrap();
//! ```

pub mod analysis;
pub mod error;
pub mod traversal;
pub mod utils;

// Re-export key OpenAPI spec types for convenience
pub use openapi_nexus_spec::oas31::spec::{
    Components, ExternalDoc, Info, ObjectOrReference, ObjectSchema, Operation, Parameter, PathItem,
    RequestBody, Response, Schema, SecurityRequirement, SecurityScheme, Server, Tag,
};
pub use openapi_nexus_spec::OpenApiV31Spec;

// Type aliases for compatibility with utoipa API
pub type OpenApi = OpenApiV31Spec;
pub type Paths = std::collections::BTreeMap<String, PathItem>;
pub type RefOr<T> = ObjectOrReference<T>;
pub type ExternalDocs = ExternalDoc;

// ObjectOrReference uses ObjectOrReference::Object(T) for inline values
// and ObjectOrReference::Ref { ref_path, ... } instead of RefOr::Ref(Ref { ref_location, ... })

// Re-export IR types
pub use analysis::{Analyzer, CircularRef, SchemaAnalyzer};
pub use error::IrError;
pub use traversal::{OpenApiTraverser, OpenApiVisitor};
pub use utils::{ReferenceResolver, Utils};
