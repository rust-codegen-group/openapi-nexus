# RFD 0009: Error Handling and Diagnostics

## Summary

This RFD defines the comprehensive error handling and diagnostics system for the OpenAPI code generator. The system provides structured error types, source location tracking, user-friendly error messages, warning systems, and diagnostic reporting with recovery strategies.

## Motivation

### Why Comprehensive Error Handling?

1. **User Experience**: Clear, actionable error messages help users fix issues quickly
2. **Debugging**: Detailed error information aids in troubleshooting
3. **Reliability**: Robust error handling prevents crashes and data loss
4. **Maintainability**: Structured error handling makes the codebase easier to maintain
5. **Integration**: Well-defined error types enable better tool integration

### Design Goals

- **Clarity**: Error messages should be clear and actionable
- **Context**: Errors should include relevant context and source locations
- **Recovery**: Provide recovery strategies where possible
- **Consistency**: Uniform error handling across all components
- **Extensibility**: Easy to add new error types and handling strategies

## Error Type Hierarchy

The error handling system uses a hierarchical error type structure that organizes errors by the layer or phase where they occur. This provides clear error categorization and enables proper error propagation.

### Core Error Types

The top-level error type represents errors that occur during the main code generation orchestration:

- **Parse Errors**: Errors encountered while reading or parsing OpenAPI specification files
- **Transform Errors**: Errors that occur during the transformation phase
- **Generation Errors**: Errors during code generation (generic errors from language generators)
- **Unsupported Language**: Errors when a requested language is not supported
- **Generator Not Found**: Errors when a language generator is not registered

### Parse Errors

Parse errors occur when reading or parsing OpenAPI specification files:

- **File Read Errors**: Issues reading the specification file from disk
- **JSON Parse Errors**: Invalid JSON syntax in JSON-formatted specifications
- **YAML Parse Errors**: Invalid YAML syntax in YAML-formatted specifications
- **Unsupported Format**: File format is not recognized or supported

### Transform Errors

Transform errors occur during the transformation pipeline phase:

- **Generic Transform Errors**: General transformation failures
- **Pass Failed**: A specific transformation pass encountered an error
- **Circular Dependency**: Circular dependencies detected in transformation passes
- **Invalid Configuration**: Invalid configuration for a transformation pass
- **Pass Not Found**: A requested transformation pass does not exist

### IR (Intermediate Representation) Errors

IR errors occur during reference resolution and schema analysis:

- **Circular Reference**: Detected circular references in schema definitions
- **Unresolved Reference**: A reference cannot be resolved to an actual definition
- **Invalid Reference**: A reference has an invalid format or structure
- **Analysis Error**: Errors during schema analysis operations
- **External Reference**: External references (HTTP/HTTPS) are not supported

### Emission Errors

Emission errors occur during code emission/generation:

- **Template Errors**: Errors in template rendering or processing

### Configuration Errors

Configuration errors occur during configuration merging and validation:

- **Validation Errors**: Configuration validation failures

## Source Location Tracking

Source location tracking provides context about where errors occur within OpenAPI specifications. This helps users quickly identify and fix issues in their specifications.

### Location Information

Source locations can include:

- File paths where errors occur
- Line and column numbers (when available)
- OpenAPI path references (e.g., `#/components/schemas/User`)
- Additional context about the error location

### Current Usage

Source location tracking is currently implemented in the IR layer, where it provides context for reference resolution and schema analysis errors. The system is designed to be extensible, allowing more detailed location tracking to be added in the future.

## User-Friendly Error Messages

Error messages are designed to be clear, actionable, and provide sufficient context for users to understand and fix issues.

### Message Characteristics

Error messages include:

- **Clear Description**: What went wrong in plain language
- **Relevant Context**: File paths, references, language names, etc.
- **Source Location**: When available, information about where the error occurred
- **Actionable Information**: Details that help users understand what to fix

### Future Enhancements

Potential improvements to error messages:

- Suggestions for fixing common errors
- Related error information and context
- Links to documentation or examples
- Automatic error recovery hints

## Warning and Lint System

The warning system provides non-fatal notifications about potential issues, deprecated features, or best practice violations. This allows users to be informed about issues without blocking code generation.

### Warning Categories

Potential warning types include:

- **Deprecated Features**: Use of deprecated OpenAPI features or patterns
- **Non-Standard Extensions**: Use of non-standard OpenAPI extensions
- **Unused Definitions**: Definitions that are not referenced
- **Performance Issues**: Potential performance problems
- **Code Style**: Style or formatting issues

### Warning Levels

Warnings can be categorized by severity:

- **Error**: Must be fixed (blocks generation)
- **Warning**: Should be fixed (non-blocking)
- **Info**: Informational messages
- **Hint**: Suggestions for improvement

### Current Implementation

Currently, warnings are logged using standard logging mechanisms. A more comprehensive warning collection and reporting system is planned for future implementation.

## Diagnostic Reporting Format

Diagnostic reporting provides structured error and warning information that can be consumed by various tools and IDEs.

### Reporting Formats

Potential diagnostic formats include:

- **Human-Readable**: Formatted text output for terminal/console display
- **JSON**: Structured JSON output for programmatic consumption
- **XML**: XML format for integration with existing toolchains
- **LSP**: Language Server Protocol format for IDE integration

### Current Implementation

Currently, errors are logged and displayed via standard error output. Structured diagnostic reporting formats are planned for future implementation to enable better tool integration.

## Recovery Strategies

Error recovery strategies enable the system to automatically fix or work around certain types of errors, improving user experience by reducing manual intervention.

### Recovery Concepts

Recovery strategies can:

- **Detect Recoverable Errors**: Identify errors that can be automatically fixed
- **Attempt Recovery**: Try to fix the error automatically
- **Provide Suggestions**: Offer manual recovery suggestions when automatic recovery is not possible

### Recovery Types

Potential recovery strategies include:

- **Missing Required Fields**: Provide default values for missing required fields
- **Invalid Field Values**: Attempt to fix or normalize invalid field values
- **Schema Validation**: Offer suggestions for schema validation failures
- **Reference Resolution**: Attempt to resolve missing references

### Current Implementation

The system currently follows a fail-fast approach where errors are logged and the process exits. Automatic error recovery strategies are planned for future implementation.

## Error Context Propagation

Error context propagation ensures that errors carry relevant context from each layer they pass through, making debugging easier by preserving the full error chain.

### Propagation Mechanism

Errors propagate through the system by:

- **Wrapping Lower-Level Errors**: Higher-level errors wrap lower-level errors to add context
- **Preserving Error Chain**: The full error chain is maintained, showing where errors originated
- **Adding Layer Context**: Each layer adds its own context to help identify where processing failed

### Error Flow

Example error propagation:

- Parse errors are wrapped in orchestration errors with additional context
- Transform errors are wrapped with transformation pipeline context
- IR errors are logged directly in the IR layer with schema analysis context

## Testing Strategy

Error handling is tested to ensure errors are created correctly, formatted properly, and propagate through the system as expected.

### Testing Approach

Error handling tests verify:

- **Error Creation**: Errors can be created with appropriate context
- **Error Formatting**: Error messages are formatted correctly and contain expected information
- **Error Propagation**: Errors propagate correctly through different layers
- **Error Context**: Error context is preserved during propagation

### Test Coverage

Tests cover:

- Unit tests for individual error types
- Integration tests for error propagation through the system
- Error message formatting and readability

## Performance Considerations

Error handling is designed to be efficient and not impact the performance of successful code generation operations.

### Performance Principles

- **Errors are Rare**: The system is optimized for the common case where errors don't occur
- **Lazy Evaluation**: Error messages are formatted only when needed (when errors occur)
- **Minimal Overhead**: Error creation and logging have minimal performance impact
- **No Caching Required**: Error handling is lightweight enough that caching is unnecessary

### Optimization Strategies

The system avoids unnecessary overhead:

- Error types are simple and efficient to create
- Error messages use lazy formatting through standard display traits
- Logging uses efficient tracing macros
- No complex error structures or caching mechanisms are needed

## Conclusion

The error handling system provides a robust foundation for reliable OpenAPI code generation. Key features:

1. **Structured Error Types**: Clear, hierarchical error types organized by layer and phase
2. **Error Logging**: All errors are logged before being returned, ensuring comprehensive error visibility
3. **User-Friendly Messages**: Clear, actionable error messages with relevant context
4. **Source Location Tracking**: Location information available in IR layer errors for better debugging
5. **Consistent Error Handling**: Uniform error handling patterns across all components

The system is designed to be extensible - additional error types, recovery strategies, and diagnostic formats can be added as needed without breaking existing functionality.

## Related RFDs

- [RFD 0001: Overall Architecture and Design Philosophy](./0001-architecture-overview.md)
- [RFD 0002: OpenAPI Parsing with utoipa](./0002-openapi-parsing.md)
- [RFD 0004: Multi-Level Transformation Passes](./0004-transformation-passes.md)
