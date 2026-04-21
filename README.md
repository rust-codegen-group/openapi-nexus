# OpenAPI Nexus

> OpenAPI 3.1 to multi-language code generator

[![CI](https://github.com/adamcavendish/openapi-nexus/actions/workflows/ci.yml/badge.svg)](https://github.com/adamcavendish/openapi-nexus/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![Rust](https://img.shields.io/badge/rust-1.90+-orange.svg)](https://www.rust-lang.org/)

OpenAPI Nexus transforms OpenAPI 3.1 specifications into type-safe client libraries. Generated output is deterministic, compile-checked in CI, and tested byte-for-byte via golden tests.

## Language Support

| Language | Generator | Status |
|----------|-----------|--------|
| TypeScript (fetch) | `typescript-fetch` | Stable |
| Go (net/http) | `go-http` | Stable |

## Quick Start

### Install

Download a binary from the [releases page](https://github.com/adamcavendish/openapi-nexus/releases), or build from source:

```bash
cargo install --path crates/openapi-nexus
```

Requires Rust 1.90+.

### Generate

```bash
# TypeScript client
openapi-nexus generate -i spec.yaml -o output -g typescript-fetch

# Go client
openapi-nexus generate -i spec.yaml -o output -g go-http

# Both at once
openapi-nexus generate -i spec.yaml -o output -g typescript-fetch,go-http
```

## Configuration

Configuration resolves in order: CLI args > environment variables (`OPENAPI_NEXUS_*`) > config file (`openapi-nexus-config.toml`) > defaults.

```bash
# Environment variables
export OPENAPI_NEXUS_INPUT="spec.yaml"
export OPENAPI_NEXUS_OUTPUT="generated"
export OPENAPI_NEXUS_GENERATOR="typescript-fetch"
```

Generator-specific options go in the config file:

```toml
[generators.go-http]
module_path = "github.com/myorg/myproject/sdk"
```

## How It Works

```
OpenAPI YAML/JSON → parse → lower to IR → CodeGenerator::generate(&IrSpec) → write
```

Parsing auto-detects OAS version. Lowering produces a version-agnostic `IrSpec`. Each generator receives the pre-lowered IR and uses [sigil-stitch](https://github.com/adamcavendish/sigil-stitch) for type-safe code emission.

## Documentation

Full documentation is available at the [project docs site](https://adamcavendish.github.io/openapi-nexus/).

## Development

```bash
# Run all tests
cargo test

# Clippy (required before commit)
cargo clippy --all-targets --all-features -- -D warnings

# Update golden files after intentional output changes
UPDATE_GOLDEN=1 cargo test

# Compile-check generated output
just golden::build-all
```

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.
