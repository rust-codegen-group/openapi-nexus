//! Template name definitions and template emitter
//! Templates are loaded via minijinja_embed from build.rs

use minijinja::{Environment, context};

use super::environment::create_template_environment;
use crate::ast::{
    TsClassDefinition, TsExpression, TsInterfaceDefinition, TsInterfaceSignature, TsProperty,
    TsTypeDefinition,
};
use crate::emission::error::EmitError;
use openapi_nexus_core::data::HeaderData;
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
    ApiClass,

    // FileCategory::Models
    /// Model file template (type definition with helpers)
    Model,
    /// Model helper functions template (instanceOf/FromJSON/ToJSON/validation)
    ModelHelpers,

    // FileCategory::Runtime
    /// Runtime utilities template
    Runtime,

    // FileCategory::None (Snippets/Partials)
    // These are included by other templates and not rendered directly
    /// File header template (used across all file types, included by other templates)
    FileHeader,
    /// API method body: Constructor for base API class
    ConstructorBaseApi,
    /// API method body: GET request handler
    ApiMethodGet,
    /// API method body: POST/PUT/PATCH request handler
    ApiMethodPostPutPatch,
    /// API method body: DELETE request handler
    ApiMethodDelete,
    /// API method body: Convenience wrapper method
    ApiMethodConvenience,
    /// API method body: Default/fallback method handler
    DefaultMethod,
    /// Partial: Build URL path snippet
    BuildUrlPath,
    /// Partial: Build query parameters snippet
    BuildQueryParams,
    /// Partial: Build request headers snippet
    BuildHeaders,
    /// Partial: Build request body snippet
    BuildRequestBody,
    /// Partial: Make HTTP request snippet
    MakeRequest,
}

impl TemplateName {
    /// Get the file path for this template (used for Minijinja template lookup)
    pub fn file_path(&self) -> &'static str {
        match self {
            // FileCategory::Readme
            Self::Readme => "README.md.j2",

            // FileCategory::Apis
            Self::ApiClass => "api/api_class.j2",

            // FileCategory::Models
            Self::Model => "models/model.j2",
            Self::ModelHelpers => "models/model_helpers.j2",

            // FileCategory::Runtime
            Self::Runtime => "runtime/runtime.j2",

            // FileCategory::None (Snippets/Partials)
            Self::FileHeader => "common/file_header.j2",
            Self::ConstructorBaseApi => "api/method_bodies/constructor_base_api.j2",
            Self::ApiMethodGet => "api/method_bodies/api_method_get.j2",
            Self::ApiMethodPostPutPatch => "api/method_bodies/api_method_post_put_patch.j2",
            Self::ApiMethodDelete => "api/method_bodies/api_method_delete.j2",
            Self::ApiMethodConvenience => "api/method_bodies/api_method_convenience.j2",
            Self::DefaultMethod => "api/method_bodies/default.j2",
            Self::BuildUrlPath => "api/method_bodies/partials/_build_url_path.j2",
            Self::BuildQueryParams => "api/method_bodies/partials/_build_query_params.j2",
            Self::BuildHeaders => "api/method_bodies/partials/_build_headers.j2",
            Self::BuildRequestBody => "api/method_bodies/partials/_build_request_body.j2",
            Self::MakeRequest => "api/method_bodies/partials/_make_request.j2",
        }
    }

    /// Get the file category for this template
    pub fn file_category(&self) -> FileCategory {
        match self {
            // FileCategory::Readme
            Self::Readme => FileCategory::Readme,

            // FileCategory::Apis
            Self::ApiClass => FileCategory::Apis,

            // FileCategory::Models
            Self::Model => FileCategory::Models,
            Self::ModelHelpers => FileCategory::Models,

            // FileCategory::Runtime
            Self::Runtime => FileCategory::Runtime,

            // FileCategory::None (Snippets/Partials)
            // These are included by other templates and not rendered directly
            Self::FileHeader
            | Self::ConstructorBaseApi
            | Self::ApiMethodGet
            | Self::ApiMethodPostPutPatch
            | Self::ApiMethodDelete
            | Self::ApiMethodConvenience
            | Self::DefaultMethod
            | Self::BuildUrlPath
            | Self::BuildQueryParams
            | Self::BuildHeaders
            | Self::BuildRequestBody
            | Self::MakeRequest => FileCategory::None,
        }
    }
}

/// Template file path mapping using type-safe enum
/// Organized by FileCategory for easier tracking
pub const TEMPLATE_PATHS: &[TemplateName] = &[
    // FileCategory::Readme
    TemplateName::Readme,
    TemplateName::FileHeader,
    // FileCategory::Apis
    TemplateName::ApiClass,
    // FileCategory::Models
    TemplateName::Model,
    TemplateName::ModelHelpers,
    // FileCategory::Runtime
    TemplateName::Runtime,
    // FileCategory::None (Snippets/Partials)
    TemplateName::ConstructorBaseApi,
    TemplateName::ApiMethodGet,
    TemplateName::ApiMethodPostPutPatch,
    TemplateName::ApiMethodDelete,
    TemplateName::ApiMethodConvenience,
    TemplateName::DefaultMethod,
    TemplateName::BuildUrlPath,
    TemplateName::BuildQueryParams,
    TemplateName::BuildHeaders,
    TemplateName::BuildRequestBody,
    TemplateName::MakeRequest,
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

        let header_obj = context! {
            title => title.map(|s| s.to_string()),
            description => description.map(|s| s.to_string()),
            version => version.map(|s| s.to_string()),
        };
        let template_data = context! {
            class => class,
            imports => class.imports.clone(),
            api_interface => api_interface,
            header => header_obj,
            title => title.map(|s| s.to_string()),
            description => description.map(|s| s.to_string()),
            version => version.map(|s| s.to_string()),
        };

        // Get the API class template and render directly
        let template = self
            .env
            .get_template(TemplateName::ApiClass.file_path())
            .map_err(|e| EmitError::TemplateError {
                message: format!(
                    "Failed to get {} template: {}",
                    TemplateName::ApiClass.file_path(),
                    e
                ),
            })?;

        template
            .render(template_data)
            .map_err(|e| EmitError::TemplateError {
                message: format!("Failed to render template: {}", e),
            })
    }

    /// Emit model helper functions (instanceOf/FromJSON/ToJSON/validation map)
    pub fn emit_model_helpers(&self, data: &serde_json::Value) -> Result<String, EmitError> {
        let template = self
            .env
            .get_template(TemplateName::ModelHelpers.file_path())
            .map_err(|e| EmitError::TemplateError {
                message: format!(
                    "Failed to get {} template: {}",
                    TemplateName::ModelHelpers.file_path(),
                    e
                ),
            })?;

        template.render(data).map_err(|e| EmitError::TemplateError {
            message: format!("Failed to render model helpers template: {}", e),
        })
    }

    /// Emit TypeScript code from a type definition
    pub fn emit_model(
        &self,
        type_def: &TsTypeDefinition,
        header: &HeaderData,
    ) -> Result<String, EmitError> {
        let type_def = type_def.clone();

        let template_data = context! {
            type_definition => type_def,
            header,
        };

        // Get the model template and render directly
        let template = self
            .env
            .get_template(TemplateName::Model.file_path())
            .map_err(|e| EmitError::TemplateError {
                message: format!(
                    "Failed to get {} template: {}",
                    TemplateName::Model.file_path(),
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
