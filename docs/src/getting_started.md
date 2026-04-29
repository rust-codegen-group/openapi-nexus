# Getting Started

## Installation

Download the latest binary from the [releases page](https://github.com/adamcavendish/openapi-nexus/releases), or build from source:

```bash
cargo install --path crates/openapi-nexus
```

Requires Rust 1.90+ (edition 2024).

## Basic Usage

Generate a TypeScript fetch client:

```bash
openapi-nexus generate \
  --input path/to/openapi.yaml \
  --output generated \
  --generator typescript-fetch
```

Generate both TypeScript and Go clients at once:

```bash
openapi-nexus generate \
  --input spec.yaml \
  --output output \
  --generators typescript-fetch,go-http
```

## Configuration

Configuration is resolved with the following precedence (highest to lowest):

1. **Command-line arguments**
2. **Environment variables** (prefixed with `OPENAPI_NEXUS_`)
3. **Configuration file** (`openapi-nexus-config.toml`)
4. **Defaults**

### Environment Variables

```bash
export OPENAPI_NEXUS_INPUT="spec.yaml"
export OPENAPI_NEXUS_OUTPUT="generated"
export OPENAPI_NEXUS_GENERATOR="typescript-fetch"
```

### Configuration File

Create an `openapi-nexus-config.toml` in your project root. See the [sample configuration file](https://github.com/adamcavendish/openapi-nexus/blob/master/openapi-nexus-config.toml.example) for all available options.

Generator-specific options live under `[generators.<name>]` sections:

```toml
[generators.go-http]
module_path = "github.com/myorg/myproject/sdk"
```

### CLI Reference

```bash
openapi-nexus generate --help
```
