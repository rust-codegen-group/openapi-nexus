# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Generate TypeScript source, runtime templates, Markdown examples, and JSON
  configuration with formatting compatible with Vite+ checks. Lay out long
  signatures, imports, unions, and converter expressions directly in Rust.
  Enable full `vp check` and validate freshly generated clients before and after
  packing, without a formatter preparation step. Exclude `dist/` from formatting
  and provide a `fmt` script for manual edits.
- Upgrade generated TypeScript clients to Vite+ `^1.5.0`, resolving Vite `8.3.7`,
  Vitest `5.1.0`, Rolldown `1.2.18`, Oxfmt `0.76.0`, Oxlint `1.91.0`,
  oxlint-tsgolint `7.0.2005`, and tsdown `0.23.4`. Pin CI validation and
  documented setup to Vite+ `1.5.1` and TypeScript `7.0.2`, and report the resolved
  tool versions during client builds. Vite+ projects require Node.js
  `^22.18.0 || ^24.11.0 || >=26.0.0`.

- Upgrade `sigil-stitch` to 0.7.5 for structured TypeScript namespace-qualified
  types with import-aware nested arguments.
- Retain inline scalar enum values in `IrTypeExpr::ScalarEnum` for union selection.
  Rust callers exhaustively matching the public IR enum must handle this variant;
  its `representation_nullable` fact retains an extracted null permission at the
  original enum boundary, while `representation_is_constraint` distinguishes
  authored types from synthetic untyped-enum fallbacks. Other generators retain
  their existing representation and enum policies.

### Fixed

- Select Rust untagged-union members using retained scalar enum and structural
  facts instead of Serde's unconstrained first-success dispatch. Preserve typed
  variants and payloads across reqwest, ureq, and aioduct, including nested
  containers and recursive members. Add `firstMatch`, `strict`, and explicit
  `Raw(serde_json::Value)` policies; propagate selected-member decoding errors
  without hiding them behind another member or an opaque fallback.
- Convert Python httpx/requests untagged-union members in models, nested containers,
  aliases, JSON/multipart requests, responses, and lazy error bodies. Share enum-
  aware selection across both transports with `firstMatch`, `strict`, and dynamic
  `raw` policies, preserving nested strict errors and productive recursion. Keep
  shared helpers import-safe and restore tagged wire wrappers from typed payload
  members. Encode required nullable union request bodies as JSON `null` instead
  of omitting them.
- Convert camelCase TypeScript untagged-union members in both JSON directions,
  including mixed client/search tools and containing requests. Add configurable
  `firstMatch`, `strict`, and wire-preserving `raw` selection policies instead
  of treating every multi-member union converter as an identity cast. Retain
  nested converter/strict-policy delegation, concrete-before-fallback priority
  through nullable and `allOf` aliases, binary model values, and adjacent-tagged
  unit variants. Convert inline container members and concrete values in open
   nested dictionaries, and preserve array/null shapes through single-member
   `allOf` aliases. Use the same value plan for models, nested HTTP bodies,
   multipart JSON parts, response/error payloads, and tagged payloads/type guards.
   Retain scalar enum and typed additional-value checks, productive recursive
   matching, honest raw alias types, and readonly dictionary contracts. Required
   nullable JSON bodies distinguish a supplied `null` from an omitted body.
   Optional bodies also delegate supplied null, without an unformatted inline
   conditional. Intersect nested enum constraints at their original nullability
   boundary and retain authored concrete null matches beside opaque fallbacks.
   Untyped numeric enums no longer inherit a synthetic string constraint;
   extracted enum null permission restores a typed nullable conversion boundary.
   Nullable `oneOf`/`anyOf` wrappers retain outer enum constraints; explicit null
   alternatives do not bypass strict candidate counts or nested strict errors.
   Emit compound conversions and predicates as static internal helpers with
   direct Vite+-compatible layout, preserving readonly inputs and public exports.
- Preserve tagged TypeScript models named `Record` or `Readonly` by qualifying
  built-in utility references without renaming the generated public model.


## [0.3.0]

### Breaking Changes

- Replace generator and file-writer dynamic error returns with typed
  `GeneratorError` and `WriteError` interfaces. The orchestration API now returns
  `Result<GenerationReport, PipelineError>`; callers must inspect the report for
  individual generator and file-writing failures.
- Return a nonzero CLI status when parsing, lowering, generation, or writing
  fails, including partial failures in a multi-generator run.
- Generate typed Java sealed interfaces and Kotlin sealed classes for supported
  tagged and untagged unions instead of `Object`/`Any` fallbacks. Regenerate JVM
  clients and update construction, matching, and payload access to their generated
  alternatives.
- Change `ConfigError::ParseOverrides` and `ConfigError::Validation` from tuple
  variants to named `message` fields. OpenAPI 3.0 `ObjectSchema` exclusive-bound
  fields now use `Option<ExclusiveBound>` instead of `Option<serde_json::Number>`.
- Add `IrTaggedVariant.additional_properties` for external-wrapper extension
  metadata. Callers constructing IR variants must initialize this field.
- Enforce readonly TypeScript dictionary indexes, including compatible `allOf`
  aliases, while retaining precise types for declared properties.

### Added

- Generate Gson adapters for Java and Kotlin union models that retain internal,
  adjacent, external, and untagged wire shapes, with specific alternatives tried
  before raw JSON fallbacks.
- Preserve declared TypeScript additional properties in preserve-mode, ergonomic,
  and wire types, including mixed objects, typed dictionaries, recursive values,
  compatible `allOf` composition, model aliases, and open tagged variants.

### Fixed

- Retain opaque TypeScript extension keys and values through JSON conversion.
  Preserve collisions with renamed fields under enumerable symbol metadata and
  restore the original extension keys during `ToJSON` conversion.
- Convert typed additional values and model-alias chains recursively, including
  nullable aliases, arrays, nested dictionaries, and external-wrapper payloads.
- Plan compatible JSON/model `allOf` dictionaries together so declared fields do
  not leak under both wire and ergonomic names. Track imports for nested fallback
  intersection types.
- Preserve nested container, array-item, map-value, and alias nullability during
  OpenAPI 3.0, 3.1, and 3.2 lowering, with corresponding output across all nine
  generators.
- Accept and retain OpenAPI 3.0 boolean `exclusiveMinimum` and `exclusiveMaximum`
  flags while continuing to accept legacy numeric representations.
- Retain Gson union decoding behavior for nulls, typed alternatives, and raw
  fallbacks. Escape Kotlin reserved identifiers correctly.
- Propagate source-emission diagnostics with artifact context rather than losing
  the original cause or silently reporting generation success.

### Changed

- Upgrade `sigil-stitch` to 0.7.4 and `snafu` to 0.9.2. Use structured declarations,
  semantic types, and quoted source fragments throughout substantially migrated
  emitters; regenerated source may have formatting and import-layout changes.
- Generate Kotlin projects with Kotlin Gradle plugin 2.2.21 while retaining their
  JVM 17 toolchain target.
- Run native generated-client consumers and stricter fixture/golden validation on
  current toolchains. TypeScript consumers exercise packed public declarations and
  runtime conversions within the existing Vite+ job.

### Removed

- Remove the nightly release workflow and nightly installer guidance. Version-tag
  releases remain the supported prebuilt-binary publication path.

### Security

- Remove unused Swagger UI dependencies from the fixture generators, eliminating
  `zip 3.0.0` and 20 other unused packages without upgrading retained dependencies.
  This resolves the dependency behind GHSA-m665-v7pm-7grj, GHSA-q7v9-x93c-xqv3,
  and GHSA-8hxf-qr8p-62x4; GitHub has marked all three alerts fixed.

## [0.2.3]

### Added

- Add `just publish` and `just publish-dry-run` recipes for publishing the
  `openapi-nexus` crate to crates.io.

### Fixed

- Detect plain inline `oneOf` members with required single-value string enum
  tags as internally tagged unions in OpenAPI 3.0, 3.1, and 3.2. This preserves
  nested field conversion in TypeScript clients using camel-case properties.

## [0.2.2]

### Changed

- Store the private representation of generated Rust `ApiCallError` values behind
  a `Box`, making the common erased error pointer-sized. Derived `Debug` output now
  reflects the private inner wrapper.

## [0.2.1]

### Breaking Changes

- Rust generated clients now expose backend-native header maps from
  `ApiError::headers()` instead of lossy `Vec<(String, String)>` values.
  Successful Rust response structs also include a public native `headers`
  field.

### Added

- Add a common Rust `ApiCallError` with conversions from every generated
  operation error, while retaining operation-specific typed errors for callers
  that need to match individual responses.
- Preserve successful response headers across every generator. Python clients
  add `*_with_http_info()` methods while keeping existing body-only methods
  unchanged.
- Generate optional typed convenience accessors for OpenAPI-declared response
  headers on both success and error paths. Undeclared headers remain available
  through each backend's native response header collection.

### Fixed

- Resolve response-header component references during OpenAPI 3.0, 3.1, and
  3.2 lowering instead of silently dropping referenced headers.

## [0.2.0]

### Breaking Changes

- Generated clients now treat only HTTP `2xx` responses as success. Documented
  `4xx`, `5xx`, and `default` responses, plus unexpected non-`2xx` responses,
  use each language's native error path instead of returning a normal response.
- Success response types no longer contain non-success status fields such as
  `status_4xx` or `status_5xx`. Regenerate clients and move non-`2xx` handling
  to the generated operation-specific error type or exception.

### Added

- Generate operation-specific typed HTTP errors for TypeScript Fetch, Go HTTP,
  Rust reqwest, Rust ureq, Rust aioduct, Python httpx, Python requests, Java
  OkHttp, and Kotlin OkHttp.
- Preserve status, headers, and raw response bytes for HTTP errors, with typed
  response-entry details and deferred body decoding where supported.
- Distinguish exact statuses, wildcard ranges, default responses, and
  unexpected non-success responses even when entries reuse the same schema.

### Changed

- TypeScript Fetch Vite+ projects now require `"typescript": "^7.0.2"` and
  `"vite-plus": "^0.2.5"` instead of unbounded latest versions.

### Fixed

- TypeScript Fetch: keep self-referential model types and JSON converters local
  instead of generating imports from their own module.
- Python httpx and requests: keep self-referential model annotations local
  while retaining imports for other referenced models.

## [0.1.17]

### Changed

- TypeScript Fetch: emit runtime code without parameter properties so generated clients compile with `erasableSyntaxOnly`
- TypeScript Fetch: collapse API unions containing `unknown` to plain `unknown`

## [0.1.16]

### Added

- README: add a Codecov coverage badge for the `main` branch

### Changed

- Rust aioduct: support aioduct 0.2 runtime APIs with `HttpEngineSend`, `RuntimePoll`, `ConnectorSend`, and `RequestBuilderSend`
- Rust aioduct: default generated `Cargo.toml` files to the `aioduct = "0.2"` semver requirement so generated clients can pick up compatible 0.2.x releases
- Update package metadata, release links, and mdBook links to `rust-codegen-group/openapi-nexus` and the `main` branch

## [0.1.15]

### Fixed

- TypeScript Fetch: handle adjacent tagged-union payload keys correctly under camelCase property naming
- TypeScript Fetch: emit tag-only discriminated union variants without synthetic payload fields
- TypeScript Fetch: preserve nullable array and reference conversions with explicit null guards
- TypeScript Fetch: simplify redundant generated union type members

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
