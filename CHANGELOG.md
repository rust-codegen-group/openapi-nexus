# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1]

### Added

- Python: discriminator-aware `_from_dict()` / `_to_dict()` helpers for tagged unions (internal, adjacent, external)
- Rust: struct variants for internally/adjacently tagged enums (fields inlined into enum)

### Changed

- Rust: upgrade sigil-stitch 0.4.3 → 0.4.4 (fixes Display impl spacing)
- Rust: generated code now includes `// @generated` marker and `#![allow(clippy::all)]`
- Rust: dead standalone struct files no longer emitted for exclusively-inlined variants
- Rust: function signatures use `&[T]` instead of `&Vec<T>`
- Rust: query params URL-encoded via `url::form_urlencoded` (reqwest/aioduct)

### Fixed

- Rust: punctuation spacing in generated `Display` impls (`std:: fmt::` → `std::fmt::`)
- Rust: flaky golden tests from temp dir name collisions under parallelism
- Python: pyright strict-mode errors in generated tagged union helpers

## [0.1.0]

### Added

- Rust code generators: `rust-reqwest`, `rust-ureq`, `rust-aioduct` (three HTTP backends with shared model emission)
- Python code generators: `python-httpx`, `python-requests`
- Java OkHttp code generator: `java-okhttp`
- Kotlin OkHttp code generator: `kotlin-okhttp`
- OpenAPI 3.0 spec parsing and IR lowering
- OpenAPI 3.2 spec parsing and IR lowering
- Configurable extra derives per type-kind for Rust generators (`extra_derives` config)
- AbortSignal support in TypeScript fetch runtime (`RequestOpts`)
- Compile-check CI gates for all 6 target languages (TypeScript, Go, Rust, Python, Java, Kotlin)

### Changed

- Consolidated 15 workspace crates into a single crate with modules (`src/` replaces `crates/`)
- Replaced minijinja templates with sigil-stitch code emission across all generators
- Converted remaining `CodeBlock::builder()` sites to `sigil_quote!()` macro (sigil-stitch 0.2.1 → 0.4.3)
- Bumped MSRV to 1.90 (Rust edition 2024)
- Bumped aioduct dependency 0.1.3 → 0.1.6 (added required `rustls-ring` feature)
- Flattened Justfile layout, aligned CI with sibling projects
- TypeScript: replaced `any` with `unknown` across generated code and runtime
- TypeScript: named re-exports instead of barrel `export *`
- TypeScript: collapsed nullable+optional fields to two states
- TypeScript: unified file headers, dropped `tslint:disable`
- Go: rewrote generator on IR + sigil-stitch (no more minijinja)

### Fixed

- TypeScript: use strict equality (`===`) in required-param guards
- TypeScript: derive Content-Type from `requestBody.content` instead of hardcoding JSON
- TypeScript: use dot access for statically-known `requestParameters` keys
- TypeScript: unified `runtime.ts` indentation to 2-space

## [0.0.5]

### Added

- Justfile with build, generate, lint, and test actions
- Kind type generation for discriminated unions in TypeScript generator
- Mixed enum support in enum-repr test fixture generator

### Changed

- Added openapi-nexus-spec to published crates list

## [0.0.4]

### Added

- additional-properties fixture and generator with Go and TypeScript support
- openapi-nexus-spec: OAS 3.0 spec types and openapi-generator fixture tests
- openapi-nexus-spec: OAS 3.1 spec types
- TypeScript: query parameter enum support and improved PascalCase consistency

### Changed

- Migrated core and codegen to OAS 3.1 spec types

## [0.0.3]

### Added

- Tagged enum pattern detection and support in TypeScript generator (externally, adjacently, internally, and untagged enum patterns)
- Intersection type support with nullable references in TypeScript generator
- Enum representation test fixture generator (`openapi-nexus-examples-enum-repr`)

### Changed

- Renamed `openapi-nexus-petstore-example` package to `openapi-nexus-examples-petstore` (organizational change)

### Fixed

- PascalCase conversion handling in model imports with aliases for TypeScript

## [0.0.2]

### Added

- Go HTTP client generator (`openapi-nexus-go` crate) with comprehensive OpenAPI schema support
- Generator framework abstraction replacing the language option (--generator instead of --language)
- Go reserved keyword escaping for parameter, struct, field, and API client names
- Union type support (oneOf, anyOf, allOf) in Go type mapping
- Main SDK file generation for Go with New() function and options pattern (WithBaseURL, WithHTTPClient)
- Golden build check scripts for go-http generator
- SDK interface system with hooks package for type safety
- Request body type extraction and generation for inline schemas

### Changed

- Replaced language CLI option with generator option (--generator, OPENAPI_NEXUS_GENERATOR env var)
- Reorganized generator architecture: moved registry to main crate, organized by generator framework
- Improved Go SDK architecture: changed API client package from operations to apis, moved SDK files to sdk/ subdirectory
- Improved import organization in Go generator (separated stdlib and project imports)
- Simplified response type names by removing Response suffix
- Updated golden test file naming convention to use .golden suffix

### Fixed

- Improved TypeScript instanceOf type guard to check both original and TypeScript property names
- Fixed TypeScript instanceOf type guard with proper type checking (null/undefined handling, unknown parameter type)

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
