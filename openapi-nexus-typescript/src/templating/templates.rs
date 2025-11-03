//! Template name definitions and template emitter
//! Templates are loaded via minijinja_embed from build.rs

use minijinja::Environment;
use serde::{Deserialize, Serialize};

use super::environment::create_template_environment;
use crate::emission::error::EmitError;
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

    // FileCategory::Models
    /// Interface model template
    #[serde(rename = "model/interface.j2")]
    ModelInterface,
    /// Type alias model template
    #[serde(rename = "model/type_alias.j2")]
    ModelTypeAlias,
    /// Enum model template
    #[serde(rename = "model/enum.j2")]
    ModelEnum,

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
    /// API method body: Default/fallback method handler
    #[serde(rename = "api/snippets/default.j2")]
    ApiDefaultMethod,
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
    /// Model helper functions template (instanceOf/FromJSON/ToJSON/validation)
    #[serde(rename = "model/snippets/interface_helpers.j2")]
    ModelInferenceHelpers,
}

impl TemplateName {
    /// Get the file path for this template (used for Minijinja template lookup)
    pub fn file_path(&self) -> String {
        serde_plain::to_string(self)
            .expect("TemplateName should always serialize to a valid string")
    }

    /// Get the file category for this template
    pub fn file_category(&self) -> FileCategory {
        match self {
            // FileCategory::Readme
            Self::Readme => FileCategory::Readme,

            // FileCategory::Apis
            Self::ApiOperation => FileCategory::Apis,

            // FileCategory::Models
            Self::ModelInterface => FileCategory::Models,
            Self::ModelTypeAlias => FileCategory::Models,
            Self::ModelEnum => FileCategory::Models,

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
            | Self::ApiDefaultMethod
            | Self::ApiBuildUrlPath
            | Self::ApiBuildQueryParams
            | Self::ApiBuildHeaders
            | Self::ApiBuildRequestBody
            | Self::ApiMakeRequest
            | Self::ModelInferenceHelpers => FileCategory::None,
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
    // FileCategory::Models
    TemplateName::ModelEnum,
    TemplateName::ModelInterface,
    TemplateName::ModelTypeAlias,
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
    TemplateName::ApiDefaultMethod,
    TemplateName::ApiMakeRequest,
    TemplateName::ApiMethodConvenience,
    TemplateName::ApiMethodDelete,
    TemplateName::ApiMethodGet,
    TemplateName::ApiMethodPostPutPatch,
    TemplateName::CommonFileHeader,
    TemplateName::ModelInferenceHelpers,
];

/// Template-based TypeScript code emitter and template handler
/// Templates are loaded via minijinja_embed from build.rs
#[derive(Debug, Clone)]
pub struct Templates {
    env: Environment<'static>,
}

impl Default for Templates {
    fn default() -> Self {
        Self::new()
    }
}

impl Templates {
    /// Create a new template handler with initialized templates
    /// Each instance has its own Environment (not shared)
    /// Templates are loaded via minijinja_embed from build.rs
    pub fn new() -> Self {
        let env = create_template_environment();
        Self { env }
    }

    pub fn render_template(
        &self,
        template_name: TemplateName,
        output_filename: &str,
        context: minijinja::Value,
    ) -> Result<FileInfo, EmitError> {
        let template_path = template_name.file_path();
        let template =
            self.env
                .get_template(&template_path)
                .map_err(|e| EmitError::TemplateError {
                    message: format!("Failed to get {} template: {}", template_path, e),
                })?;
        let content = template
            .render(context)
            .map_err(|e| EmitError::TemplateError {
                message: format!("Failed to render {} template: {}", template_path, e),
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
    ) -> Result<String, EmitError> {
        let template_path = template_name.file_path();
        let template =
            self.env
                .get_template(&template_path)
                .map_err(|e| EmitError::TemplateError {
                    message: format!("Failed to get {} template: {}", template_path, e),
                })?;
        template
            .render(context)
            .map_err(|e| EmitError::TemplateError {
                message: format!("Failed to render {} template: {}", template_path, e),
            })
    }
}
