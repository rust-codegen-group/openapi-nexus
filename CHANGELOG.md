# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
