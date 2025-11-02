//! Template environment setup and configuration

use minijinja::Environment;

use super::filters::{
    create_format_class_signature_filter, create_format_doc_comment_filter,
    create_format_generic_list_filter, create_format_import_filter,
    create_format_interface_signature_filter, create_format_method_signature_filter,
    create_format_method_signature_iface_filter, create_format_ts_class_property_filter,
    create_format_ts_property_filter, create_format_type_definition_filter,
    create_format_type_expr_filter, from_json_line_filter, instance_guard_filter,
    to_json_line_filter,
};
use super::functions::file_header;
use crate::config::MAX_LINE_WIDTH;

/// Helper macro to register multiple max_line_width-dependent filters in one
/// shot to avoid repetition.
macro_rules! add_mlw_filters {
    ($env:expr, $max:expr, { $( $name:expr => $factory:path ),+ $(,)? }) => {
        $( $env.add_filter($name, $factory($max)); )+
    };
}

/// Create a new template environment with all filters and functions
/// Each language generator instance has its own environment
pub fn create_template_environment() -> Environment<'static> {
    let mut env = Environment::new();
    env.set_trim_blocks(true);
    env.set_lstrip_blocks(true);

    // Load all embedded templates
    minijinja_embed::load_templates!(&mut env);

    // Model helpers filters
    env.add_filter("instance_guard", instance_guard_filter);
    env.add_filter("from_json_line", from_json_line_filter);
    env.add_filter("to_json_line", to_json_line_filter);

    // Add filters that need max_line_width
    add_mlw_filters!(env, MAX_LINE_WIDTH, {
        "format_class_signature" => create_format_class_signature_filter,
        "format_doc_comment" => create_format_doc_comment_filter,
        "format_generic_list" => create_format_generic_list_filter,
        "format_import" => create_format_import_filter,
        "format_interface_signature" => create_format_interface_signature_filter,
        "format_method_signature" => create_format_method_signature_filter,
        "format_method_signature_iface" => create_format_method_signature_iface_filter,
        "format_ts_class_property" => create_format_ts_class_property_filter,
        "format_ts_property" => create_format_ts_property_filter,
        "format_type_definition" => create_format_type_definition_filter,
        "format_type_expr" => create_format_type_expr_filter,
    });

    // Add custom functions
    env.add_function("file_header", file_header);

    env
}
