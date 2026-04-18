# CLAUDE.md (DO NEVER COMMIT CLAUDE.md FILE)

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Quick Commands (Justfile)

```bash
just --list --list-submodules    # List all available commands

# Testing
just test::test-all              # Run all tests
just test::test-typescript       # Run TypeScript tests
just test::test-go               # Run Go tests
just test::test-specific NAME    # Run a specific TypeScript test by name

# Golden tests
just golden::check               # Run Rust golden comparison tests
just golden::update              # Update all .golden files
just golden::update-ts           # Update TypeScript .golden files only
just golden::update-go           # Update Go .golden files only
just golden::build-ts            # Compile-check TypeScript goldens with tsc --noEmit
just golden::build-go            # Compile-check Go goldens with go build ./...
just golden::build-all           # Compile-check all goldens

# Building
just build::build-all            # Build all crates
just build::build-release        # Release build
just build::build-crate CRATE    # Build a specific crate
just build::check                # cargo check
just build::check-all            # cargo check --all-targets --all-features

# Linting (required before commit)
just lint::strict                # Clippy with warnings as errors
just lint::all-strict            # Format + lint strict
just lint::fmt                   # cargo fmt only
just lint::clippy                # cargo clippy only

# Code generation
just generate::typescript input.yaml output/
just generate::go input.yaml output/
just generate::all input.yaml output/    # Both TypeScript and Go
```

## Project Overview

OpenAPI Nexus is a modular OpenAPI 3.1 code generator written in Rust. It transforms OpenAPI specifications into type-safe client libraries for multiple languages (TypeScript, Go, Rust planned).

### Architecture

The project follows a compiler-like pipeline:
```
OpenAPI Spec → Parse (utoipa) → Transform → CodeGenerator trait → Templates (minijinja) → Generated Code
```

### Workspace Structure (15 crates)

**Core pipeline crates:**
- `openapi-nexus` - CLI entry point, orchestrator, `GeneratorRegistry`
- `openapi-nexus-core` - Core traits (`CodeGenerator`, `FileWriter`, `CombinedGenerator`), tagged enum pattern detection, data types (`ApiMethodData`, `ModelData`, `RuntimeData`)
- `openapi-nexus-common` - Shared types (`GeneratorType`, `Language`, `SourceLocation`)
- `openapi-nexus-spec` - OpenAPI specification types (OAS 3.0, 3.1, 3.2)
- `openapi-nexus-parser` - OpenAPI parsing using utoipa
- `openapi-nexus-ir` - Intermediate representation: `Analyzer` (schema extraction), `SchemaAnalyzer` (dependency/circular ref analysis), `OpenApiVisitor`/`OpenApiTraverser` (visitor pattern), `ReferenceResolver`
- `openapi-nexus-transforms` - Transform pipeline with 7 passes: circular reference detection, dependency analysis, naming convention, path normalization, reference resolution, schema normalization, type inference

**Language generators:**
- `openapi-nexus-typescript` - TypeScript fetch API client generation
- `openapi-nexus-go` - Go HTTP client generation
- `openapi-nexus-rust` - Rust code generation (in development)

**Supporting:**
- `openapi-nexus-config` - Configuration system (CLI args > env vars `OPENAPI_NEXUS_*` > TOML config > defaults)
- `openapi-nexus-plugin` - Plugin framework

**Fixture generators** (test utilities):
- `fixture-generators/enum-repr`, `additional-properties`, `petstore` - Generate type-checked OpenAPI specs from Rust code using utoipa

## Common Commands

```bash
# Run all tests
cargo test

# Run tests for a specific crate
cargo test -p openapi-nexus-typescript
cargo test -p openapi-nexus-go

# Run a specific test
cargo test -p openapi-nexus-typescript test_minimal_golden

# Golden tests with update mode (after intentional output changes)
UPDATE_GOLDEN=1 cargo test --test golden_tests_typescript_fetch
UPDATE_GOLDEN=1 cargo test --test golden_tests_go_http

# Clippy with warnings as errors (required before committing)
cargo clippy --all-targets --all-features -- -D warnings

# Golden build verification (check generated code compiles)
just golden::build-ts
just golden::build-go
just golden::build-all

# Run the CLI
cargo run -- generate --input tests/fixtures/valid/minimal.yaml --output output --generator typescript-fetch

# Generate multiple generators at once
cargo run --bin openapi-nexus -- generate --input <spec> --output <dir> --generators typescript-fetch,go-http

# Generate fixture OpenAPI specs
cargo run --bin fixture-generator-enum-repr-spec-generator
cargo run --bin fixture-generator-petstore-spec-generator

# Sync external OpenAPI Generator test fixtures
./scripts/sync-openapi-generator-fixtures.sh
```

## Key Design Patterns

### Generator Registration
Generators register via `GeneratorRegistry` in `openapi-nexus/src/generator_registry.rs`. To add a new language generator:

1. Implement `CodeGenerator` trait from `openapi-nexus-core/src/traits/code_generator.rs`
2. Implement `FileWriter` trait from `openapi-nexus-core/src/traits/file_writer.rs`
3. Register in `OpenApiCodeGenerator::new()` (`openapi-nexus/src/openapi_code_generator.rs`)

### The `CodeGenerator` Trait
The default `generate()` method orchestrates: `collect_operations_by_tag()` → `generate_apis()` → `generate_models()` → `generate_runtime()` → `generate_readme()` → `generate_project_files()`. Each returns `Vec<FileInfo>`.

### Tagged Enum Patterns
Discriminated unions (oneOf/anyOf items) are classified in `openapi-nexus-core/src/tagged_enum_pattern.rs`:
- `ExternallyTagged` - Object with single required string enum property
- `AdjacentlyTagged` - Object with tag (string enum) + content (object/ref)
- `InternallyTagged` - allOf containing a string enum property
- `Untagged` - `$ref` to `#/components/schemas/*`

## Template System (minijinja)

Templates use **minijinja** (Jinja2-compatible) with `minijinja-embed` for compile-time embedding. Each language generator has its own `templating/environment.rs` that creates a `minijinja::Environment` with embedded templates, custom filters, and functions.

### Template Locations
- TypeScript: `openapi-nexus-typescript/templates/` - subdirs for `typescript-fetch/`, `model/`, `api/` (includes `api/snippets/`), `common/`
- Go: `openapi-nexus-go/templates/go-http/` - subdirs for `api/`, `model/`, `project/`, `runtime/`, `types/`

### Template Conventions
- Templates are `.j2` files loaded at compile time via `minijinja_embed::load_templates!`
- Use Jinja2 syntax - `'text' not in var` instead of JavaScript methods like `.includes()`
- `trim_blocks` and `lstrip_blocks` are enabled by default
- Custom `fmt` filter available for formatting

## Testing Strategy

### Golden Tests
Each golden test has:
- An OpenAPI fixture in `tests/fixtures/valid/`
- Expected output with `.golden` suffix in `tests/golden/typescript/typescript-fetch/` or `tests/golden/go/go-http/`
- Test runner in `openapi-nexus-typescript/tests/golden_tests_typescript_fetch.rs` or `openapi-nexus-go/tests/golden_tests_go_http.rs`

Run a specific golden test:
```bash
cargo test -p openapi-nexus-go --test golden_tests_go_http -- minimal --nocapture
```

### Fixture Tests
OpenAPI specs in `fixture-generators/` are generated from Rust code using utoipa, ensuring fixtures are type-checked and valid.

### Invalid Fixture Tests
Tests in `tests/fixtures/invalid/` verify proper error handling for invalid specs.

## Language-Specific Notes

### TypeScript Generator
- Uses `pretty.rs` crate for code formatting (not prettier)
- File naming: PascalCase by default (configurable)
- Module system: ESM or CommonJS support
- Located in `openapi-nexus-typescript/src/` - key module: `generator/` contains sub-generators

### Go Generator
- Uses standard library formatting
- File naming: snake_case
- Handles reserved keyword escaping
- Located in `openapi-nexus-go/src/`

## Important Dependencies

- `utoipa` - OpenAPI parsing and generation
- `minijinja` + `minijinja-embed` - Jinja2-compatible template engine with compile-time embedding
- `pretty` - Pretty printing for generated code
- `snafu` - Error handling
- `heck` - Case conversion utilities
- `rust-embed` - Static asset embedding

## Rust Version

Required: Rust 1.90+, edition 2024 (see `rust-toolchain.toml`)
