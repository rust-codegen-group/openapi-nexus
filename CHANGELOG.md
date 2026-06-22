# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Bump `sigil-stitch` to 0.6.8

## [0.1.14]

### Added

- Multipart request body generation across all generated clients
  - Supports explicit per-part media types from OpenAPI `encoding.contentType`
  - Adds multipart fixtures for edge cases, nested object parts, unsupported schemas, and optional bodies
- Optional request body fixture coverage across all generators

### Fixed

- Rust: preserve `requestBody.required: false` and emit optional request bodies as `Option<&T>`
  - `reqwest` and `aioduct` omit body setup when optional bodies are absent
  - `ureq` uses the existing empty-send path for absent optional POST bodies
- Rust: align JSON, form, multipart, XML, text, and binary request body media-type emission
- TypeScript and Python: align multipart wire output with current generated clients
- Go, Java, Kotlin, Python, Rust, and TypeScript: fix SDK generation edge cases around array parameters, tagged unions, type aliases, response matching, path prefixes, and media-type selection

### Changed

- Refactor Rust request body emission around `sigil_quote!`
- Bump `sigil-stitch` to 0.6.7

## [0.1.13]

### Added

- cargo-dist support for prebuilt binary releases across 7 platforms
  - Release workflow: pushes version tags → builds + publishes to GitHub Releases
  - Nightly workflow: pushes to `master` → builds + updates `nightly` GitHub Release
  - Shell and PowerShell installers generated automatically
  - Install via: `curl .../releases/download/nightly/openapi-nexus-installer.sh | sh`

## [0.1.12]

### Added

- Dynamic/lazy auth token provider support across all generators
  - TypeScript: `accessToken` wired into `createFetchParams()`, evaluated per-request
  - Go: `BearerAuth` gains `TokenProvider func() string` field
  - Java: `BearerAuth` gains `Supplier<String>` constructor
  - Kotlin: `BearerAuth` gains `(() -> String)` secondary constructor
  - Python: `BearerAuth` accepts `str | Callable[[], str]`
  - Rust: `BearerAuth` gains `from_provider()` factory with internal `TokenSource` enum
- `ApiKeyAuth` dynamic key provider support (all generators)
  - Go: `KeyProvider func() string` field; Java: `Supplier<String>` constructor; Kotlin: `() -> String` constructor; Python: `str | Callable[[], str]`; Rust: `from_provider()` factory
- `BasicAuth` dynamic credential provider support
  - Go: `UsernameProvider` / `PasswordProvider` func fields
  - TypeScript: `username` / `password` accept `(() => string | Promise<string>)`, wired into `createFetchParams()`

## [0.1.11]

### Added

- TypeScript: `indent` config option for controlling output indentation (default `"  "`)

### Fixed

- Python: eliminated extra blank lines between if/elif blocks in tagged union `_from_dict`/`_to_dict` helpers

### Changed

- Bump sigil-stitch 0.5.3 → 0.6.6: adopt `$comment(expr)`, `$attr(expr)`, `%V` verbatim strings, structural braces, and `end_control_flow_no_newline()` across all generators

## [0.1.10]

### Changed

- Bump sigil-stitch to v0.5.3
- Use `else` for last isinstance branch in discriminated union `to_dict` helpers (pyright strict clean)

## [0.1.9]

### Added

- TypeScript: `toolchain = "vp"` config for vite-plus — generates `vite.config.ts` with type-aware linting, `vp pack` library build, and `vp check --no-fmt` type checking

### Fixed

- TypeScript: wildcard response codes (4XX, 5XX) now emit range checks instead of duplicate `status === 0` branches
- TypeScript: split import collection into request-only and response-only sets so `toJSON` is only imported for request body types
- TypeScript: empty-object `fromJSON`/`toJSON` converters use pass-through cast instead of empty destructure
- TypeScript: vp toolchain emits `.mjs`/`.d.mts` paths in `package.json` to match `vp pack` output
- Python: add `to_dict`/`from_dict` to allOf intersection dataclasses
- Python: serialize `list[Model]` request bodies with `to_dict` mapping
- Python: use comma-join for array query params instead of `str(list)`
- Python: recursively serialize Map values containing object types
- Go: populate `Status4XX`/`Status5XX` fields for wildcard response codes
- Go: emit `AdditionalProperties` field with `MarshalJSON`/`UnmarshalJSON`

### Changed

- Bumped sigil-stitch 0.5.0 → 0.5.1
- Internal: migrated raw `format!()` string building to sigil-stitch structured APIs (Go `TypeName`, `add_embedded`, Python `CodeBlock`, Rust `AnnotationSpec::args`)

## [0.1.8]

### Fixed

- TypeScript: avoid TS2783 compile error in camelCase mode by putting spread before discriminator literal in `fromJSON`/`toJSON` converters

## [0.1.7]

### Added

- Rust (aioduct): config-driven feature management via `[generators.rust-aioduct.aioduct]` section
  - `runtime` — select async runtime: `tokio` (default), `smol`, or `compio`
  - `tls` — TLS backend: `rustls-ring` (default), `rustls-aws-lc-rs`, or `"false"` to disable
  - `compression` — opt-in decompression codecs: `gzip`, `brotli`, `zstd`, `deflate`
  - `features` — pass-through feature flags (e.g. `tracing`, `http3`, `blocking`)
  - `version` — override the pinned aioduct version
- Rust (aioduct): generated client now uses `Client::with_rustls()` (or `with_http3()`) based on TLS config, fixing HTTPS support

### Changed

- Bumped aioduct dependency 0.1.6 → 0.1.8

### Fixed

- Rust: escape double quotes in Cargo.toml `description` field (fixes TOML parse errors when spec descriptions contain quotes)
- Rust: skip discriminator field in internally-tagged struct variants (fixes `serde(tag)` conflict)
- Rust: emit unit variants when all fields are the discriminator (e.g. `SetupUnspecified,` instead of `SetupUnspecified {}`)

## [0.1.6]

### Added

- TypeScript: `property_naming = "camelCase"` config generates dual-type model files with wire-format (`Name$Wire`) and ergonomic (`Name`) interfaces, plus `nameFromJSON` / `nameToJSON` converter functions
  - Covers Objects, TaggedUnions (internal, adjacent, external tagging), Intersections (allOf), and Unions (oneOf)
  - Externally-tagged unions use `if ('KEY' in json)` chains instead of switch statements
  - Barrel re-exports both types and converter functions automatically

### Fixed

- TypeScript: intersection `fromJSON` returns now use type assertion (`as Name`) for mixed object+enum allOf compositions
- TypeScript: two-pass convertible set computation correctly handles Intersection/Union schemas containing enum member refs
- TypeScript: non-convertible schemas (enums, simple aliases) no longer generate broken `$Wire` import references in camelCase mode
- TypeScript: reserved-word field names (e.g. `delete`, `class`) are correctly quoted in generated interfaces (sigil-stitch 0.5.0)

## [0.1.5]

### Added

- TypeScript: `emit_enum_constants` config for companion const objects (`export const Name = { KEY: 'val' as const, ... }`) alongside enum type aliases
- TypeScript: `emit_type_guards` config for `is*` type guard functions alongside tagged union type aliases (internal, adjacent, and external tagging)
- TypeScript: models barrel (`models/index.ts`) now emits value re-exports alongside type re-exports when features are enabled

## [0.1.4]

### Added

- Rust: native `[utoipa]` config section for automatic `utoipa::ToSchema` integration
  - Structs, string enums, integer enums, tagged unions, intersections, and aliases get `#[derive(utoipa::ToSchema)]`
  - Untagged and tagged unions (internal/adjacent) get manual `impl utoipa::PartialSchema + ToSchema` with `OneOfBuilder`
  - Adds `utoipa` to generated Cargo.toml dependencies automatically
- Rust: `[lints] workspace = true` emitted in generated Cargo.toml when `workspace_mode = true`
- Rust: `url` dependency is now conditional (only included when spec has query parameters) for reqwest and aioduct backends

### Fixed

- Rust: removed standalone `tokio` dependency from reqwest generated Cargo.toml
- Rust: variant schemas for internal/adjacent tagged unions are no longer inlined when utoipa is enabled (standalone types required for `PartialSchema` references)

## [0.1.3]

### Fixed

- Rust: API types referencing suppressed primitive aliases (e.g. schema "string") now resolve to the primitive type instead of emitting broken `crate::models::String` paths
- Rust: removed standalone `tokio` dependency from aioduct generated Cargo.toml (already pulled via aioduct's `"tokio"` feature)
- Rust: codegen warns when a qualified derive path (e.g. `utoipa::ToSchema`) has no matching `dependencies` entry

## [0.1.2]

### Added

- Rust: `workspace_deps` config with 3 modes (`explicit`, `workspace_version`, `full`) for generated Cargo.toml dependencies
- Rust: `per_type` extra derives for targeting specific schemas by name
- Rust: `package_name` accepted as alias for `crate_name` in generator config

### Fixed

- Rust: bogus `string.rs` no longer generated for schemas named "string" (was producing `pub type String = String;` that shadows std)
- Rust: multi-line operation summaries now correctly prefix every line with `///` (YAML `|-` block scalars)
- Rust: `#[serde(untagged)]` enums excluded from blanket `extra_derives.unions` (incompatible with utoipa::ToSchema)

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
