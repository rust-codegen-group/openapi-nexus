# OpenAPI Nexus

> OpenAPI 3.0 / 3.1 / 3.2 to multi-language code generator

[![CI](https://github.com/rust-codegen-group/openapi-nexus/actions/workflows/ci.yml/badge.svg)](https://github.com/rust-codegen-group/openapi-nexus/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/rust-codegen-group/openapi-nexus/branch/main/graph/badge.svg)](https://codecov.io/gh/rust-codegen-group/openapi-nexus)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![Rust](https://img.shields.io/badge/rust-1.90+-orange.svg)](https://www.rust-lang.org/)

OpenAPI Nexus transforms OpenAPI specifications into type-safe client libraries. Generated output is deterministic, compile-checked in CI, and tested byte-for-byte via golden tests.

## Language Support

| Language | Generator | HTTP Client | Status |
|----------|-----------|-------------|--------|
| TypeScript | `typescript-fetch` | fetch | Beta |
| Go | `go-http` | net/http | Beta |
| Rust | `rust-reqwest` | reqwest | Beta |
| Rust | `rust-ureq` | ureq | Beta |
| Rust | `rust-aioduct` | aioduct | Beta |
| Python | `python-httpx` | httpx | Beta |
| Python | `python-requests` | requests | Beta |
| Java | `java-okhttp` | OkHttp | Beta |
| Kotlin | `kotlin-okhttp` | OkHttp | Beta |

## Quick Start

### Install

**Shell installer (no Rust toolchain needed):**

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/rust-codegen-group/openapi-nexus/releases/download/0.1.16/openapi-nexus-installer.sh | sh
```

**Nightly build (latest main):**

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/rust-codegen-group/openapi-nexus/releases/download/nightly/openapi-nexus-installer.sh | sh
```

**Build from source:**

```bash
cargo install openapi-nexus
```

Requires Rust 1.90+.

### Generate

```bash
# TypeScript client
openapi-nexus generate -i spec.yaml -o output --generators typescript-fetch

# Go client
openapi-nexus generate -i spec.yaml -o output --generators go-http

# Rust client (reqwest)
openapi-nexus generate -i spec.yaml -o output --generators rust-reqwest

# Python client (httpx)
openapi-nexus generate -i spec.yaml -o output --generators python-httpx

# Java client
openapi-nexus generate -i spec.yaml -o output --generators java-okhttp

# Generate another target into its own directory
openapi-nexus generate -i spec.yaml -o output/go --generators go-http
openapi-nexus generate -i spec.yaml -o output/rust --generators rust-reqwest
```

## Configuration

Configuration resolves in order: CLI args > environment variables (`OPENAPI_NEXUS_*`) > config file (`openapi-nexus-config.toml`) > defaults.

```bash
# Environment variables
export OPENAPI_NEXUS_INPUT="spec.yaml"
export OPENAPI_NEXUS_OUTPUT="generated"
export OPENAPI_NEXUS_GENERATORS="typescript-fetch"
```

Generator-specific options go in the config file:

```toml
[generators.go-http]
module_path = "github.com/myorg/myproject/sdk"

[generators.rust-reqwest]
workspace_mode = true
workspace_deps = "workspace_version"

[generators.rust-reqwest.extra_derives.structs]
derives = ["PartialEq"]

[generators.rust-reqwest.extra_derives.enums]
derives = ["Hash"]

[generators.rust-reqwest.utoipa]
enabled = true
dependency = '{ version = "5" }'

[generators.rust-aioduct.aioduct]
compression = ["gzip", "zstd"]
features = ["tracing"]

[generators.typescript-fetch]
emit_enum_constants = true
emit_type_guards = true
property_naming = "camelCase"
```

## How It Works

```
OpenAPI YAML/JSON → parse → lower to IR → CodeGenerator::generate(&IrSpec) → write
```

Parsing auto-detects OAS version (3.0, 3.1, 3.2). Lowering produces a version-agnostic `IrSpec`. Each generator receives the pre-lowered IR and uses [sigil-stitch](https://github.com/adamcavendish/sigil-stitch) for type-safe code emission.

## Documentation

Full documentation is available at the [project docs site](https://adamcavendish.github.io/openapi-nexus/).

Examples live under [`examples/`](examples/), including multipart file upload and raw binary body usage in [`examples/multipart-binary`](examples/multipart-binary/).

## Development

```bash
# Run all tests
cargo test

# Clippy (required before commit)
cargo clippy --all-targets --all-features -- -D warnings

# Update golden files after intentional output changes
UPDATE_GOLDEN=1 cargo test

# Compile-check generated output for all languages
just golden-build-all
```

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.
