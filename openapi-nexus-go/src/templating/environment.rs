//! Template environment setup and configuration

use minijinja::Environment;

use super::filters::fmt_filter;

/// Create a new template environment with all filters and functions
/// Each language generator instance has its own environment
pub fn create_template_environment() -> Environment<'static> {
    let mut env = Environment::new();
    env.set_trim_blocks(true);
    env.set_lstrip_blocks(true);

    // Load all embedded templates
    minijinja_embed::load_templates!(&mut env);

    // Format filters
    env.add_filter("fmt", fmt_filter);

    env
}
