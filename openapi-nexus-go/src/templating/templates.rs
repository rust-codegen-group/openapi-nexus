//! Template name definitions and template emitter
//! Templates are loaded via minijinja_embed from build.rs

use minijinja::Environment;
use serde::{Deserialize, Serialize};

use super::environment::create_template_environment;
use crate::errors::GeneratorError;
use openapi_nexus_core::traits::FileCategory;
use openapi_nexus_core::traits::file_writer::FileInfo;

/// Template name enum for type-safe template references
/// All templates used in the Go generator must be declared here
/// Organized by FileCategory
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TemplateName {
    // FileCategory::Readme
    /// README documentation template
    #[serde(rename = "go-http/README.md.j2")]
    Readme,

    // FileCategory::Apis
    /// Main API class template (generates complete API class files)
    #[serde(rename = "go-http/api/operation.j2")]
    ApiOperation,
    /// Main SDK file template
    #[serde(rename = "go-http/project/main_sdk.j2")]
    MainSdk,

    // FileCategory::Models
    /// Struct model template
    #[serde(rename = "go-http/model/struct.j2")]
    ModelStruct,
    /// Type alias model template
    #[serde(rename = "go-http/model/type_alias.j2")]
    ModelTypeAlias,

    // FileCategory::Runtime
    /// Runtime utilities template
    #[serde(rename = "go-http/runtime/runtime.j2")]
    Runtime,
    /// Runtime utils.go
    #[serde(rename = "go-http/runtime/utils.j2")]
    RuntimeUtils,
    /// Runtime requestbody.go
    #[serde(rename = "go-http/runtime/requestbody.j2")]
    RuntimeRequestBody,
    /// Runtime queryparams.go
    #[serde(rename = "go-http/runtime/queryparams.j2")]
    RuntimeQueryParams,
    /// Runtime pathparams.go
    #[serde(rename = "go-http/runtime/pathparams.j2")]
    RuntimePathParams,
    /// Runtime headers.go
    #[serde(rename = "go-http/runtime/headers.j2")]
    RuntimeHeaders,
    /// Runtime json.go
    #[serde(rename = "go-http/runtime/json.j2")]
    RuntimeJson,
    /// Runtime contenttype.go
    #[serde(rename = "go-http/runtime/contenttype.j2")]
    RuntimeContentType,
    /// Runtime form.go
    #[serde(rename = "go-http/runtime/form.j2")]
    RuntimeForm,
    /// Runtime retries.go
    #[serde(rename = "go-http/runtime/retries.j2")]
    RuntimeRetries,
    /// Runtime security.go
    #[serde(rename = "go-http/runtime/security.j2")]
    RuntimeSecurity,
    /// Runtime env.go
    #[serde(rename = "go-http/runtime/env.j2")]
    RuntimeEnv,
    /// Runtime config.go
    #[serde(rename = "go-http/runtime/config.j2")]
    RuntimeConfig,
    /// Runtime retry_config.go
    #[serde(rename = "go-http/runtime/retry_config.j2")]
    RuntimeRetryConfig,
    /// Runtime hooks.go
    #[serde(rename = "go-http/runtime/hooks.j2")]
    RuntimeHooks,
    /// Runtime hooks_registration.j2
    #[serde(rename = "go-http/runtime/hooks_registration.j2")]
    RuntimeHooksRegistration,
    /// Types pointers.go
    #[serde(rename = "go-http/types/pointers.j2")]
    TypesPointers,
    /// Types date.go
    #[serde(rename = "go-http/types/date.j2")]
    TypesDate,
    /// Types datetime.go
    #[serde(rename = "go-http/types/datetime.j2")]
    TypesDateTime,
    /// Types bigint.go
    #[serde(rename = "go-http/types/bigint.j2")]
    TypesBigInt,
    /// Types optionalnullable.go
    #[serde(rename = "go-http/types/optionalnullable.j2")]
    TypesOptionalNullable,

    // FileCategory::ProjectFiles
    /// go.mod file template
    #[serde(rename = "go-http/project/go_mod.j2")]
    GoMod,

    // FileCategory::None (Snippets/Partials)
    // These are included by other templates and not rendered directly
    /// File header template (used across all file types, included by other templates)
    #[serde(rename = "common/file_header.j2")]
    CommonFileHeader,
    /// API method body: GET request handler
    #[serde(rename = "api/snippets/method_get.j2")]
    ApiMethodGet,
    /// API method body: POST request handler
    #[serde(rename = "api/snippets/method_post.j2")]
    ApiMethodPost,
    /// API method body: PUT request handler
    #[serde(rename = "api/snippets/method_put.j2")]
    ApiMethodPut,
    /// API method body: PATCH request handler
    #[serde(rename = "api/snippets/method_patch.j2")]
    ApiMethodPatch,
    /// API method body: DELETE request handler
    #[serde(rename = "api/snippets/method_delete.j2")]
    ApiMethodDelete,
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
    /// Partial: Return response snippet
    #[serde(rename = "api/snippets/return_response.j2")]
    ApiReturnResponse,
    /// Partial: Validate required parameters snippet
    #[serde(rename = "api/snippets/validate_required_params.j2")]
    ApiValidateRequiredParams,
    /// Client constructor function
    #[serde(rename = "api/snippets/constructor_client.j2")]
    ApiConstructorClient,
}

impl TemplateName {
    /// Get the file path for this template (used for Minijinja template lookup)
    pub fn file_path(&self) -> String {
        match self {
            TemplateName::Readme => "go-http/README.md.j2".to_string(),
            TemplateName::ApiOperation => "go-http/api/operation.j2".to_string(),
            TemplateName::MainSdk => "go-http/project/main_sdk.j2".to_string(),
            TemplateName::ModelStruct => "go-http/model/struct.j2".to_string(),
            TemplateName::ModelTypeAlias => "go-http/model/type_alias.j2".to_string(),
            TemplateName::Runtime => "go-http/runtime/runtime.j2".to_string(),
            TemplateName::RuntimeUtils => "go-http/runtime/utils.j2".to_string(),
            TemplateName::RuntimeRequestBody => "go-http/runtime/requestbody.j2".to_string(),
            TemplateName::RuntimeQueryParams => "go-http/runtime/queryparams.j2".to_string(),
            TemplateName::RuntimePathParams => "go-http/runtime/pathparams.j2".to_string(),
            TemplateName::RuntimeHeaders => "go-http/runtime/headers.j2".to_string(),
            TemplateName::RuntimeJson => "go-http/runtime/json.j2".to_string(),
            TemplateName::RuntimeContentType => "go-http/runtime/contenttype.j2".to_string(),
            TemplateName::RuntimeForm => "go-http/runtime/form.j2".to_string(),
            TemplateName::RuntimeRetries => "go-http/runtime/retries.j2".to_string(),
            TemplateName::RuntimeSecurity => "go-http/runtime/security.j2".to_string(),
            TemplateName::RuntimeEnv => "go-http/runtime/env.j2".to_string(),
            TemplateName::RuntimeConfig => "go-http/runtime/config.j2".to_string(),
            TemplateName::RuntimeRetryConfig => "go-http/runtime/retry_config.j2".to_string(),
            TemplateName::RuntimeHooks => "go-http/runtime/hooks.j2".to_string(),
            TemplateName::RuntimeHooksRegistration => {
                "go-http/runtime/hooks_registration.j2".to_string()
            }
            TemplateName::TypesPointers => "go-http/types/pointers.j2".to_string(),
            TemplateName::TypesDate => "go-http/types/date.j2".to_string(),
            TemplateName::TypesDateTime => "go-http/types/datetime.j2".to_string(),
            TemplateName::TypesBigInt => "go-http/types/bigint.j2".to_string(),
            TemplateName::TypesOptionalNullable => "go-http/types/optionalnullable.j2".to_string(),
            TemplateName::GoMod => "go-http/project/go_mod.j2".to_string(),
            TemplateName::CommonFileHeader => "common/file_header.j2".to_string(),
            TemplateName::ApiMethodGet => "api/snippets/method_get.j2".to_string(),
            TemplateName::ApiMethodPost => "api/snippets/method_post.j2".to_string(),
            TemplateName::ApiMethodPut => "api/snippets/method_put.j2".to_string(),
            TemplateName::ApiMethodPatch => "api/snippets/method_patch.j2".to_string(),
            TemplateName::ApiMethodDelete => "api/snippets/method_delete.j2".to_string(),
            TemplateName::ApiBuildUrlPath => "api/snippets/build_url_path.j2".to_string(),
            TemplateName::ApiBuildQueryParams => "api/snippets/build_query_params.j2".to_string(),
            TemplateName::ApiBuildHeaders => "api/snippets/build_headers.j2".to_string(),
            TemplateName::ApiBuildRequestBody => "api/snippets/build_request_body.j2".to_string(),
            TemplateName::ApiMakeRequest => "api/snippets/make_request.j2".to_string(),
            TemplateName::ApiReturnResponse => "api/snippets/return_response.j2".to_string(),
            TemplateName::ApiValidateRequiredParams => {
                "api/snippets/validate_required_params.j2".to_string()
            }
            TemplateName::ApiConstructorClient => "api/snippets/constructor_client.j2".to_string(),
        }
    }

    /// Get the file category for this template
    pub fn file_category(&self) -> FileCategory {
        match self {
            // FileCategory::Readme
            Self::Readme => FileCategory::Readme,

            // FileCategory::Apis
            Self::ApiOperation | Self::MainSdk => FileCategory::Apis,

            // FileCategory::Models
            Self::ModelStruct | Self::ModelTypeAlias => FileCategory::Models,

            // FileCategory::Runtime
            Self::Runtime
            | Self::RuntimeUtils
            | Self::RuntimeRequestBody
            | Self::RuntimeQueryParams
            | Self::RuntimePathParams
            | Self::RuntimeHeaders
            | Self::RuntimeJson
            | Self::RuntimeContentType
            | Self::RuntimeForm
            | Self::RuntimeRetries
            | Self::RuntimeSecurity
            | Self::RuntimeEnv
            | Self::RuntimeConfig
            | Self::RuntimeRetryConfig
            | Self::RuntimeHooks
            | Self::RuntimeHooksRegistration
            | Self::TypesPointers
            | Self::TypesDate
            | Self::TypesDateTime
            | Self::TypesBigInt
            | Self::TypesOptionalNullable => FileCategory::Runtime,

            // FileCategory::ProjectFiles
            Self::GoMod => FileCategory::ProjectFiles,

            // FileCategory::None (Snippets/Partials)
            Self::CommonFileHeader
            | Self::ApiMethodGet
            | Self::ApiMethodPost
            | Self::ApiMethodPut
            | Self::ApiMethodPatch
            | Self::ApiMethodDelete
            | Self::ApiBuildUrlPath
            | Self::ApiBuildQueryParams
            | Self::ApiBuildHeaders
            | Self::ApiBuildRequestBody
            | Self::ApiMakeRequest
            | Self::ApiReturnResponse
            | Self::ApiValidateRequiredParams
            | Self::ApiConstructorClient => FileCategory::None,
        }
    }
}

/// Template-based Go code emitter and template handler
/// Templates are loaded via minijinja_embed from build.rs
#[derive(Debug, Clone)]
pub struct Templates {
    env: Environment<'static>,
}

impl Templates {
    /// Create a new template handler with initialized templates for a specific generator
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
    ) -> Result<FileInfo, GeneratorError> {
        let template_path = template_name.file_path();
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
        let template_path = template_name.file_path();
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

impl Default for Templates {
    fn default() -> Self {
        Self::new()
    }
}
