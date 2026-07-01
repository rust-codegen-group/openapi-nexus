# Getting Started

## Installation

**Shell installer (no Rust toolchain needed):**

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/rust-codegen-group/openapi-nexus/releases/download/0.1.17/openapi-nexus-installer.sh | sh
```

**Nightly build (latest main):**

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/rust-codegen-group/openapi-nexus/releases/download/nightly/openapi-nexus-installer.sh | sh
```

**Build from source:**

```bash
cargo install openapi-nexus
```

Requires Rust 1.90+ (edition 2024).

## Basic Usage

Generate a TypeScript fetch client:

```bash
openapi-nexus generate \
  --input path/to/openapi.yaml \
  --output generated \
  --generators typescript-fetch
```

Generate another target language into a separate directory:

```bash
openapi-nexus generate \
  --input spec.yaml \
  --output output/go \
  --generators go-http

openapi-nexus generate \
  --input spec.yaml \
  --output output/python \
  --generators python-httpx
```

All nine generators, each with its own output directory:

```bash
for generator in \
  typescript-fetch \
  go-http \
  rust-reqwest \
  rust-ureq \
  rust-aioduct \
  python-httpx \
  python-requests \
  java-okhttp \
  kotlin-okhttp
do
  openapi-nexus generate -i spec.yaml -o "output/${generator}" -g "${generator}"
done
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
export OPENAPI_NEXUS_GENERATORS="typescript-fetch"
```

### Configuration File

Create an `openapi-nexus-config.toml` in your project root. See the [sample configuration file](https://github.com/rust-codegen-group/openapi-nexus/blob/main/openapi-nexus-config.toml.example) for all available options.

Generator-specific options live under `[generators.<name>]` sections:

```toml
[generators.go-http]
module_path = "github.com/myorg/myproject/sdk"

[generators.rust-reqwest]
crate_name = "my-api-client"
workspace_mode = true
workspace_deps = "workspace_version"  # "explicit" | "workspace_version" | "full"

[generators.rust-reqwest.extra_derives.structs]
derives = ["PartialEq"]

[generators.rust-reqwest.extra_derives.enums]
derives = ["Hash"]

[generators.rust-reqwest.utoipa]
enabled = true
dependency = '{ version = "5" }'

[generators.typescript-fetch]
emit_enum_constants = true
emit_type_guards = true
property_naming = "camelCase"
```

### CLI Reference

```bash
openapi-nexus generate --help
```
