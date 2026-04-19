# RFD 0002: OpenAPI Parsing with utoipa

## Summary

This RFD defines the OpenAPI parsing strategy using utoipa as the foundation for parsing OpenAPI 3.1 specifications. We leverage utoipa's robust type system and validation capabilities while building additional utilities for code generation purposes.

## Motivation

### Why utoipa?

1. **Native Rust Implementation**: Written in Rust with excellent type safety
2. **OpenAPI 3.1 Support**: Full support for OpenAPI 3.1 specification
3. **Comprehensive Type System**: Rich type definitions for all OpenAPI constructs
4. **Validation**: Built-in validation and error reporting
5. **Active Development**: Well-maintained with regular updates
6. **Integration**: Designed for Rust ecosystem integration

## Integration Strategy

### Direct Usage of utoipa Types

We use utoipa's types directly as our intermediate representation:

```rust
// From openapi-nexus-ir/src/lib.rs
pub use utoipa::openapi::{
    Components, ExternalDocs, Info, OpenApi, PathItem, Paths, RefOr, Response, Schema,
    SecurityRequirement, Server, Tag,
};
```

This approach provides:

- **Type Safety**: Compile-time guarantees about OpenAPI structure
- **Validation**: Automatic validation of OpenAPI specifications
- **Completeness**: Full coverage of OpenAPI 3.1 features
- **Maintenance**: Automatic updates with utoipa releases

### IR Utilities Layer

The `openapi-nexus-ir` crate provides utilities for working with utoipa types:

```rust
// openapi-nexus-ir/src/lib.rs
pub mod analysis;
pub mod traversal;
pub mod utils;

// Re-export key utoipa types for convenience
pub use utoipa::openapi::path::{Operation, Parameter};
pub use utoipa::openapi::{
    Components, ExternalDocs, Info, OpenApi, PathItem, Paths, RefOr, Response, Schema,
    SecurityRequirement, Server, Tag,
};
```

## Schema Traversal and Analysis

### Traversal Patterns

The IR layer provides common traversal patterns for OpenAPI specifications:

```rust
// openapi-nexus-ir/src/traversal.rs
pub trait OpenApiVisitor {
    fn visit_openapi(&mut self, openapi: &OpenApi) -> Result<(), Error>;
    fn visit_paths(&mut self, paths: &Paths) -> Result<(), Error>;
    fn visit_operation(&mut self, operation: &Operation) -> Result<(), Error>;
    fn visit_schema(&mut self, schema: &Schema) -> Result<(), Error>;
    // ... more visit methods
}
```

### Analysis Utilities

Common analysis operations are provided as utilities:

```rust
// openapi-nexus-ir/src/analysis.rs
pub struct SchemaAnalyzer {
    // Analysis state
}

impl SchemaAnalyzer {
    pub fn find_all_schemas(&self, openapi: &OpenApi) -> Vec<&Schema>;
    pub fn find_operation_schemas(&self, operation: &Operation) -> Vec<&Schema>;
    pub fn analyze_schema_dependencies(&self, schema: &Schema) -> Vec<String>;
    pub fn detect_circular_references(&self, openapi: &OpenApi) -> Vec<CircularRef>;
}
```

## Handling OpenAPI References ($ref)

### Reference Resolution Strategy

OpenAPI specifications often use `$ref` to reference other parts of the spec:

```yaml
components:
  schemas:
    User:
      type: object
      properties:
        id:
          type: integer
        profile:
          $ref: '#/components/schemas/UserProfile'
```

Our approach:

1. **Preserve References**: Keep `RefOr<T>` types to maintain reference information
2. **Lazy Resolution**: Resolve references only when needed for code generation
3. **Reference Tracking**: Track reference chains to detect cycles
4. **Error Reporting**: Provide clear error messages for broken references

### Reference Utilities

```rust
// openapi-nexus-ir/src/utils.rs
pub struct ReferenceResolver {
    openapi: &OpenApi,
}

impl ReferenceResolver {
    pub fn resolve_schema_ref(&self, reference: &str) -> Result<&Schema, Error>;
    pub fn resolve_response_ref(&self, reference: &str) -> Result<&Response, Error>;
    pub fn resolve_parameter_ref(&self, reference: &str) -> Result<&Parameter, Error>;
    pub fn is_external_reference(&self, reference: &str) -> bool;
}
```

## Error Handling and Validation

### Error Types

```rust
// openapi-nexus-parser/src/error.rs
#[derive(Debug, Snafu)]
pub enum ParseError {
    #[snafu(display("Invalid OpenAPI specification: {}", message))]
    InvalidSpec { message: String },
    
    #[snafu(display("Unsupported OpenAPI version: {}", version))]
    UnsupportedVersion { version: String },
    
    #[snafu(display("Circular reference detected: {}", path))]
    CircularReference { path: String },
    
    #[snafu(display("External reference not supported: {}", reference))]
    ExternalReference { reference: String },
    
    #[snafu(display("Schema validation failed: {}", details))]
    SchemaValidation { details: String },
}
```

### Validation Strategy

1. **Parse Validation**: utoipa handles basic OpenAPI structure validation
2. **Reference Validation**: Check all references are valid and resolvable
3. **Schema Validation**: Validate JSON Schema constraints
4. **Code Generation Validation**: Check for constructs that can't be generated

### Error Reporting

```rust
pub struct ParseResult {
    pub openapi: OpenApi,
    pub warnings: Vec<ParseWarning>,
    pub errors: Vec<ParseError>,
}

pub struct ParseWarning {
    pub message: String,
    pub location: SourceLocation,
    pub suggestion: Option<String>,
}
```

## Parser Implementation

### Main Parser Interface

```rust
// openapi-nexus-parser/src/parser.rs
pub struct OpenApiParser {
    config: ParserConfig,
}

impl OpenApiParser {
    pub fn new() -> Self {
        Self {
            config: ParserConfig::default(),
        }
    }
    
    pub fn parse_file<P: AsRef<Path>>(&self, path: P) -> Result<ParseResult, ParseError> {
        let content = std::fs::read_to_string(path)?;
        self.parse_content(&content)
    }
    
    pub fn parse_content(&self, content: &str) -> Result<ParseResult, ParseError> {
        // Parse YAML/JSON content
        // Validate OpenAPI structure
        // Resolve references
        // Return parse result
    }
}
```

### Configuration Options

```rust
#[derive(Debug, Clone)]
pub struct ParserConfig {
    pub allow_external_refs: bool,
    pub strict_mode: bool,
    pub validate_schemas: bool,
    pub max_reference_depth: usize,
}
```

## Performance Considerations

### Lazy Loading

- Parse only what's needed for code generation
- Defer reference resolution until required
- Cache resolved references

### Memory Management

- Use references where possible to avoid copying
- Implement reference counting for shared schemas
- Clean up unused parsed data

### Parallel Processing

- Parse multiple files in parallel when possible
- Parallel reference resolution for independent references
- Concurrent schema analysis

## Testing Strategy

### Unit Tests

- Test individual parser components
- Test reference resolution logic
- Test error handling paths

### Integration Tests

- Test with real OpenAPI specifications
- Test error recovery scenarios
- Test performance with large specs

### Test Fixtures

```rust
// test-fixtures/
//   ├── valid-specs/
//   │   ├── petstore.yaml
//   │   ├── github-api.yaml
//   │   └── complex-spec.yaml
//   ├── invalid-specs/
//   │   ├── circular-refs.yaml
//   │   ├── broken-refs.yaml
//   │   └── invalid-schema.yaml
//   └── edge-cases/
//       ├── empty-spec.yaml
//       ├── minimal-spec.yaml
//       └── external-refs.yaml
```

## Future Enhancements

### External Reference Support

Currently, we don't support external references. Future work could include:

- HTTP reference resolution
- File system reference resolution
- Caching of external references

### Custom Validation Rules

Allow users to define custom validation rules:

- Business logic validation
- Naming convention validation
- API design guideline validation

### Incremental Parsing

For large specifications, support incremental parsing:

- Parse only changed sections
- Update existing parsed data
- Maintain reference integrity

## Conclusion

Using utoipa as the foundation for OpenAPI parsing provides a solid, type-safe base for our code generator. The IR utilities layer adds the necessary functionality for code generation while maintaining compatibility with utoipa's design.

The reference resolution strategy balances simplicity with functionality, and the error handling approach provides clear feedback to users about specification issues.

## Related RFDs

- [RFD 0001: Overall Architecture and Design Philosophy](./0001-architecture-overview.md)
- [RFD 0004: Multi-Level Transformation Passes](./0004-transformation-passes.md)
- [RFD 0009: Error Handling and Diagnostics](./0009-error-handling.md)
