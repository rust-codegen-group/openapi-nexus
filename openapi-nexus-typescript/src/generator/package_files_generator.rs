//! Package file generators for npm package structure

use heck::ToKebabCase as _;
use utoipa::openapi::OpenApi;

use crate::config::TsConfig;
use openapi_nexus_core::traits::file_writer::FileInfo;

/// Generator for npm package files
pub struct PackageFilesGenerator<'a> {
    config: &'a TsConfig,
}

impl<'a> PackageFilesGenerator<'a> {
    /// Create a new package files generator
    pub fn new(config: &'a TsConfig) -> Self {
        Self { config }
    }

    /// Generate package.json file from OpenAPI specification
    pub fn generate_package_json(&self, openapi: &OpenApi) -> FileInfo {
        // Extract metadata from OpenAPI spec
        let title = openapi.info.title.clone();
        let version = openapi.info.version.clone();
        let description = openapi
            .info
            .description
            .clone()
            .unwrap_or_else(|| format!("TypeScript client for {}", title));

        // Convert title to kebab-case for package name
        let package_name = title.to_kebab_case();
        let scoped_name = if let Some(scope) = &self.config.scope {
            format!("{}/{}", scope, package_name)
        } else {
            package_name
        };

        let mut package_json = serde_json::json!({
            "name": scoped_name,
            "version": version,
            "description": description,
            "type": "module",
            "main": "./dist/index.js",
            "types": "./dist/index.d.ts",
            "files": [
                "dist"
            ],
            "exports": {
                ".": {
                    "import": "./dist/index.js",
                    "types": "./dist/index.d.ts"
                }
            },
            "keywords": [
                "openapi",
                "api-client",
                "typescript"
            ],
            "license": "MIT"
        });

        // Add build scripts if configured
        if self.config.include_build_scripts {
            package_json["scripts"] = serde_json::json!({
                "build": "tsc",
                "build:esm": "tsc -p tsconfig.esm.json",
                "prepublishOnly": "npm run build"
            });
        }

        let content =
            serde_json::to_string_pretty(&package_json).unwrap_or_else(|_| "{}".to_string());

        FileInfo::project("package.json".to_string(), content)
    }

    /// Generate tsconfig.json file
    pub fn generate_tsconfig(&self, _openapi: &OpenApi) -> FileInfo {
        let module_str = self.config.typescript_module.to_string();

        let tsconfig = serde_json::json!({
            "compilerOptions": {
                "target": self.config.typescript_target,
                "module": module_str,
                "lib": ["ES2020", "DOM"],
                "declaration": true,
                "declarationMap": true,
                "sourceMap": true,
                "outDir": "./dist",
                "rootDir": "./",
                "moduleResolution": "node",
                "esModuleInterop": true,
                "skipLibCheck": true,
                "strict": true,
                "forceConsistentCasingInFileNames": true,
                "resolveJsonModule": true,
                "typeRoots": [
                    "node_modules/@types"
                ]
            },
            "include": [
                "**/*.ts"
            ],
            "exclude": [
                "dist",
                "node_modules",
                "**/*.test.ts",
                "**/*.spec.ts"
            ]
        });

        let content = serde_json::to_string_pretty(&tsconfig).unwrap_or_else(|_| "{}".to_string());

        FileInfo::project("tsconfig.json".to_string(), content)
    }

    /// Generate tsconfig.esm.json file for ES modules
    pub fn generate_tsconfig_esm(&self, _openapi: &OpenApi) -> FileInfo {
        let tsconfig_esm = serde_json::json!({
            "extends": "./tsconfig.json",
            "compilerOptions": {
                "module": "esnext",
                "outDir": "dist/esm"
            }
        });

        let content =
            serde_json::to_string_pretty(&tsconfig_esm).unwrap_or_else(|_| "{}".to_string());

        FileInfo::project("tsconfig.esm.json".to_string(), content)
    }
}
