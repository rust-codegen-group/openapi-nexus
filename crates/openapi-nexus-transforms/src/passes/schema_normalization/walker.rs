//! Recursive spec traversal and inline schema promotion.
//!
//! Single-pass bottom-up DFS: recurse into children first, then promote the
//! current node if [`should_promote`] returns true. Uses [`std::mem::take`]
//! to satisfy the borrow checker when extracting inline schemas in-place.

use std::collections::BTreeMap;

use openapi_nexus_spec::oas31::spec::{
    Header, MediaType, ObjectOrReference, ObjectSchema, Operation, Parameter, PathItem,
    RequestBody, Response, Schema,
};

use super::naming::{FieldContext, SchemaNameGenerator};
use super::predicate::should_promote;
use crate::passes::TransformError;

/// Configuration for the normalization pass.
pub struct NormalizationConfig {
    pub normalize_objects: bool,
    pub normalize_arrays: bool,
}

/// Accumulated state during a single normalization run.
struct NormalizationState {
    namer: SchemaNameGenerator,
    /// Schemas promoted during traversal: (name, schema).
    promoted: Vec<(String, ObjectSchema)>,
    config: NormalizationConfig,
}

/// Entry point: normalize the entire OpenAPI spec in place.
pub fn normalize_spec(
    spec: &mut openapi_nexus_spec::oas31::spec::OpenApiV31Spec,
    config: NormalizationConfig,
) -> Result<(), TransformError> {
    let existing_names: Vec<String> = spec
        .components
        .as_ref()
        .map(|c| c.schemas.keys().cloned().collect())
        .unwrap_or_default();

    let mut state = NormalizationState {
        namer: SchemaNameGenerator::new(existing_names),
        promoted: Vec::new(),
        config,
    };

    // 1. Normalize component schemas
    if let Some(components) = spec.components.as_mut() {
        let mut schemas = std::mem::take(&mut components.schemas);
        for (name, schema_ref) in schemas.iter_mut() {
            if let ObjectOrReference::Object(obj) = schema_ref {
                normalize_object_schema_children(obj, name, &mut state);
            }
        }
        components.schemas = schemas;

        // 2. Normalize component request bodies
        let mut request_bodies = std::mem::take(&mut components.request_bodies);
        for (name, rb_ref) in request_bodies.iter_mut() {
            if let ObjectOrReference::Object(rb) = rb_ref {
                normalize_request_body_content(rb, name, &mut state);
            }
        }
        components.request_bodies = request_bodies;

        // 3. Normalize component responses
        let mut responses = std::mem::take(&mut components.responses);
        for (name, resp_ref) in responses.iter_mut() {
            if let ObjectOrReference::Object(resp) = resp_ref {
                normalize_response_content(resp, name, &mut state);
            }
        }
        components.responses = responses;

        // 4. Normalize component parameters
        let mut parameters = std::mem::take(&mut components.parameters);
        for (name, param_ref) in parameters.iter_mut() {
            if let ObjectOrReference::Object(param) = param_ref {
                normalize_parameter_schema(param, name, &mut state);
            }
        }
        components.parameters = parameters;

        // 5. Normalize component headers
        let mut headers = std::mem::take(&mut components.headers);
        for (name, header_ref) in headers.iter_mut() {
            if let ObjectOrReference::Object(header) = header_ref {
                normalize_header_schema(header, name, &mut state);
            }
        }
        components.headers = headers;
    }

    // 6. Normalize paths
    if let Some(paths) = spec.paths.as_mut() {
        for (path, path_item) in paths.iter_mut() {
            normalize_path_item(path_item, path, &mut state);
        }
    }

    // 7. Normalize webhooks
    for (name, path_item) in spec.webhooks.iter_mut() {
        normalize_path_item(path_item, name, &mut state);
    }

    // 8. Insert all promoted schemas into components
    if !state.promoted.is_empty() {
        let components = spec.components.get_or_insert_with(Default::default);
        for (name, schema) in state.promoted {
            components
                .schemas
                .insert(name, ObjectOrReference::Object(schema));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Core promotion logic
// ---------------------------------------------------------------------------

/// Normalize a single `ObjectOrReference<ObjectSchema>` in place.
/// If it is an inline `Object` that should be promoted, extract it via `std::mem::take`
/// and replace with a `$ref`.
fn normalize_object_or_ref(
    oor: &mut ObjectOrReference<ObjectSchema>,
    parent_name: &str,
    ctx: FieldContext,
    state: &mut NormalizationState,
) {
    let ObjectOrReference::Object(obj) = oor else {
        return;
    };

    // Compute candidate name for this node (used as parent_name for recursive children)
    let candidate_name = state.namer.peek_name(parent_name, &ctx);

    // Bottom-up: normalize children first
    normalize_object_schema_children(obj, &candidate_name, state);

    // Check promotion eligibility
    if !should_promote_with_config(obj, &state.config) {
        return;
    }

    let promoted_name = state.namer.generate_unique(&candidate_name);
    let taken = std::mem::take(obj);
    state.promoted.push((promoted_name.clone(), taken));
    *oor = ObjectOrReference::Ref {
        ref_path: format!("#/components/schemas/{promoted_name}"),
        summary: None,
        description: None,
    };
}

/// Checks `should_promote` gated by config flags.
fn should_promote_with_config(schema: &ObjectSchema, config: &NormalizationConfig) -> bool {
    if !config.normalize_objects && !config.normalize_arrays {
        return false;
    }
    if !config.normalize_objects && !schema.properties.is_empty() {
        // Properties-only promotion disabled
        return false;
    }
    should_promote(schema)
}

// ---------------------------------------------------------------------------
// ObjectSchema child traversal
// ---------------------------------------------------------------------------

/// Walk all child locations of an `ObjectSchema` that may contain inline schemas.
fn normalize_object_schema_children(
    obj: &mut ObjectSchema,
    parent_name: &str,
    state: &mut NormalizationState,
) {
    // Properties
    for (field_name, prop_ref) in obj.properties.iter_mut() {
        normalize_object_or_ref(
            prop_ref,
            parent_name,
            FieldContext::Property(field_name.clone()),
            state,
        );
    }

    // allOf
    for (i, member) in obj.all_of.iter_mut().enumerate() {
        normalize_object_or_ref(member, parent_name, FieldContext::Variant(i), state);
    }

    // anyOf
    for (i, member) in obj.any_of.iter_mut().enumerate() {
        normalize_object_or_ref(member, parent_name, FieldContext::Variant(i), state);
    }

    // oneOf
    for (i, member) in obj.one_of.iter_mut().enumerate() {
        normalize_object_or_ref(member, parent_name, FieldContext::Variant(i), state);
    }

    // prefixItems
    for (i, item) in obj.prefix_items.iter_mut().enumerate() {
        normalize_object_or_ref(item, parent_name, FieldContext::PrefixItem(i), state);
    }

    // items
    if let Some(items) = obj.items.as_mut() {
        normalize_schema(items, parent_name, FieldContext::ArrayItem, state);
    }

    // additionalProperties
    if let Some(ap) = obj.additional_properties.as_mut() {
        normalize_schema(ap, parent_name, FieldContext::AdditionalProperties, state);
    }
}

/// Normalize a `Schema` value (wraps `ObjectOrReference<ObjectSchema>` inside `Schema::Object`).
fn normalize_schema(
    schema: &mut Schema,
    parent_name: &str,
    ctx: FieldContext,
    state: &mut NormalizationState,
) {
    if let Schema::Object(boxed) = schema {
        normalize_object_or_ref(boxed.as_mut(), parent_name, ctx, state);
    }
}

// ---------------------------------------------------------------------------
// Path / Operation traversal
// ---------------------------------------------------------------------------

fn normalize_path_item(item: &mut PathItem, path: &str, state: &mut NormalizationState) {
    macro_rules! normalize_method {
        ($field:ident, $method_str:expr) => {
            if let Some(ref mut op) = item.$field {
                normalize_operation(op, path, $method_str, state);
            }
        };
    }

    normalize_method!(get, "GET");
    normalize_method!(put, "PUT");
    normalize_method!(post, "POST");
    normalize_method!(delete, "DELETE");
    normalize_method!(options, "OPTIONS");
    normalize_method!(head, "HEAD");
    normalize_method!(patch, "PATCH");
    normalize_method!(trace, "TRACE");
}

fn normalize_operation(
    op: &mut Operation,
    path: &str,
    method: &str,
    state: &mut NormalizationState,
) {
    let op_id = op.operation_id.clone();

    // Request body
    if let Some(ObjectOrReference::Object(rb)) = op.request_body.as_mut() {
        let parent_name = op_id
            .as_deref()
            .map(heck::ToPascalCase::to_pascal_case)
            .unwrap_or_else(|| {
                format!(
                    "{}{}",
                    heck::ToPascalCase::to_pascal_case(method),
                    path.split('/')
                        .filter(|s| !s.is_empty())
                        .map(|s| heck::ToPascalCase::to_pascal_case(
                            s.trim_start_matches('{').trim_end_matches('}')
                        ))
                        .collect::<String>()
                )
            });

        normalize_media_type_map(
            &mut rb.content,
            &parent_name,
            FieldContext::RequestBody {
                op_id: op_id.clone(),
                method: method.to_string(),
                path: path.to_string(),
            },
            state,
        );
    }

    // Responses
    if let Some(responses) = op.responses.as_mut() {
        for (status, resp_ref) in responses.iter_mut() {
            if let ObjectOrReference::Object(resp) = resp_ref {
                let parent_name = op_id
                    .as_deref()
                    .map(heck::ToPascalCase::to_pascal_case)
                    .unwrap_or_else(|| {
                        format!(
                            "{}{}",
                            heck::ToPascalCase::to_pascal_case(method),
                            path.split('/')
                                .filter(|s| !s.is_empty())
                                .map(|s| heck::ToPascalCase::to_pascal_case(
                                    s.trim_start_matches('{').trim_end_matches('}')
                                ))
                                .collect::<String>()
                        )
                    });

                normalize_media_type_map(
                    &mut resp.content,
                    &parent_name,
                    FieldContext::ResponseBody {
                        op_id: op_id.clone(),
                        method: method.to_string(),
                        path: path.to_string(),
                        status: status.clone(),
                    },
                    state,
                );
            }
        }
    }

    // Parameters
    for param_ref in op.parameters.iter_mut() {
        if let ObjectOrReference::Object(param) = param_ref {
            normalize_parameter_schema(param, &param.name.clone(), state);
        }
    }
}

// ---------------------------------------------------------------------------
// Media type / parameter / header helpers
// ---------------------------------------------------------------------------

fn normalize_media_type_map(
    content: &mut BTreeMap<String, MediaType>,
    parent_name: &str,
    ctx: FieldContext,
    state: &mut NormalizationState,
) {
    for (_mime, media_type) in content.iter_mut() {
        if let Some(ref mut schema_ref) = media_type.schema {
            normalize_object_or_ref(schema_ref, parent_name, ctx.clone(), state);
        }
    }
}

fn normalize_request_body_content(
    rb: &mut RequestBody,
    name: &str,
    state: &mut NormalizationState,
) {
    normalize_media_type_map(
        &mut rb.content,
        name,
        FieldContext::RequestBody {
            op_id: Some(name.to_string()),
            method: String::new(),
            path: String::new(),
        },
        state,
    );
}

fn normalize_response_content(resp: &mut Response, name: &str, state: &mut NormalizationState) {
    normalize_media_type_map(
        &mut resp.content,
        name,
        FieldContext::ResponseBody {
            op_id: Some(name.to_string()),
            method: String::new(),
            path: String::new(),
            status: String::new(),
        },
        state,
    );
}

fn normalize_parameter_schema(
    param: &mut Parameter,
    parent_name: &str,
    state: &mut NormalizationState,
) {
    if let Some(ref mut schema_ref) = param.schema {
        normalize_object_or_ref(
            schema_ref,
            parent_name,
            FieldContext::ParameterSchema {
                param_name: param.name.clone(),
            },
            state,
        );
    }
}

fn normalize_header_schema(header: &mut Header, parent_name: &str, state: &mut NormalizationState) {
    if let Some(ref mut schema_ref) = header.schema {
        normalize_object_or_ref(
            schema_ref,
            parent_name,
            FieldContext::Property(parent_name.to_string()),
            state,
        );
    }
}
