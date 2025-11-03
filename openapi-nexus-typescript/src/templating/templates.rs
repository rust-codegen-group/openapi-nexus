//! Template name definitions and template emitter
//! Templates are loaded via minijinja_embed from build.rs

use minijinja::{Environment, context};

use super::environment::create_template_environment;
use crate::ast::{
    TsClassDefinition, TsExpression, TsInterfaceDefinition, TsInterfaceSignature, TsProperty,
};
use crate::emission::error::EmitError;
use crate::templating::data::{ApiOperationData, CommonFileHeaderData};
use openapi_nexus_core::traits::FileCategory;
use openapi_nexus_core::traits::file_writer::FileInfo;

/// Template name enum for type-safe template references
/// All templates used in the TypeScript generator must be declared here
/// Organized by FileCategory
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TemplateName {
    // FileCategory::Readme
    /// README documentation template
    Readme,

    // FileCategory::Apis
    /// Main API class template (generates complete API class files)
    ApiOperation,

    // FileCategory::Models
    /// Interface model template
    ModelInterface,
    /// Type alias model template
    ModelTypeAlias,
    /// Enum model template
    ModelEnum,

    // FileCategory::Runtime
    /// Runtime utilities template
    Runtime,

    // FileCategory::ProjectFiles
    /// Project index file template
    ProjectIndex,

    // FileCategory::None (Snippets/Partials)
    // These are included by other templates and not rendered directly
    /// File header template (used across all file types, included by other templates)
    CommonFileHeader,
    /// API method body: Constructor for base API class
    ApiConstructorBaseApi,
    /// API method body: GET request handler
    ApiMethodGet,
    /// API method body: POST/PUT/PATCH request handler
    ApiMethodPostPutPatch,
    /// API method body: DELETE request handler
    ApiMethodDelete,
    /// API method body: Convenience wrapper method
    ApiMethodConvenience,
    /// API method body: Default/fallback method handler
    ApiDefaultMethod,
    /// Partial: Build URL path snippet
    ApiBuildUrlPath,
    /// Partial: Build query parameters snippet
    ApiBuildQueryParams,
    /// Partial: Build request headers snippet
    ApiBuildHeaders,
    /// Partial: Build request body snippet
    ApiBuildRequestBody,
    /// Partial: Make HTTP request snippet
    ApiMakeRequest,
    /// Model helper functions template (instanceOf/FromJSON/ToJSON/validation)
    ModelInferenceHelpers,
}

impl TemplateName {
    /// Get the file path for this template (used for Minijinja template lookup)
    pub fn file_path(&self) -> &'static str {
        match self {
            // FileCategory::Readme
            Self::Readme => "README.md.j2",

            // FileCategory::Apis
            Self::ApiOperation => "api/operation.j2",

            // FileCategory::Models
            Self::ModelInterface => "model/interface.j2",
            Self::ModelTypeAlias => "model/type_alias.j2",
            Self::ModelEnum => "model/enum.j2",

            // FileCategory::Runtime
            Self::Runtime => "runtime/runtime.j2",

            // FileCategory::ProjectFiles
            Self::ProjectIndex => "project/index.j2",

            // FileCategory::None (Snippets/Partials)
            Self::CommonFileHeader => "common/file_header.j2",
            Self::ApiConstructorBaseApi => "api/snippets/constructor_base_api.j2",
            Self::ApiMethodGet => "api/snippets/api_method_get.j2",
            Self::ApiMethodPostPutPatch => "api/snippets/api_method_post_put_patch.j2",
            Self::ApiMethodDelete => "api/snippets/api_method_delete.j2",
            Self::ApiMethodConvenience => "api/snippets/api_method_convenience.j2",
            Self::ApiDefaultMethod => "api/snippets/default.j2",
            Self::ApiBuildUrlPath => "api/snippets/build_url_path.j2",
            Self::ApiBuildQueryParams => "api/snippets/build_query_params.j2",
            Self::ApiBuildHeaders => "api/snippets/build_headers.j2",
            Self::ApiBuildRequestBody => "api/snippets/build_request_body.j2",
            Self::ApiMakeRequest => "api/snippets/make_request.j2",
            Self::ModelInferenceHelpers => "model/snippets/interface_helpers.j2",
        }
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
        let template = self
            .env
            .get_template(template_name.file_path())
            .map_err(|e| EmitError::TemplateError {
                message: format!(
                    "Failed to get {} template: {}",
                    template_name.file_path(),
                    e
                ),
            })?;
        let content = template
            .render(context)
            .map_err(|e| EmitError::TemplateError {
                message: format!(
                    "Failed to render {} template: {}",
                    template_name.file_path(),
                    e
                ),
            })?;

        Ok(FileInfo::new(
            output_filename.to_string(),
            content,
            template_name.file_category(),
        ))
    }

    /// Emit TypeScript code from a class definition
    pub fn emit_class(
        &self,
        class: &TsClassDefinition,
        title: Option<&str>,
        description: Option<&str>,
        version: Option<&str>,
    ) -> Result<String, EmitError> {
        let class = class.clone();

        // Build interface signature (export interface FooInterface ...)
        let interface_signature =
            TsInterfaceSignature::new(format!("{}Interface", class.signature.name))
                .with_generics(class.signature.generics.clone());
        // Convert methods into function-typed properties for the interface
        let interface_properties: Vec<TsProperty> = class
            .methods
            .clone()
            .into_iter()
            .filter(|m| m.name != "constructor")
            .map(|m| {
                let func_type = TsExpression::Function {
                    parameters: m.parameters,
                    return_type: m.return_type.map(Box::new),
                };
                TsProperty {
                    name: m.name,
                    type_expr: func_type,
                    optional: false,
                    documentation: m.documentation,
                }
            })
            .collect();
        let api_interface =
            TsInterfaceDefinition::new(interface_signature).with_properties(interface_properties);

        let common_file_header = CommonFileHeaderData::new(
            title.map(|s| s.to_string()).unwrap_or_default(),
            description.map(|s| s.to_string()),
            version.map(|s| s.to_string()).unwrap_or_default(),
        );

        let imports = class.imports.clone();
        let api_operation = ApiOperationData::new(class, imports, api_interface);

        let template_data = context! {
            common_file_header,
            api_operation,
        };

        // Get the API class template and render directly
        let template = self
            .env
            .get_template(TemplateName::ApiOperation.file_path())
            .map_err(|e| EmitError::TemplateError {
                message: format!(
                    "Failed to get {} template: {}",
                    TemplateName::ApiOperation.file_path(),
                    e
                ),
            })?;

        template
            .render(template_data)
            .map_err(|e| EmitError::TemplateError {
                message: format!("Failed to render template: {}", e),
            })
    }
}
