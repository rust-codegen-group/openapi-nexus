//! Template path information for file generation

/// Information about a template and its output location
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplatePathInfo {
    /// Path to the template file (e.g., "api/api_class.j2")
    pub template_path: String,
    /// Relative output path for the generated file (e.g., "apis/UserApi.ts")
    pub output_relpath: String,
}

impl TemplatePathInfo {
    /// Create a new template path info
    pub fn new(template_path: impl Into<String>, output_relpath: impl Into<String>) -> Self {
        Self {
            template_path: template_path.into(),
            output_relpath: output_relpath.into(),
        }
    }
}
