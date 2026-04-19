# RFD 0004: Multi-Level Transformation Passes

> **Status: Superseded.** This RFD described a standalone `openapi-nexus-transforms`
> crate with `OpenApiTransformPass` / `IrTransformPass` / `AstTransformPass` traits
> and a `TransformPipeline`. That crate and those traits no longer exist — the
> transform pipeline was replaced by direct lowering from the parsed OpenAPI spec
> into an IR (see `openapi-nexus-ir::lower`). Schema analysis, reference resolution,
> and tagged-enum classification are now IR-internal operations rather than
> independently-registered passes. This document is retained for historical context
> only. A future RFD should formalize the current IR-based pipeline.

## Summary

This RFD defines the multi-level transformation pass architecture that operates on OpenAPI specifications, intermediate representations, and language-specific ASTs. The system enables powerful code generation optimizations and customizations through a composable pipeline of transformation passes.

## Motivation

### Why Multi-Level Transformations?

1. **Separation of Concerns**: Different types of transformations operate at different levels
2. **Reusability**: Passes can be reused across different languages
3. **Composability**: Passes can be combined in different ways
4. **Extensibility**: Easy to add new transformation passes
5. **Debugging**: Easier to debug when transformations are isolated

### Transformation Levels

1. **OpenAPI Level**: Operate on the raw OpenAPI specification
2. **IR Level**: Operate on intermediate representation utilities
3. **AST Level**: Operate on language-specific ASTs

## Three-Level Architecture

### Level 1: OpenAPI-Level Transforms

These transforms operate on the utoipa `OpenApi` structure:

```rust
// openapi-nexus-transforms/src/passes.rs
pub trait OpenApiTransformPass {
    fn name(&self) -> &str;
    fn transform(&self, openapi: &mut OpenApi) -> Result<(), TransformError>;
    fn dependencies(&self) -> Vec<&str>;
}
```

**Example Passes**:

- **Reference Resolution**: Resolve all `$ref` references
- **Schema Normalization**: Normalize schema structures
- **Path Normalization**: Standardize path patterns
- **Security Analysis**: Analyze security requirements
- **Validation**: Validate OpenAPI specification

### Level 2: IR-Level Transforms

These transforms operate on the intermediate representation utilities:

```rust
pub trait IrTransformPass {
    fn name(&self) -> &str;
    fn transform(&self, ir: &mut IrContext) -> Result<(), TransformError>;
    fn dependencies(&self) -> Vec<&str>;
}

pub struct IrContext {
    pub openapi: OpenApi,
    pub schema_analysis: SchemaAnalysis,
    pub type_mappings: TypeMappings,
    pub custom_types: CustomTypes,
}
```

**Example Passes**:

- **Type Inference**: Infer types from OpenAPI schemas
- **Dependency Analysis**: Analyze schema dependencies
- **Circular Reference Detection**: Detect and handle circular references
- **Semantic Analysis**: Perform semantic analysis on the specification
- **Optimization**: Optimize for code generation

### Level 3: AST-Level Transforms

These transforms operate on language-specific ASTs:

```rust
pub trait AstTransformPass<T> {
    fn name(&self) -> &str;
    fn transform(&self, ast: &mut T) -> Result<(), TransformError>;
    fn dependencies(&self) -> Vec<&str>;
}
```

**Example Passes**:

- **Naming Conventions**: Apply language-specific naming conventions
- **Code Style**: Apply code style rules
- **Optimization**: Optimize generated code
- **Validation**: Validate generated AST
- **Customization**: Apply user customizations

## Transform Pipeline

### Pipeline Composition

```rust
pub struct TransformPipeline {
    openapi_passes: Vec<Box<dyn OpenApiTransformPass>>,
    ir_passes: Vec<Box<dyn IrTransformPass>>,
    ast_passes: HashMap<String, Vec<Box<dyn AstTransformPass<dyn Any>>>>,
}

impl TransformPipeline {
    pub fn new() -> Self {
        Self {
            openapi_passes: Vec::new(),
            ir_passes: Vec::new(),
            ast_passes: HashMap::new(),
        }
    }
    
    pub fn add_openapi_pass<P: OpenApiTransformPass + 'static>(mut self, pass: P) -> Self {
        self.openapi_passes.push(Box::new(pass));
        self
    }
    
    pub fn add_ir_pass<P: IrTransformPass + 'static>(mut self, pass: P) -> Self {
        self.ir_passes.push(Box::new(pass));
        self
    }
    
    pub fn add_ast_pass<T, P: AstTransformPass<T> + 'static>(mut self, language: &str, pass: P) -> Self {
        self.ast_passes.entry(language.to_string())
            .or_insert_with(Vec::new)
            .push(Box::new(pass));
        self
    }
}
```

### Pass Execution

```rust
impl TransformPipeline {
    pub fn execute(&self, mut openapi: OpenApi, language: &str) -> Result<OpenApi, TransformError> {
        // Execute OpenAPI-level passes
        for pass in &self.openapi_passes {
            pass.transform(&mut openapi)?;
        }
        
        // Create IR context
        let mut ir_context = IrContext::from_openapi(openapi);
        
        // Execute IR-level passes
        for pass in &self.ir_passes {
            pass.transform(&mut ir_context)?;
        }
        
        // Generate AST
        let mut ast = self.generate_ast(&ir_context, language)?;
        
        // Execute AST-level passes
        if let Some(ast_passes) = self.ast_passes.get(language) {
            for pass in ast_passes {
                pass.transform(&mut ast)?;
            }
        }
        
        Ok(ir_context.openapi)
    }
}
```

## Pass Dependencies and Ordering

### Dependency Resolution

```rust
pub struct PassDependencyResolver {
    passes: Vec<PassInfo>,
}

#[derive(Debug, Clone)]
pub struct PassInfo {
    pub name: String,
    pub dependencies: Vec<String>,
    pub level: PassLevel,
}

#[derive(Debug, Clone)]
pub enum PassLevel {
    OpenApi,
    Ir,
    Ast(String), // Language name
}
```

### Topological Sorting

```rust
impl PassDependencyResolver {
    pub fn resolve_order(&self) -> Result<Vec<PassInfo>, TransformError> {
        // Implement topological sort algorithm
        // Handle circular dependencies
        // Return ordered list of passes
    }
}
```

## Built-in Transformation Passes

### OpenAPI-Level Passes

#### Reference Resolution Pass

```rust
pub struct ReferenceResolutionPass {
    max_depth: usize,
}

impl OpenApiTransformPass for ReferenceResolutionPass {
    fn name(&self) -> &str { "reference-resolution" }
    
    fn transform(&self, openapi: &mut OpenApi) -> Result<(), TransformError> {
        // Resolve all $ref references
        // Handle circular references
        // Validate resolved references
    }
    
    fn dependencies(&self) -> Vec<&str> { vec![] }
}
```

#### Schema Normalization Pass

```rust
pub struct SchemaNormalizationPass {
    normalize_arrays: bool,
    normalize_objects: bool,
}

impl OpenApiTransformPass for SchemaNormalizationPass {
    fn name(&self) -> &str { "schema-normalization" }
    
    fn transform(&self, openapi: &mut OpenApi) -> Result<(), TransformError> {
        // Normalize schema structures
        // Standardize property names
        // Handle nullable fields
    }
    
    fn dependencies(&self) -> Vec<&str> { vec!["reference-resolution"] }
}
```

### IR-Level Passes

#### Type Inference Pass

```rust
pub struct TypeInferencePass {
    strict_mode: bool,
}

impl IrTransformPass for TypeInferencePass {
    fn name(&self) -> &str { "type-inference" }
    
    fn transform(&self, ir: &mut IrContext) -> Result<(), TransformError> {
        // Infer types from OpenAPI schemas
        // Create type mappings
        // Handle complex types
    }
    
    fn dependencies(&self) -> Vec<&str> { vec!["reference-resolution", "schema-normalization"] }
}
```

#### Dependency Analysis Pass

```rust
pub struct DependencyAnalysisPass;

impl IrTransformPass for DependencyAnalysisPass {
    fn name(&self) -> &str { "dependency-analysis" }
    
    fn transform(&self, ir: &mut IrContext) -> Result<(), TransformError> {
        // Analyze schema dependencies
        // Create dependency graph
        // Detect circular dependencies
    }
    
    fn dependencies(&self) -> Vec<&str> { vec!["type-inference"] }
}
```

### AST-Level Passes

#### Naming Convention Pass

```rust
pub struct NamingConventionPass {
    convention: NamingConvention,
}

#[derive(Debug, Clone)]
pub enum NamingConvention {
    CamelCase,
    PascalCase,
    SnakeCase,
    KebabCase,
}

impl AstTransformPass<TsNode> for NamingConventionPass {
    fn name(&self) -> &str { "naming-convention" }
    
    fn transform(&self, ast: &mut TsNode) -> Result<(), TransformError> {
        // Apply naming conventions to AST nodes
        // Convert property names
        // Convert type names
    }
    
    fn dependencies(&self) -> Vec<&str> { vec![] }
}
```

#### Code Style Pass

```rust
pub struct CodeStylePass {
    style: CodeStyle,
}

#[derive(Debug, Clone)]
pub struct CodeStyle {
    pub indent_size: usize,
    pub use_semicolons: bool,
    pub quote_style: QuoteStyle,
    pub trailing_commas: bool,
}

impl AstTransformPass<TsNode> for CodeStylePass {
    fn name(&self) -> &str { "code-style" }
    
    fn transform(&self, ast: &mut TsNode) -> Result<(), TransformError> {
        // Apply code style rules
        // Format documentation
        // Standardize formatting
    }
    
    fn dependencies(&self) -> Vec<&str> { vec!["naming-convention"] }
}
```

## Plugin System Integration

### Pass Plugin Trait

```rust
pub trait TransformPassPlugin {
    fn name(&self) -> &str;
    fn create_openapi_pass(&self) -> Option<Box<dyn OpenApiTransformPass>>;
    fn create_ir_pass(&self) -> Option<Box<dyn IrTransformPass>>;
    fn create_ast_pass<T>(&self) -> Option<Box<dyn AstTransformPass<T>>>;
}
```

### Plugin Registration

```rust
pub struct PassRegistry {
    plugins: Vec<Box<dyn TransformPassPlugin>>,
}

impl PassRegistry {
    pub fn register_plugin<P: TransformPassPlugin + 'static>(&mut self, plugin: P) {
        self.plugins.push(Box::new(plugin));
    }
    
    pub fn create_pipeline(&self, config: &PipelineConfig) -> TransformPipeline {
        let mut pipeline = TransformPipeline::new();
        
        for plugin in &self.plugins {
            if let Some(pass) = plugin.create_openapi_pass() {
                pipeline = pipeline.add_openapi_pass(pass);
            }
            if let Some(pass) = plugin.create_ir_pass() {
                pipeline = pipeline.add_ir_pass(pass);
            }
        }
        
        pipeline
    }
}
```

## Error Handling

### Transform Error Types

```rust
#[derive(Debug, Snafu)]
pub enum TransformError {
    #[snafu(display("Transform pass '{}' failed: {}", pass, error))]
    PassFailed { pass: String, error: String },
    
    #[snafu(display("Circular dependency detected: {}", cycle))]
    CircularDependency { cycle: String },
    
    #[snafu(display("Invalid pass configuration: {}", message))]
    InvalidConfiguration { message: String },
    
    #[snafu(display("Pass '{}' not found", pass))]
    PassNotFound { pass: String },
}
```

### Error Recovery

```rust
pub struct ErrorRecoveryStrategy {
    pub continue_on_error: bool,
    pub log_errors: bool,
    pub fallback_passes: Vec<String>,
}

impl TransformPipeline {
    pub fn execute_with_recovery(&self, openapi: OpenApi, language: &str, strategy: &ErrorRecoveryStrategy) -> Result<OpenApi, TransformError> {
        // Execute passes with error recovery
        // Log errors if configured
        // Use fallback passes if available
    }
}
```

## Performance Considerations

### Parallel Execution

```rust
impl TransformPipeline {
    pub fn execute_parallel(&self, openapi: OpenApi, language: &str) -> Result<OpenApi, TransformError> {
        // Execute independent passes in parallel
        // Use thread pool for CPU-intensive passes
        // Maintain dependency order
    }
}
```

### Caching

```rust
pub struct PassCache {
    cache: HashMap<String, CachedResult>,
}

impl PassCache {
    pub fn get_cached_result(&self, pass_name: &str, input_hash: &str) -> Option<&CachedResult> {
        self.cache.get(&format!("{}:{}", pass_name, input_hash))
    }
    
    pub fn cache_result(&mut self, pass_name: &str, input_hash: &str, result: CachedResult) {
        self.cache.insert(format!("{}:{}", pass_name, input_hash), result);
    }
}
```

## Testing Strategy

### Unit Tests

Test individual transformation passes:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_reference_resolution() {
        let mut openapi = create_test_openapi();
        let pass = ReferenceResolutionPass::new();
        
        pass.transform(&mut openapi).unwrap();
        
        // Verify references are resolved
        assert!(openapi.components.as_ref().unwrap().schemas.is_empty());
    }
}
```

### Integration Tests

Test pass combinations:

```rust
#[test]
fn test_pipeline_execution() {
    let openapi = load_test_spec();
    let pipeline = TransformPipeline::new()
        .add_openapi_pass(ReferenceResolutionPass::new())
        .add_openapi_pass(SchemaNormalizationPass::new())
        .add_ir_pass(TypeInferencePass::new());
    
    let result = pipeline.execute(openapi, "typescript");
    assert!(result.is_ok());
}
```

## Configuration

### Pipeline Configuration

```rust
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub openapi_passes: Vec<String>,
    pub ir_passes: Vec<String>,
    pub ast_passes: HashMap<String, Vec<String>>,
    pub parallel_execution: bool,
    pub error_recovery: ErrorRecoveryStrategy,
}
```

### Pass Configuration

```rust
#[derive(Debug, Clone)]
pub struct PassConfig {
    pub enabled: bool,
    pub parameters: HashMap<String, serde_json::Value>,
}
```

## Conclusion

The multi-level transformation pass architecture provides a powerful and flexible system for transforming OpenAPI specifications into high-quality generated code. The separation of concerns across different levels enables reusable and composable transformations, while the plugin system allows for easy extensibility.

The dependency resolution system ensures passes are executed in the correct order, and the error handling system provides robust error recovery and reporting.

## Related RFDs

- [RFD 0001: Overall Architecture and Design Philosophy](./0001-architecture-overview.md)
- [RFD 0002: OpenAPI Parsing with utoipa](./0002-openapi-parsing.md)
- [RFD 0003: Language-Specific AST Design](./0003-language-ast-design.md)
- [RFD 0008: Plugin System and Extensibility](./0008-plugin-system.md)
