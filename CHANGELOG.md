# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.0.1-alpha.4]

### Added

- Cross-compilation support with `cross.toml` configuration for multi-platform builds
- GitHub Actions CI workflow now uses `cross` tool for Linux cross-compilation targets
- Comprehensive HTTP response handling with status codes and content types
- ContentType enum for representing HTTP content types with JSON and text support
- StatusCode struct to handle exact codes, ranges (e.g., 2XX), and default responses
- HttpResponse struct to normalize OpenAPI responses with status, content types, and schemas
- OpenApiRefExt trait for resolving component references (schemas, responses, parameters)
- collect_responses() method to OperationInfo to categorize success, error, and default responses
- Type-safe response handling code generation for operations with multiple status codes and content types
- Support for response handling with different content types and optional response bodies
- Test fixtures for various response scenarios (default responses, fallback, multi-status, no response body)

### Changed

- Refactored return type generation to use HttpResponse and StatusCode for type-safe response handling
- Updated response transformer to handle different content types and response body presence
- Updated dependencies in Cargo.toml and Cargo.lock

## [0.0.1-alpha.3]

### Added

- Automatic import generation for model dependencies with proper type/value separation
- Support for duplicated parameter names and recursive JSON typings
- Extraction of nested inline objects into named interfaces
- Union type support with member extraction and improved import handling
- Support for inline objects and any type in union types
- Type discrimination helpers for union types in JSON serialization
- Script to check TypeScript build status for test directories

### Changed

- Standardized naming fields and track original schema names throughout the type system

### Fixed

- Improved array type mapping to properly resolve item types

## [0.0.1-alpha.2]

### Added

- GitHub Actions workflow for multi-platform builds
- Duplicate parameter name handling with location-based conflict resolution
- Property name conversion to camelCase while preserving original names

### Changed

- Updated default TypeScript config values and improved ts_lib handling
- Updated package.json to use dist output and enable build scripts

### Fixed

- Fixed package.json to use correct source files

## [0.0.1] - Initial Release

- Initial release of OpenAPI Nexus
- OpenAPI 3.1 specification parsing using utoipa
- CLI application with command-line argument support
- Configuration system supporting CLI, env vars, and config files
- Template-based code emission using Jinja2
- TypeScript/fetch code generation with full type safety
- Multi-file output organization
