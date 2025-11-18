# 🚀 OpenAPI Nexus (Working In Progress)

> **OpenAPI 3.1 to Code Generator** - Generate type-safe, production-ready code from OpenAPI specifications

[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![Rust](https://img.shields.io/badge/rust-1.90+-orange.svg)](https://www.rust-lang.org/)

OpenAPI Nexus is a modern, modular code generator that transforms OpenAPI 3.1 specifications into client or server libraries. It provides a flexible, extensible architecture for generating high-quality code across multiple languages.

## ✨ Features

- 🎯 **OpenAPI 3.1 Support** - Support for OpenAPI 3.1 specifications
- 🏗️ **Modular Architecture** - Extensible pipeline design allows easy addition of new languages and transformations
- 📦 **Multi-file Output** - Organized output structure with separate API, model, and runtime modules
- 🎨 **Configurable** - Flexible configuration via CLI, environment variables, or config files
- 🔄 **Transform Pipeline** - Built-in transformation passes for normalization and optimization (Working In Progress)
- 📝 **Template-based** - Jinja2-style templates for customizable code generation

## 🚦 Quick Start

### Installation

1. Download the binary from the [releases page](https://github.com/adamcavendish/openapi-nexus/releases).

### Basic Usage

Generate TypeScript code from an OpenAPI specification:

```bash
openapi-nexus generate --input path/to/openapi.yaml --output generated --generator typescript-fetch
```

## 📖 Configuration

OpenAPI Nexus supports multiple configuration methods with the following precedence (highest to lowest):

1. **Command-line arguments**
2. **Environment variables**
3. **Configuration file** (`openapi-nexus-config.toml`)
4. **Defaults**

### Configuration File (Optional)

Create an `openapi-nexus-config.toml` file in your project root by referencing the [sample configuration file](./openapi-nexus-config.toml.example).

### Environment Variables

All configuration options can also be set via environment variables:

```bash
export OPENAPI_NEXUS_INPUT="spec.yaml"
export OPENAPI_NEXUS_OUTPUT="generated"
export OPENAPI_NEXUS_GENERATOR="typescript-fetch"
export OPENAPI_NEXUS_TS_FILE_NAMING_CONVENTION="PascalCase"
```

### CLI Options

```bash
openapi-nexus generate --help
```

## 🗂️ Language Support

- ✅ TypeScript
- 🚧 Rust
- 🚧 Python
- 🚧 Go
- ...

## 🏛️ Architecture

OpenAPI Nexus follows a modular, pipeline-based architecture:

```text
OpenAPI Spec → Parse → Transform → AST → Emit → Generated Code
```

### Pipeline Stages

1. **Parse** - Converts OpenAPI YAML/JSON to internal representation using utoipa
2. **Transform** - Applies extra modifications to the OpenAPI specification
3. **AST Generation** - Converts to language-specific partial Abstract Syntax Trees
4. **Emission** – Produces formatted source code from ASTs using a hybrid templating approach

## 📄 License

This project is dual-licensed under either:

- MIT License ([LICENSE-MIT](LICENSE-MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))

at your option.
