//! Template name definitions and template emitter
//! Templates are loaded via minijinja_embed from build.rs

use minijinja::Environment;
use serde::{Deserialize, Serialize};

use super::environment::create_template_environment;
use crate::errors::GeneratorError;
use openapi_nexus_common::GeneratorType;
use openapi_nexus_core::traits::FileCategory;
use openapi_nexus_core::traits::file_writer::FileInfo;

/// Template name enum for type-safe template references
/// All templates used in the TypeScript generator must be declared here
/// Organized by FileCategory
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TemplateName {
    // FileCategory::Readme
    /// README documentation template
    #[serde(rename = "README.md.j2")]
    Readme,

    // FileCategory::Apis
    /// Main API class template (generates complete API class files)
    #[serde(rename = "api/operation.j2")]
    ApiOperation,

    // FileCategory::Runtime
    /// Runtime utilities template
    #[serde(rename = "runtime/runtime.j2")]
    Runtime,

    // FileCategory::ProjectFiles
    /// Project index file template
    #[serde(rename = "project/index.j2")]
    ProjectIndex,

    // FileCategory::None (Snippets/Partials)
    // These are included by other templates and not rendered directly
    /// File header template (used across all file types, included by other templates)
    #[serde(rename = "common/file_header.j2")]
    CommonFileHeader,
    /// API method body: Constructor for base API class
    #[serde(rename = "api/snippets/constructor_base_api.j2")]
    ApiConstructorBaseApi,
    /// API method body: GET request handler
    #[serde(rename = "api/snippets/method_get.j2")]
    ApiMethodGet,
    /// API method body: POST/PUT/PATCH request handler
    #[serde(rename = "api/snippets/method_post_put_patch.j2")]
    ApiMethodPostPutPatch,
    /// API method body: DELETE request handler
    #[serde(rename = "api/snippets/method_delete.j2")]
    ApiMethodDelete,
    /// API method body: Convenience wrapper method
    #[serde(rename = "api/snippets/method_convenience.j2")]
    ApiMethodConvenience,
    /// Partial: Build URL path snippet
    #[serde(rename = "api/snippets/build_url_path.j2")]
    ApiBuildUrlPath,
    /// Partial: Build query parameters snippet
    #[serde(rename = "api/snippets/build_query_params.j2")]
    ApiBuildQueryParams,
    /// Partial: Build request headers snippet
    #[serde(rename = "api/snippets/build_headers.j2")]
    ApiBuildHeaders,
    /// Partial: Build request body snippet
    #[serde(rename = "api/snippets/build_request_body.j2")]
    ApiBuildRequestBody,
    /// Partial: Make HTTP request snippet
    #[serde(rename = "api/snippets/make_request.j2")]
    ApiMakeRequest,
}

impl TemplateName {
    /// Get the file path for this template (used for Minijinja template lookup)
    pub fn file_path(&self) -> String {
        serde_plain::to_string(self)
            .expect("TemplateName should always serialize to a valid string")
    }

    /// Resolve the template path with generator prefix if needed
    /// All entry templates (those with a FileCategory) live in the generator-specific directory
    /// Snippets (FileCategory::None) remain at the root as they are included by entry templates
    pub fn resolve_path(&self, generator_name: &str) -> String {
        let path = self.file_path();
        if self.file_category() != FileCategory::None {
            format!("{}/{}", generator_name, path)
        } else {
            path
        }
    }

    /// Get the file category for this template
    pub fn file_category(&self) -> FileCategory {
        match self {
            // FileCategory::Readme
            Self::Readme => FileCategory::Readme,

            // FileCategory::Apis
            Self::ApiOperation => FileCategory::Apis,

            // FileCategory::Runtime
            Self::Runtime => FileCategory::Runtime,

            // FileCategory::ProjectFiles
            Self::ProjectIndex => FileCategory::ProjectFiles,

            // FileCategory::None (Snippets/Partials)
            // These are included by other templates and not rendered directly
            Self::CommonFileHeader
            | Self::ApiConstructorBaseApi
            | Self::ApiMethodGet
            | Self::ApiMethodPostPutPatch
            | Self::ApiMethodDelete
            | Self::ApiMethodConvenience
            | Self::ApiBuildUrlPath
            | Self::ApiBuildQueryParams
            | Self::ApiBuildHeaders
            | Self::ApiBuildRequestBody
            | Self::ApiMakeRequest => FileCategory::None,
        }
    }
}

/// Template file path mapping using type-safe enum
/// Organized by FileCategory for easier tracking
pub const TEMPLATE_PATHS: &[TemplateName] = &[
    // FileCategory::Readme
    TemplateName::Readme,
    // FileCategory::Apis
    TemplateName::ApiOperation,
    // FileCategory::Runtime
    TemplateName::Runtime,
    // FileCategory::ProjectFiles
    TemplateName::ProjectIndex,
    // FileCategory::None (Snippets/Partials)
    TemplateName::ApiBuildHeaders,
    TemplateName::ApiBuildQueryParams,
    TemplateName::ApiBuildRequestBody,
    TemplateName::ApiBuildUrlPath,
    TemplateName::ApiConstructorBaseApi,
    TemplateName::ApiMakeRequest,
    TemplateName::ApiMethodConvenience,
    TemplateName::ApiMethodDelete,
    TemplateName::ApiMethodGet,
    TemplateName::ApiMethodPostPutPatch,
    TemplateName::CommonFileHeader,
];

/// Template-based TypeScript code emitter and template handler
/// Templates are loaded via minijinja_embed from build.rs
#[derive(Debug, Clone)]
pub struct Templates {
    env: Environment<'static>,
    generator_name: String,
}

impl Templates {
    /// Create a new template handler with initialized templates for a specific generator
    /// Each instance has its own Environment (not shared)
    /// Templates are loaded via minijinja_embed from build.rs
    pub fn new(generator: GeneratorType) -> Self {
        let env = create_template_environment();
        let generator_name = generator.to_string();
        Self {
            env,
            generator_name,
        }
    }

    pub fn render_template(
        &self,
        template_name: TemplateName,
        output_filename: &str,
        context: minijinja::Value,
    ) -> Result<FileInfo, GeneratorError> {
        let template_path = template_name.resolve_path(&self.generator_name);
        let template = self.env.get_template(&template_path).map_err(|e| {
            GeneratorError::TemplateNotFound {
                template_path: template_path.clone(),
                source: e,
            }
        })?;
        let content = template
            .render(context)
            .map_err(|e| GeneratorError::TemplateRender {
                template_path: template_path.clone(),
                source: e,
            })?;

        Ok(FileInfo::new(
            output_filename.to_string(),
            content,
            template_name.file_category(),
        ))
    }

    /// Render a template and return the content as a string
    pub fn render_template_string(
        &self,
        template_name: TemplateName,
        context: minijinja::Value,
    ) -> Result<String, GeneratorError> {
        let template_path = template_name.resolve_path(&self.generator_name);
        let template = self.env.get_template(&template_path).map_err(|e| {
            GeneratorError::TemplateNotFound {
                template_path: template_path.clone(),
                source: e,
            }
        })?;
        template
            .render(context)
            .map_err(|e| GeneratorError::TemplateRender {
                template_path: template_path.clone(),
                source: e,
            })
    }
}
