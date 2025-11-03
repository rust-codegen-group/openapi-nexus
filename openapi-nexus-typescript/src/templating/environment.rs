//! Template environment setup and configuration

use minijinja::Environment;

use super::filters::{
    format_class_signature_filter, format_doc_comment_filter, format_import_filter,
    format_interface_signature_filter, format_method_signature_filter, format_rc_doc_filter,
    format_ts_class_property_filter, format_ts_expression_filter, format_ts_property_filter,
    format_type_definition_filter,
};
use super::functions::file_header;

/// Create a new template environment with all filters and functions
/// Each language generator instance has its own environment
pub fn create_template_environment() -> Environment<'static> {
    let mut env = Environment::new();
    env.set_trim_blocks(true);
    env.set_lstrip_blocks(true);

    // Load all embedded templates
    minijinja_embed::load_templates!(&mut env);

    // Format filters
    env.add_filter("format_class_signature", format_class_signature_filter);
    env.add_filter("format_doc_comment", format_doc_comment_filter);
    env.add_filter("format_import", format_import_filter);
    env.add_filter(
        "format_interface_signature",
        format_interface_signature_filter,
    );
    env.add_filter("format_method_signature", format_method_signature_filter);
    env.add_filter("format_rc_doc", format_rc_doc_filter);
    env.add_filter("format_ts_class_property", format_ts_class_property_filter);
    env.add_filter("format_ts_expression", format_ts_expression_filter);
    env.add_filter("format_ts_property", format_ts_property_filter);
    env.add_filter("format_type_definition", format_type_definition_filter);

    // Add custom functions
    env.add_function("file_header", file_header);

    env
}
