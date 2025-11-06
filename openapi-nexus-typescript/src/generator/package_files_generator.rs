//! Package file generators for npm package structure

use heck::ToKebabCase as _;
use utoipa::openapi::OpenApi;

use openapi_nexus_config::TypeScriptConfig;
use openapi_nexus_core::traits::file_writer::FileInfo;

/// Generator for npm package files
pub struct PackageFilesGenerator<'a> {
    config: &'a TypeScriptConfig,
}

impl<'a> PackageFilesGenerator<'a> {
    /// Create a new package files generator
    pub fn new(config: &'a TypeScriptConfig) -> Self {
        Self { config }
    }

    /// Extract keywords from OpenAPI extensions
    ///
    /// Returns keywords from `x-keywords` extension if present, otherwise None.
    /// If None is returned, default keywords should be used.
    fn extract_keywords(&self, openapi: &OpenApi) -> Option<Vec<String>> {
        if let Some(extensions) = &openapi.info.extensions
            && let Some(serde_json::Value::Array(keywords_array)) = extensions.get("x-keywords")
        {
            let keywords: Vec<String> = keywords_array
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            if !keywords.is_empty() {
                return Some(keywords);
            }
        }
        None
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

        // Extract license from OpenAPI info.license (only include if present)
        let license: Option<String> = openapi.info.license.as_ref().map(|l| l.name.clone());

        // Extract keywords - use extension keywords if found, otherwise use defaults
        let keywords = self.extract_keywords(openapi).unwrap_or_else(|| {
            vec![
                "openapi".to_string(),
                "api-client".to_string(),
                "typescript".to_string(),
                "generated".to_string(),
            ]
        });

        // Use configured package name or derive from title
        let package_name = self
            .config
            .package_name
            .clone()
            .unwrap_or_else(|| title.to_kebab_case());
        let scoped_name = if let Some(scope) = &self.config.package_scope {
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
            "exports": {
                ".": {
                    "types": "./dist/index.d.ts",
                    "default": "./dist/index.js"
                }
            },
            "files": [
                "dist"
            ],
            "keywords": keywords
        });

        // Only include license if it was found
        if let Some(license_name) = license {
            package_json["license"] = serde_json::Value::String(license_name);
        }

        // Add build scripts if configured
        if self.config.include_build_scripts {
            package_json["scripts"] = serde_json::json!({
                "build": "tsc",
            });
        }

        let content =
            serde_json::to_string_pretty(&package_json).unwrap_or_else(|_| "{}".to_string());

        FileInfo::project("package.json".to_string(), content)
    }

    /// Generate tsconfig.json file
    pub fn generate_tsconfig(&self, _openapi: &OpenApi) -> FileInfo {
        let module_str = self.config.ts_module.to_string();

        let mut tsconfig = serde_json::json!({
            "compilerOptions": {
                "target": self.config.ts_target,
                "module": module_str,
                "declaration": true,
                "declarationMap": true,
                "sourceMap": true,
                "outDir": "./dist",
                "rootDir": "./",
                "moduleResolution": "bundler",
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
                "node_modules"
            ]
        });
        // Add lib field
        tsconfig["compilerOptions"]["lib"] = serde_json::json!(self.config.ts_lib);

        let content = serde_json::to_string_pretty(&tsconfig).unwrap_or_else(|_| "{}".to_string());

        FileInfo::project("tsconfig.json".to_string(), content)
    }

    /// Generate tsconfig.esm.json file for ES modules
    pub fn generate_tsconfig_esm(&self, _openapi: &OpenApi) -> FileInfo {
        let module_str = self.config.ts_module.to_string();
        let tsconfig_esm = serde_json::json!({
            "extends": "./tsconfig.json",
            "compilerOptions": {
                "module": module_str,
                "outDir": "dist/esm"
            }
        });

        let content =
            serde_json::to_string_pretty(&tsconfig_esm).unwrap_or_else(|_| "{}".to_string());

        FileInfo::project("tsconfig.esm.json".to_string(), content)
    }
}
