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

            // FileCategory::Runtime
            Self::Runtime => FileCategory::Runtime,

            // FileCategory::ProjectFiles
            Self::ProjectIndex => FileCategory::ProjectFiles,

            // FileCategory::None (Snippets/Partials)
            // These are included by other templates and not rendered directly
            Self::CommonFileHeader => FileCategory::None,
        }
    }
}

/// Template file path mapping using type-safe enum
/// Organized by FileCategory for easier tracking
pub const TEMPLATE_PATHS: &[TemplateName] = &[
    // FileCategory::Readme
    TemplateName::Readme,
    // FileCategory::Runtime
    TemplateName::Runtime,
    // FileCategory::ProjectFiles
    TemplateName::ProjectIndex,
    // FileCategory::None (Snippets/Partials)
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
