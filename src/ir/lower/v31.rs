//! OpenAPI v3.1 → IR lowering.

use heck::ToPascalCase;
use indexmap::IndexMap;

use crate::spec::oas31::spec::{
    self as oas, ObjectOrReference, ObjectSchema, Schema, SchemaType, SchemaTypeSet,
};

use super::LowerError;
use crate::ir::types::{
    ApiKeyLocation, IrContact, IrEnum, IrEnumValue, IrEnumValueType, IrHeader, IrInfo,
    IrIntersection, IrLicense, IrOAuth2Flow, IrOAuth2Flows, IrObject, IrOperation, IrParameter,
    IrPrimitive, IrProperty, IrRequestBody, IrResponse, IrSchema, IrSchemaKind,
    IrSecurityRequirement, IrSecurityScheme, IrServer, IrSpec, IrTaggedUnion, IrTaggedVariant,
    IrTypeExpr, IrUnion, IrValidation, ParameterLocation, TaggingStyle,
};

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn lower_v31(spec: &oas::OpenApiV31Spec) -> Result<IrSpec, LowerError> {
    let mut ctx = LowerCtx::new(spec);

    // 1. Lower component schemas
    ctx.in_component_phase = true;
    if let Some(components) = &spec.components {
        for (name, schema_ref) in &components.schemas {
            let mut schema = ctx.lower_named_schema(name, schema_ref)?;
            schema.is_component = true;
            ctx.schemas.insert(name.clone(), schema);
        }
    }
    ctx.in_component_phase = false;

    // 2. Lower operations (and discover inline schemas)
    let mut operations = Vec::new();
    if let Some(paths) = &spec.paths {
        for (path, path_item) in paths {
            let ops = ctx.lower_path_item(path, path_item)?;
            operations.extend(ops);
        }
    }

    // 3. Lower webhooks
    for (name, path_item) in &spec.webhooks {
        let ops = ctx.lower_path_item(name, path_item)?;
        operations.extend(ops);
    }

    // 4. Lower security schemes
    let mut security_schemes = IndexMap::new();
    if let Some(components) = &spec.components {
        for (name, scheme_ref) in &components.security_schemes {
            if let ObjectOrReference::Object(scheme) = scheme_ref {
                security_schemes.insert(name.clone(), lower_security_scheme(scheme));
            }
        }
    }

    // 5. Lower top-level security
    let security = spec
        .security
        .iter()
        .flat_map(|req| {
            req.0.iter().map(|(name, scopes)| IrSecurityRequirement {
                scheme_name: name.clone(),
                scopes: scopes.clone(),
            })
        })
        .collect();

    Ok(IrSpec {
        info: lower_info(&spec.info),
        servers: spec
            .servers
            .iter()
            .map(|s| IrServer {
                url: s.url.clone(),
                description: s.description.clone(),
            })
            .collect(),
        schemas: ctx.schemas,
        operations,
        security_schemes,
        security,
    })
}

/// Extract the path prefix from the first server URL, used to strip
/// duplicated segments from operation paths.
fn server_path_prefix(servers: &[crate::spec::oas31::spec::Server]) -> String {
    servers
        .first()
        .map(|s| {
            let u = &s.url;
            if let Some(proto_end) = u.find("://") {
                let after_proto = &u[proto_end + 3..];
                if let Some(path_start) = after_proto.find('/') {
                    after_proto[path_start..].trim_end_matches('/').to_string()
                } else {
                    String::new()
                }
            } else if u.starts_with('/') {
                u.trim_end_matches('/').to_string()
            } else if !u.is_empty() && !u.starts_with("http") {
                // Relative URL (e.g., "v2", "api/v2"): treat as path prefix
                let with_slash = if u.starts_with('/') {
                    u.to_string()
                } else {
                    format!("/{u}")
                };
                with_slash.trim_end_matches('/').to_string()
            } else {
                String::new()
            }
        })
        .unwrap_or_default()
}

fn strip_server_path_prefix(path: &str, servers: &[crate::spec::oas31::spec::Server]) -> String {
    let prefix = server_path_prefix(servers);
    if !prefix.is_empty()
        && path.starts_with(&prefix)
        && (path.len() == prefix.len() || path.as_bytes()[prefix.len()] == b'/')
    {
        let stripped = &path[prefix.len()..];
        if stripped.is_empty() { "/" } else { stripped }.to_string()
    } else {
        path.to_string()
    }
}

// ---------------------------------------------------------------------------
// Lowering context
// ---------------------------------------------------------------------------

struct LowerCtx<'a> {
    spec: &'a oas::OpenApiV31Spec,
    schemas: IndexMap<String, IrSchema>,
    used_names: std::collections::HashSet<String>,
    /// When true, promoted schemas are marked as component schemas.
    in_component_phase: bool,
}

impl<'a> LowerCtx<'a> {
    fn new(spec: &'a oas::OpenApiV31Spec) -> Self {
        let mut used_names = std::collections::HashSet::new();
        if let Some(components) = &spec.components {
            for name in components.schemas.keys() {
                used_names.insert(name.clone());
            }
        }
        LowerCtx {
            spec,
            schemas: IndexMap::new(),
            used_names,
            in_component_phase: false,
        }
    }

    fn generate_unique_name(&mut self, base: &str) -> String {
        if self.used_names.insert(base.to_string()) {
            return base.to_string();
        }
        let mut suffix = 2;
        loop {
            let candidate = format!("{base}{suffix}");
            if self.used_names.insert(candidate.clone()) {
                return candidate;
            }
            suffix += 1;
        }
    }

    // -------------------------------------------------------------------
    // Schema lowering
    // -------------------------------------------------------------------

    fn lower_named_schema(
        &mut self,
        name: &str,
        schema_ref: &ObjectOrReference<ObjectSchema>,
    ) -> Result<IrSchema, LowerError> {
        match schema_ref {
            ObjectOrReference::Ref { ref_path, .. } => Ok(IrSchema {
                name: name.to_string(),
                description: None,
                deprecated: false,
                kind: IrSchemaKind::Alias(IrTypeExpr::Named(extract_schema_name(ref_path))),
                is_component: false,
            }),
            ObjectOrReference::Object(obj) => self.lower_object_schema_to_ir_schema(name, obj),
        }
    }

    fn lower_object_schema_to_ir_schema(
        &mut self,
        name: &str,
        obj: &ObjectSchema,
    ) -> Result<IrSchema, LowerError> {
        let kind = self.classify_schema(name, obj)?;
        Ok(IrSchema {
            name: name.to_string(),
            description: obj.description.clone(),
            deprecated: obj.deprecated.unwrap_or(false),
            kind,
            is_component: false,
        })
    }

    fn classify_schema(
        &mut self,
        name: &str,
        obj: &ObjectSchema,
    ) -> Result<IrSchemaKind, LowerError> {
        // Check for enum first
        if !obj.enum_values.is_empty() {
            return Ok(IrSchemaKind::Enum(self.lower_enum(obj)));
        }

        // Check for discriminated union (oneOf with discriminator or tagged enum pattern)
        if !obj.one_of.is_empty() {
            if let Some(tagged) = self.try_lower_tagged_union(name, obj)? {
                return Ok(IrSchemaKind::TaggedUnion(tagged));
            }
            // Untagged oneOf: filter null members, track nullability
            let has_null = obj.one_of.iter().any(is_null_schema);
            let non_null: Vec<_> = obj.one_of.iter().filter(|m| !is_null_schema(m)).collect();
            let members = non_null
                .iter()
                .enumerate()
                .map(|(idx, m)| {
                    let candidate = format!("{}Member{}", name, idx + 1);
                    self.lower_schema_ref_with_promotion(&candidate, m)
                })
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(IrSchemaKind::Union(IrUnion {
                members,
                nullable: has_null,
            }));
        }

        // anyOf without oneOf → union
        if !obj.any_of.is_empty() {
            let has_null = obj.any_of.iter().any(is_null_schema);
            let non_null: Vec<_> = obj.any_of.iter().filter(|m| !is_null_schema(m)).collect();
            let members = non_null
                .iter()
                .enumerate()
                .map(|(idx, m)| {
                    let candidate = format!("{}Member{}", name, idx + 1);
                    self.lower_schema_ref_with_promotion(&candidate, m)
                })
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(IrSchemaKind::Union(IrUnion {
                members,
                nullable: has_null,
            }));
        }

        // allOf → intersection
        if !obj.all_of.is_empty() {
            let members = obj
                .all_of
                .iter()
                .enumerate()
                .map(|(idx, m)| {
                    let candidate = format!("{}AllOf{}", name, idx + 1);
                    self.lower_schema_ref_with_promotion(&candidate, m)
                })
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(IrSchemaKind::Intersection(IrIntersection { members }));
        }

        // Object with properties
        if !obj.properties.is_empty() || obj.additional_properties.is_some() {
            return Ok(IrSchemaKind::Object(self.lower_object(name, obj)?));
        }

        // Array → alias
        if let Some(items) = &obj.items {
            let items_candidate = format!("{name}Items");
            let inner = self.lower_schema_value_with_promotion(&items_candidate, items)?;
            return Ok(IrSchemaKind::Alias(IrTypeExpr::Array(Box::new(inner))));
        }

        // Simple type → alias
        Ok(IrSchemaKind::Alias(self.lower_type_from_schema(obj)))
    }

    fn lower_enum(&self, obj: &ObjectSchema) -> IrEnum {
        let value_type = classify_enum_values(&obj.enum_values);
        let values = obj
            .enum_values
            .iter()
            .map(|v| IrEnumValue {
                value: v.clone(),
                description: None,
            })
            .collect();
        IrEnum { value_type, values }
    }

    fn try_lower_tagged_union(
        &mut self,
        parent_name: &str,
        obj: &ObjectSchema,
    ) -> Result<Option<IrTaggedUnion>, LowerError> {
        // First check for explicit discriminator
        if let Some(disc) = &obj.discriminator {
            return self.lower_discriminated_union(parent_name, obj, disc);
        }

        // Then check tagged enum patterns
        use crate::ir::tagged_enum_pattern::TaggedEnumPattern;
        let patterns: Vec<Option<TaggedEnumPattern>> = obj
            .one_of
            .iter()
            .map(TaggedEnumPattern::detect_from_schema)
            .collect();

        // All must match for it to be a tagged union
        let all_tagged = patterns.iter().all(|p| p.is_some());
        if !all_tagged || patterns.is_empty() {
            return Ok(None);
        }

        // Check if all patterns are externally tagged (no common tag field needed)
        let all_external = patterns
            .iter()
            .all(|p| matches!(p, Some(TaggedEnumPattern::ExternallyTagged { .. })));

        if all_external {
            // Externally tagged: no tag field, discrimination is by property name
            let mut variants = Vec::new();
            for (member, pattern) in obj.one_of.iter().zip(patterns.iter()) {
                let pattern = pattern.as_ref().unwrap();
                let (disc_value, content_type) =
                    self.extract_tagged_variant(parent_name, member, pattern)?;
                let description = match member {
                    ObjectOrReference::Object(obj) => obj.description.clone(),
                    _ => None,
                };
                variants.push(IrTaggedVariant {
                    discriminator_value: disc_value,
                    content_type,
                    description,
                });
            }
            return Ok(Some(IrTaggedUnion {
                discriminator_field: String::new(), // No common tag field for external
                tagging: TaggingStyle::External,
                variants,
            }));
        }

        // Find the common tag field (for Internal/Adjacent styles)
        let tag_fields: Vec<&str> = patterns
            .iter()
            .filter_map(|p| p.as_ref()?.tag_field())
            .collect();

        if tag_fields.is_empty() {
            return Ok(None);
        }

        let first_tag = tag_fields[0];
        let all_same_tag = tag_fields.iter().all(|t| *t == first_tag);
        if !all_same_tag {
            return Ok(None);
        }

        // Build variants
        // For variants detected as ExternallyTagged in a mixed union that has a
        // common tag field, re-interpret them as InternallyTagged unit variants.
        let mut variants = Vec::new();
        for (member, pattern) in obj.one_of.iter().zip(patterns.iter()) {
            let pattern = pattern.as_ref().unwrap();
            let effective_pattern = match pattern {
                TaggedEnumPattern::ExternallyTagged { .. } => {
                    &TaggedEnumPattern::InternallyTagged {
                        variant_name: pattern.variant_name().to_string(),
                        tag_field: first_tag.to_string(),
                    }
                }
                other => other,
            };
            let (disc_value, content_type) =
                self.extract_tagged_variant(parent_name, member, effective_pattern)?;
            let description = match member {
                ObjectOrReference::Object(obj) => obj.description.clone(),
                _ => None,
            };
            variants.push(IrTaggedVariant {
                discriminator_value: disc_value,
                content_type,
                description,
            });
        }

        Ok(Some(IrTaggedUnion {
            discriminator_field: first_tag.to_string(),
            tagging: classify_tagging_style(&patterns),
            variants,
        }))
    }

    fn lower_discriminated_union(
        &mut self,
        parent_name: &str,
        obj: &ObjectSchema,
        disc: &oas::Discriminator,
    ) -> Result<Option<IrTaggedUnion>, LowerError> {
        let mut variants = Vec::new();

        if let Some(mapping) = &disc.mapping {
            // Explicit mapping: value -> $ref
            for (value, ref_path) in mapping {
                let schema_name = extract_schema_name(ref_path);
                variants.push(IrTaggedVariant {
                    discriminator_value: value.clone(),
                    content_type: IrTypeExpr::Named(schema_name),
                    description: None,
                });
            }
        } else {
            // Infer from oneOf members
            for member in &obj.one_of {
                match member {
                    ObjectOrReference::Ref { ref_path, .. } => {
                        let schema_name = extract_schema_name(ref_path);
                        variants.push(IrTaggedVariant {
                            discriminator_value: schema_name.clone(),
                            content_type: IrTypeExpr::Named(schema_name),
                            description: None,
                        });
                    }
                    ObjectOrReference::Object(inline_obj) => {
                        let variant_name =
                            self.generate_unique_name(&format!("{parent_name}Variant"));
                        let schema =
                            self.lower_object_schema_to_ir_schema(&variant_name, inline_obj)?;
                        let disc_value = variant_name.clone();
                        self.schemas.insert(variant_name.clone(), schema);
                        variants.push(IrTaggedVariant {
                            discriminator_value: disc_value,
                            content_type: IrTypeExpr::Named(variant_name),
                            description: None,
                        });
                    }
                }
            }
        }

        Ok(Some(IrTaggedUnion {
            discriminator_field: disc.property_name.clone(),
            tagging: TaggingStyle::Internal,
            variants,
        }))
    }

    fn extract_tagged_variant(
        &mut self,
        parent_name: &str,
        member: &ObjectOrReference<ObjectSchema>,
        pattern: &crate::ir::tagged_enum_pattern::TaggedEnumPattern,
    ) -> Result<(String, IrTypeExpr), LowerError> {
        use crate::ir::tagged_enum_pattern::TaggedEnumPattern;

        match pattern {
            TaggedEnumPattern::InternallyTagged { tag_field, .. } => {
                if let ObjectOrReference::Object(obj) = member {
                    if !obj.all_of.is_empty() {
                        // allOf with [ref, tag_object]
                        // The ref is the content type, the tag_object contains the discriminator value
                        let mut content_ref = None;
                        let mut disc_value = None;

                        for item in &obj.all_of {
                            match item {
                                ObjectOrReference::Ref { ref_path, .. } => {
                                    content_ref = Some(extract_schema_name(ref_path));
                                }
                                ObjectOrReference::Object(tag_obj) => {
                                    // Extract discriminator value from the tag object
                                    if let Some(ObjectOrReference::Object(prop_schema)) =
                                        tag_obj.properties.get(tag_field.as_str())
                                        && let Some(first_enum) = prop_schema.enum_values.first()
                                    {
                                        disc_value = first_enum.as_str().map(String::from);
                                    }
                                }
                            }
                        }

                        let content_type = content_ref
                            .map(IrTypeExpr::Named)
                            .unwrap_or(IrTypeExpr::Any);
                        let disc_value =
                            disc_value.unwrap_or_else(|| pattern.variant_name().to_string());
                        Ok((disc_value, content_type))
                    } else {
                        // Plain object with tag property embedded
                        // Extract discriminator value from the tag property
                        let disc_value = obj
                            .properties
                            .get(tag_field.as_str())
                            .and_then(|prop_ref| match prop_ref {
                                ObjectOrReference::Object(prop_obj) => prop_obj
                                    .enum_values
                                    .first()
                                    .and_then(|v| v.as_str().map(String::from)),
                                _ => None,
                            })
                            .unwrap_or_else(|| pattern.variant_name().to_string());

                        // The content type is this object itself, promoted to a named schema
                        // (excluding the tag field since it's in the discriminator)
                        let variant_name = self.generate_unique_name(&format!(
                            "{}{}",
                            parent_name,
                            disc_value.to_pascal_case()
                        ));
                        let mut variant_schema =
                            self.lower_object_schema_to_ir_schema(&variant_name, obj)?;
                        variant_schema.is_component = self.in_component_phase;
                        self.schemas.insert(variant_name.clone(), variant_schema);
                        Ok((disc_value, IrTypeExpr::Named(variant_name)))
                    }
                } else {
                    Ok((
                        pattern.variant_name().to_string(),
                        self.lower_schema_ref(member)?,
                    ))
                }
            }

            TaggedEnumPattern::ExternallyTagged { .. } => {
                // Single required property: property name is the variant key,
                // property value is the content type
                if let ObjectOrReference::Object(obj) = member {
                    if let Some((prop_name, prop_schema)) = obj.properties.iter().next() {
                        let content_type = self.lower_schema_ref(prop_schema)?;
                        Ok((prop_name.clone(), content_type))
                    } else {
                        Ok((pattern.variant_name().to_string(), IrTypeExpr::Any))
                    }
                } else {
                    Ok((
                        pattern.variant_name().to_string(),
                        self.lower_schema_ref(member)?,
                    ))
                }
            }

            TaggedEnumPattern::AdjacentlyTagged { content_field, .. } => {
                if let ObjectOrReference::Object(obj) = member {
                    // Extract tag value
                    let tag_field = pattern.tag_field().unwrap_or("");
                    let disc_value = obj
                        .properties
                        .get(tag_field)
                        .and_then(|prop| {
                            if let ObjectOrReference::Object(prop_obj) = prop {
                                prop_obj.enum_values.first()?.as_str().map(String::from)
                            } else {
                                None
                            }
                        })
                        .unwrap_or_else(|| pattern.variant_name().to_string());

                    // Extract content type
                    let content_type = obj
                        .properties
                        .get(content_field.as_str())
                        .map(|prop| self.lower_schema_ref(prop))
                        .transpose()?
                        .unwrap_or(IrTypeExpr::Any);

                    Ok((disc_value, content_type))
                } else {
                    Ok((
                        pattern.variant_name().to_string(),
                        self.lower_schema_ref(member)?,
                    ))
                }
            }

            TaggedEnumPattern::Untagged { .. } => Ok((
                pattern.variant_name().to_string(),
                self.lower_schema_ref(member)?,
            )),
        }
    }

    fn lower_object(
        &mut self,
        parent_name: &str,
        obj: &ObjectSchema,
    ) -> Result<IrObject, LowerError> {
        let required_set: std::collections::HashSet<&str> =
            obj.required.iter().map(|s| s.as_str()).collect();

        let mut properties = IndexMap::new();
        for (field_name, prop_ref) in &obj.properties {
            let type_expr = self.lower_schema_ref_with_promotion(
                &format!("{parent_name}{}", field_name.to_pascal_case()),
                prop_ref,
            )?;
            let (base_type, nullable) = unwrap_nullable(type_expr);
            let (description, format, default_value, validation) = match prop_ref {
                ObjectOrReference::Object(prop_obj) => (
                    prop_obj.description.clone(),
                    prop_obj.format.clone(),
                    prop_obj.default.clone(),
                    extract_validation(prop_obj),
                ),
                ObjectOrReference::Ref { .. } => (None, None, None, None),
            };

            properties.insert(
                field_name.clone(),
                IrProperty {
                    name: field_name.clone(),
                    type_expr: base_type,
                    required: required_set.contains(field_name.as_str()),
                    nullable,
                    description,
                    default_value,
                    format,
                    validation,
                },
            );
        }

        let additional_properties = match &obj.additional_properties {
            Some(Schema::Boolean(b)) => {
                if b.0 {
                    Some(IrTypeExpr::Any)
                } else {
                    None
                }
            }
            Some(Schema::Object(boxed_ref)) => Some(self.lower_schema_ref(boxed_ref)?),
            None => None,
        };

        Ok(IrObject {
            properties,
            additional_properties,
        })
    }

    // -------------------------------------------------------------------
    // Type expression lowering
    // -------------------------------------------------------------------

    fn lower_schema_ref(
        &mut self,
        schema_ref: &ObjectOrReference<ObjectSchema>,
    ) -> Result<IrTypeExpr, LowerError> {
        match schema_ref {
            ObjectOrReference::Ref { ref_path, .. } => {
                if ref_path.starts_with("http://") || ref_path.starts_with("https://") {
                    return Err(LowerError::ExternalReference {
                        reference: ref_path.clone(),
                    });
                }
                Ok(IrTypeExpr::Named(extract_schema_name(ref_path)))
            }
            ObjectOrReference::Object(obj) => Ok(self.lower_type_from_schema(obj)),
        }
    }

    /// Lower a schema ref, potentially promoting complex inline schemas to named types.
    fn lower_schema_ref_with_promotion(
        &mut self,
        candidate_name: &str,
        schema_ref: &ObjectOrReference<ObjectSchema>,
    ) -> Result<IrTypeExpr, LowerError> {
        match schema_ref {
            ObjectOrReference::Ref { ref_path, .. } => {
                Ok(IrTypeExpr::Named(extract_schema_name(ref_path)))
            }
            ObjectOrReference::Object(obj) => {
                // Check for nullable wrapper pattern: oneOf/anyOf with [null, real_type]
                if let Some(inner) = self.try_lower_nullable_wrapper(candidate_name, obj)? {
                    return Ok(inner);
                }
                if should_promote_to_named(obj) {
                    let name = self.generate_unique_name(candidate_name);
                    let mut schema = self.lower_object_schema_to_ir_schema(&name, obj)?;
                    schema.is_component = self.in_component_phase;
                    self.schemas.insert(name.clone(), schema);
                    Ok(IrTypeExpr::Named(name))
                } else if is_array_with_promotable_items(obj) {
                    // Array with inline object items: promote the items schema
                    let items_name = format!("{candidate_name}Item");
                    let inner = self.lower_array_items_with_promotion(&items_name, obj)?;
                    Ok(IrTypeExpr::Array(Box::new(inner)))
                } else {
                    Ok(self.lower_type_from_schema(obj))
                }
            }
        }
    }

    /// Detect nullable wrapper patterns: `oneOf: [null, T]` or `anyOf: [..., null]`
    /// where removing null leaves a single type. Returns `Some(Nullable(inner))` if matched.
    fn try_lower_nullable_wrapper(
        &mut self,
        candidate_name: &str,
        obj: &ObjectSchema,
    ) -> Result<Option<IrTypeExpr>, LowerError> {
        // Check oneOf and anyOf for nullable pattern
        let members = if !obj.one_of.is_empty() {
            &obj.one_of
        } else if !obj.any_of.is_empty() {
            &obj.any_of
        } else {
            return Ok(None);
        };

        // Count null members and non-null members
        let mut null_count = 0;
        let mut non_null: Vec<&ObjectOrReference<ObjectSchema>> = Vec::new();
        for m in members {
            if is_null_schema(m) {
                null_count += 1;
            } else {
                non_null.push(m);
            }
        }

        // Only handle the case: exactly 1 null + 1 non-null member → Nullable(inner)
        if null_count != 1 || non_null.len() != 1 {
            return Ok(None);
        }

        let inner_ref = non_null[0];
        let inner = self.lower_schema_ref_with_promotion(candidate_name, inner_ref)?;
        Ok(Some(IrTypeExpr::Nullable(Box::new(inner))))
    }

    /// Lower array items, promoting inline object items to named schemas.
    fn lower_array_items_with_promotion(
        &mut self,
        candidate_name: &str,
        obj: &ObjectSchema,
    ) -> Result<IrTypeExpr, LowerError> {
        if let Some(items) = &obj.items {
            match items.as_ref() {
                Schema::Object(boxed_ref) => {
                    self.lower_schema_ref_with_promotion(candidate_name, boxed_ref)
                }
                Schema::Boolean(_) => Ok(IrTypeExpr::Any),
            }
        } else {
            Ok(IrTypeExpr::Any)
        }
    }

    fn lower_type_from_schema(&self, obj: &ObjectSchema) -> IrTypeExpr {
        // Single-value string enum → string literal type
        if obj.enum_values.len() == 1
            && let Some(serde_json::Value::String(s)) = obj.enum_values.first()
        {
            return IrTypeExpr::StringLiteral(s.clone());
        }

        // Multi-value string enum → inline string enum type
        if obj.enum_values.len() > 1 {
            let all_strings: Vec<String> = obj
                .enum_values
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            if all_strings.len() == obj.enum_values.len() {
                return IrTypeExpr::StringEnum(all_strings);
            }
        }

        match &obj.schema_type {
            Some(type_set) => type_set_to_type_expr(type_set, obj),
            None => {
                // No type specified, infer from context
                if !obj.properties.is_empty() {
                    IrTypeExpr::Any // complex inline object, handled by promotion
                } else if !obj.enum_values.is_empty() {
                    IrTypeExpr::Primitive(IrPrimitive::String) // enum without type
                } else {
                    IrTypeExpr::Any
                }
            }
        }
    }

    fn lower_schema_value_with_promotion(
        &mut self,
        candidate_name: &str,
        schema: &Schema,
    ) -> Result<IrTypeExpr, LowerError> {
        match schema {
            Schema::Boolean(_) => Ok(IrTypeExpr::Any),
            Schema::Object(boxed_ref) => {
                self.lower_schema_ref_with_promotion(candidate_name, boxed_ref)
            }
        }
    }

    // -------------------------------------------------------------------
    // Operation lowering
    // -------------------------------------------------------------------

    fn lower_path_item(
        &mut self,
        path: &str,
        item: &oas::PathItem,
    ) -> Result<Vec<IrOperation>, LowerError> {
        let mut ops = Vec::new();

        // Lower path-level parameters once; they apply to all operations under this path.
        let path_params = item
            .parameters
            .iter()
            .map(|p| self.lower_parameter_ref(p))
            .collect::<Result<Vec<_>, _>>()?;

        macro_rules! lower_method {
            ($field:ident, $method:expr) => {
                if let Some(op) = &item.$field {
                    ops.push(self.lower_operation(path, $method, op, &path_params)?);
                }
            };
        }

        lower_method!(get, "GET");
        lower_method!(put, "PUT");
        lower_method!(post, "POST");
        lower_method!(delete, "DELETE");
        lower_method!(options, "OPTIONS");
        lower_method!(head, "HEAD");
        lower_method!(patch, "PATCH");
        lower_method!(trace, "TRACE");

        Ok(ops)
    }

    fn lower_operation(
        &mut self,
        path: &str,
        method: &str,
        op: &oas::Operation,
        path_params: &[IrParameter],
    ) -> Result<IrOperation, LowerError> {
        let op_id = op
            .operation_id
            .clone()
            .unwrap_or_else(|| format!("{}{}", method.to_lowercase(), path.to_pascal_case()));

        // Operation-level parameters
        let op_parameters = op
            .parameters
            .iter()
            .map(|p| self.lower_parameter_ref(p))
            .collect::<Result<Vec<_>, _>>()?;

        // Merge: path-level params first, then operation-level params override by (name, location).
        let mut parameters = Vec::new();
        for pp in path_params {
            // Include path-level param only if the operation doesn't override it
            let overridden = op_parameters
                .iter()
                .any(|op| op.name == pp.name && op.location == pp.location);
            if !overridden {
                parameters.push(pp.clone());
            }
        }
        parameters.extend(op_parameters);

        // Request body
        let request_body = match &op.request_body {
            Some(ObjectOrReference::Object(rb)) => Some(self.lower_request_body(&op_id, rb)?),
            Some(ObjectOrReference::Ref { ref_path, .. }) => {
                self.lower_request_body_ref(&op_id, ref_path)?
            }
            None => None,
        };

        // Responses
        let responses = match &op.responses {
            Some(resp_map) => resp_map
                .iter()
                .map(|(status, resp_ref)| self.lower_response_ref(&op_id, status, resp_ref))
                .collect::<Result<Vec<_>, _>>()?,
            None => Vec::new(),
        };

        // Security
        let security = op
            .security
            .iter()
            .flat_map(|req| {
                req.0.iter().map(|(name, scopes)| IrSecurityRequirement {
                    scheme_name: name.clone(),
                    scopes: scopes.clone(),
                })
            })
            .collect();

        Ok(IrOperation {
            operation_id: op_id,
            tags: op.tags.clone(),
            method: method.to_string(),
            path: strip_server_path_prefix(path, &self.spec.servers),
            summary: op.summary.clone(),
            description: op.description.clone(),
            deprecated: op.deprecated.unwrap_or(false),
            parameters,
            request_body,
            responses,
            security,
        })
    }

    fn lower_parameter_ref(
        &mut self,
        param_ref: &ObjectOrReference<oas::Parameter>,
    ) -> Result<IrParameter, LowerError> {
        match param_ref {
            ObjectOrReference::Object(param) => self.lower_parameter(param),
            ObjectOrReference::Ref { ref_path, .. } => {
                // Resolve parameter ref
                if let Some(components) = &self.spec.components {
                    let name = extract_component_name(ref_path);
                    if let Some(ObjectOrReference::Object(param)) = components.parameters.get(&name)
                    {
                        return self.lower_parameter(param);
                    }
                }
                Err(LowerError::UnresolvedReference {
                    reference: ref_path.clone(),
                })
            }
        }
    }

    fn lower_parameter(&mut self, param: &oas::Parameter) -> Result<IrParameter, LowerError> {
        let type_expr = match &param.schema {
            Some(schema_ref) => self.lower_schema_ref(schema_ref)?,
            None => IrTypeExpr::Any,
        };

        Ok(IrParameter {
            name: param.name.clone(),
            location: match param.location {
                oas::ParameterIn::Path => ParameterLocation::Path,
                oas::ParameterIn::Query => ParameterLocation::Query,
                oas::ParameterIn::Header => ParameterLocation::Header,
                oas::ParameterIn::Cookie => ParameterLocation::Cookie,
            },
            type_expr,
            required: param.required.unwrap_or(false),
            description: param.description.clone(),
            default_value: param.schema.as_ref().and_then(|s| match s {
                ObjectOrReference::Object(obj) => obj.default.clone(),
                _ => None,
            }),
        })
    }

    fn lower_request_body(
        &mut self,
        parent_name: &str,
        rb: &oas::RequestBody,
    ) -> Result<IrRequestBody, LowerError> {
        let mut content = IndexMap::new();
        for (mime, media_type) in &rb.content {
            if let Some(schema_ref) = &media_type.schema {
                let type_expr = self.lower_schema_ref_with_promotion(
                    &format!("{parent_name}Request"),
                    schema_ref,
                )?;
                content.insert(mime.clone(), type_expr);
            }
        }

        Ok(IrRequestBody {
            required: rb.required.unwrap_or(false),
            description: rb.description.clone(),
            content,
        })
    }

    fn lower_request_body_ref(
        &mut self,
        parent_name: &str,
        ref_path: &str,
    ) -> Result<Option<IrRequestBody>, LowerError> {
        if let Some(components) = &self.spec.components {
            let name = extract_component_name(ref_path);
            if let Some(ObjectOrReference::Object(rb)) = components.request_bodies.get(&name) {
                return Ok(Some(self.lower_request_body(parent_name, rb)?));
            }
        }
        Err(LowerError::UnresolvedReference {
            reference: ref_path.to_string(),
        })
    }

    fn lower_response_ref(
        &mut self,
        parent_name: &str,
        status: &str,
        resp_ref: &ObjectOrReference<oas::Response>,
    ) -> Result<IrResponse, LowerError> {
        match resp_ref {
            ObjectOrReference::Object(resp) => self.lower_response(parent_name, status, resp),
            ObjectOrReference::Ref { ref_path, .. } => {
                if let Some(components) = &self.spec.components {
                    let name = extract_component_name(ref_path);
                    if let Some(ObjectOrReference::Object(resp)) = components.responses.get(&name) {
                        return self.lower_response(parent_name, status, resp);
                    }
                }
                Err(LowerError::UnresolvedReference {
                    reference: ref_path.clone(),
                })
            }
        }
    }

    fn lower_response(
        &mut self,
        parent_name: &str,
        status: &str,
        resp: &oas::Response,
    ) -> Result<IrResponse, LowerError> {
        let mut content = IndexMap::new();
        for (mime, media_type) in &resp.content {
            if let Some(schema_ref) = &media_type.schema {
                let type_expr = self.lower_schema_ref_with_promotion(
                    &format!("{parent_name}Response{status}"),
                    schema_ref,
                )?;
                content.insert(mime.clone(), type_expr);
            }
        }

        let mut headers = IndexMap::new();
        for (name, header_ref) in &resp.headers {
            if let ObjectOrReference::Object(header) = header_ref {
                let type_expr = match &header.schema {
                    Some(schema_ref) => self.lower_schema_ref(schema_ref)?,
                    None => IrTypeExpr::Any,
                };
                headers.insert(
                    name.clone(),
                    IrHeader {
                        description: header.description.clone(),
                        type_expr,
                        required: header.required.unwrap_or(false),
                    },
                );
            }
        }

        Ok(IrResponse {
            status: status.to_string(),
            description: resp.description.clone().unwrap_or_default(),
            content,
            item_content: IndexMap::new(),
            headers,
        })
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

fn extract_schema_name(ref_path: &str) -> String {
    ref_path
        .strip_prefix("#/components/schemas/")
        .unwrap_or(ref_path)
        .to_string()
}

fn extract_component_name(ref_path: &str) -> String {
    ref_path.rsplit('/').next().unwrap_or(ref_path).to_string()
}

fn type_set_to_type_expr(type_set: &SchemaTypeSet, obj: &ObjectSchema) -> IrTypeExpr {
    let types: Vec<SchemaType> = match type_set {
        SchemaTypeSet::Single(t) => vec![*t],
        SchemaTypeSet::Multiple(ts) => ts.clone(),
    };

    // Filter out null
    let non_null: Vec<SchemaType> = types
        .iter()
        .copied()
        .filter(|t| *t != SchemaType::Null)
        .collect();
    let has_null = types.contains(&SchemaType::Null);

    if non_null.is_empty() {
        return IrTypeExpr::Primitive(IrPrimitive::String); // null-only type, unusual
    }

    let base = if non_null.len() == 1 {
        single_type_to_expr(non_null[0], obj)
    } else {
        // Multiple non-null types (e.g. type: [string, integer]) → union
        let members: Vec<IrTypeExpr> = non_null
            .iter()
            .map(|t| single_type_to_expr(*t, obj))
            .collect();
        IrTypeExpr::Union(members)
    };

    if has_null {
        IrTypeExpr::Nullable(Box::new(base))
    } else {
        base
    }
}

fn single_type_to_expr(t: SchemaType, obj: &ObjectSchema) -> IrTypeExpr {
    match t {
        SchemaType::String => match obj.format.as_deref() {
            Some("date") => IrTypeExpr::Primitive(IrPrimitive::Date),
            Some("date-time") => IrTypeExpr::Primitive(IrPrimitive::DateTime),
            Some("uuid") => IrTypeExpr::Primitive(IrPrimitive::Uuid),
            Some("binary") => IrTypeExpr::Primitive(IrPrimitive::Binary),
            Some(fmt) => IrTypeExpr::Primitive(IrPrimitive::StringWithFormat(fmt.to_string())),
            None => IrTypeExpr::Primitive(IrPrimitive::String),
        },
        SchemaType::Integer => match obj.format.as_deref() {
            Some(fmt) => IrTypeExpr::Primitive(IrPrimitive::IntegerWithFormat(fmt.to_string())),
            None => IrTypeExpr::Primitive(IrPrimitive::Integer),
        },
        SchemaType::Number => match obj.format.as_deref() {
            Some(fmt) => IrTypeExpr::Primitive(IrPrimitive::NumberWithFormat(fmt.to_string())),
            None => IrTypeExpr::Primitive(IrPrimitive::Number),
        },
        SchemaType::Boolean => IrTypeExpr::Primitive(IrPrimitive::Boolean),
        SchemaType::Array => {
            if let Some(items) = &obj.items {
                match items.as_ref() {
                    Schema::Object(boxed_ref) => match boxed_ref.as_ref() {
                        ObjectOrReference::Ref { ref_path, .. } => IrTypeExpr::Array(Box::new(
                            IrTypeExpr::Named(extract_schema_name(ref_path)),
                        )),
                        ObjectOrReference::Object(inner) => {
                            let inner_type = single_type_to_expr(
                                inner
                                    .schema_type
                                    .as_ref()
                                    .map(|ts| match ts {
                                        oas::SchemaTypeSet::Single(t) => *t,
                                        oas::SchemaTypeSet::Multiple(ts) => ts
                                            .iter()
                                            .copied()
                                            .find(|t| *t != oas::SchemaType::Null)
                                            .unwrap_or(oas::SchemaType::String),
                                    })
                                    .unwrap_or(oas::SchemaType::String),
                                inner,
                            );
                            IrTypeExpr::Array(Box::new(inner_type))
                        }
                    },
                    Schema::Boolean(_) => IrTypeExpr::Array(Box::new(IrTypeExpr::Any)),
                }
            } else {
                IrTypeExpr::Array(Box::new(IrTypeExpr::Any))
            }
        }
        SchemaType::Object => {
            if let Some(ap) = &obj.additional_properties {
                match ap {
                    Schema::Boolean(b) => {
                        if b.0 {
                            IrTypeExpr::Map(Box::new(IrTypeExpr::Any))
                        } else {
                            IrTypeExpr::Any
                        }
                    }
                    Schema::Object(boxed_ref) => match boxed_ref.as_ref() {
                        ObjectOrReference::Ref { ref_path, .. } => IrTypeExpr::Map(Box::new(
                            IrTypeExpr::Named(extract_schema_name(ref_path)),
                        )),
                        ObjectOrReference::Object(inner) => {
                            let inner_type = single_type_to_expr(
                                inner
                                    .schema_type
                                    .as_ref()
                                    .map(|ts| match ts {
                                        oas::SchemaTypeSet::Single(t) => *t,
                                        oas::SchemaTypeSet::Multiple(ts) => ts
                                            .iter()
                                            .copied()
                                            .find(|t| *t != oas::SchemaType::Null)
                                            .unwrap_or(oas::SchemaType::String),
                                    })
                                    .unwrap_or(oas::SchemaType::String),
                                inner,
                            );
                            IrTypeExpr::Map(Box::new(inner_type))
                        }
                    },
                }
            } else {
                IrTypeExpr::Any
            }
        }
        SchemaType::Null => IrTypeExpr::Primitive(IrPrimitive::String), // standalone null, unusual
    }
}

fn classify_enum_values(values: &[serde_json::Value]) -> IrEnumValueType {
    let mut has_string = false;
    let mut has_number = false;
    let mut has_integer = false;

    for v in values {
        match v {
            serde_json::Value::String(_) => has_string = true,
            serde_json::Value::Number(n) => {
                if n.is_i64() || n.is_u64() {
                    has_integer = true;
                } else {
                    has_number = true;
                }
            }
            _ => {}
        }
    }

    let mixed_count = [has_string, has_number, has_integer]
        .iter()
        .filter(|&&b| b)
        .count();
    if mixed_count >= 2 {
        IrEnumValueType::Mixed
    } else if has_number {
        IrEnumValueType::Number
    } else if has_integer {
        IrEnumValueType::Integer
    } else {
        IrEnumValueType::String
    }
}

fn should_promote_to_named(obj: &ObjectSchema) -> bool {
    !obj.properties.is_empty()
        || !obj.all_of.is_empty()
        || !obj.any_of.is_empty()
        || !obj.one_of.is_empty()
}

/// Check if a schema reference is a null type (used for nullable oneOf detection).
fn is_null_schema(schema_ref: &ObjectOrReference<ObjectSchema>) -> bool {
    match schema_ref {
        ObjectOrReference::Object(obj) => {
            matches!(
                &obj.schema_type,
                Some(oas::SchemaTypeSet::Single(oas::SchemaType::Null))
            )
        }
        ObjectOrReference::Ref { .. } => false,
    }
}

/// Check if a schema is an array whose items are an inline object that should be promoted.
fn is_array_with_promotable_items(obj: &ObjectSchema) -> bool {
    let is_array = obj
        .schema_type
        .as_ref()
        .is_some_and(|t| matches!(t, oas::SchemaTypeSet::Single(oas::SchemaType::Array)));
    if !is_array {
        return false;
    }
    if let Some(items) = &obj.items
        && let Schema::Object(boxed_ref) = items.as_ref()
        && let ObjectOrReference::Object(inner) = boxed_ref.as_ref()
    {
        return should_promote_to_named(inner);
    }
    false
}

fn classify_tagging_style(
    patterns: &[Option<crate::ir::tagged_enum_pattern::TaggedEnumPattern>],
) -> TaggingStyle {
    use crate::ir::tagged_enum_pattern::TaggedEnumPattern;
    let mut saw_internal = false;
    let mut saw_adjacent: Option<String> = None;
    for p in patterns.iter().flatten() {
        match p {
            TaggedEnumPattern::AdjacentlyTagged { content_field, .. } => {
                saw_adjacent = Some(content_field.clone());
            }
            TaggedEnumPattern::InternallyTagged { .. } => {
                saw_internal = true;
            }
            TaggedEnumPattern::ExternallyTagged { .. } | TaggedEnumPattern::Untagged { .. } => {}
        }
    }
    if let Some(content_field) = saw_adjacent {
        return TaggingStyle::Adjacent { content_field };
    }
    if saw_internal {
        return TaggingStyle::Internal;
    }
    // All patterns are ExternallyTagged (unit variants with just the tag field).
    // Since we only reach this function when not all_external was already handled,
    // default to Internal.
    TaggingStyle::Internal
}

fn unwrap_nullable(expr: IrTypeExpr) -> (IrTypeExpr, bool) {
    match expr {
        IrTypeExpr::Nullable(inner) => (*inner, true),
        other => (other, false),
    }
}

fn extract_validation(obj: &ObjectSchema) -> Option<IrValidation> {
    let v = IrValidation {
        max_length: obj.max_length,
        min_length: obj.min_length,
        pattern: obj.pattern.clone(),
        maximum: obj.maximum.as_ref().and_then(|n| n.as_f64()),
        exclusive_maximum: obj.exclusive_maximum.as_ref().map(|_| true),
        minimum: obj.minimum.as_ref().and_then(|n| n.as_f64()),
        exclusive_minimum: obj.exclusive_minimum.as_ref().map(|_| true),
        multiple_of: obj.multiple_of.as_ref().and_then(|n| n.as_f64()),
        max_items: obj.max_items,
        min_items: obj.min_items,
        unique_items: obj.unique_items,
    };

    if v == IrValidation::default() {
        None
    } else {
        Some(v)
    }
}

fn lower_info(info: &oas::Info) -> IrInfo {
    IrInfo {
        title: info.title.clone(),
        description: info.description.clone(),
        version: info.version.clone(),
        terms_of_service: info.terms_of_service.as_ref().map(|u| u.to_string()),
        contact: info.contact.as_ref().map(|c| IrContact {
            name: c.name.clone(),
            url: c.url.as_ref().map(|u| u.to_string()),
            email: c.email.clone(),
        }),
        license: info.license.as_ref().map(|l| IrLicense {
            name: l.name.clone(),
            url: l.url.as_ref().map(|u| u.to_string()),
            identifier: l.identifier.clone(),
        }),
    }
}

fn lower_security_scheme(scheme: &oas::SecurityScheme) -> IrSecurityScheme {
    use oas::SecurityScheme as SS;
    match scheme {
        SS::ApiKey {
            name,
            location,
            description,
            ..
        } => IrSecurityScheme::ApiKey {
            name: name.clone(),
            location: match location.as_str() {
                "query" => ApiKeyLocation::Query,
                "header" => ApiKeyLocation::Header,
                "cookie" => ApiKeyLocation::Cookie,
                _ => ApiKeyLocation::Header, // default fallback
            },
            description: description.clone(),
        },
        SS::Http {
            scheme,
            bearer_format,
            description,
            ..
        } => IrSecurityScheme::Http {
            scheme: scheme.clone(),
            bearer_format: bearer_format.clone(),
            description: description.clone(),
        },
        SS::OAuth2 {
            flows, description, ..
        } => IrSecurityScheme::OAuth2 {
            flows: Box::new(lower_oauth2_flows(flows)),
            description: description.clone(),
        },
        SS::OpenIdConnect {
            open_id_connect_url,
            description,
            ..
        } => IrSecurityScheme::OpenIdConnect {
            open_id_connect_url: open_id_connect_url.clone(),
            description: description.clone(),
        },
        SS::MutualTls { description, .. } => IrSecurityScheme::MutualTls {
            description: description.clone(),
        },
    }
}

fn lower_oauth2_flows(flows: &oas::Flows) -> IrOAuth2Flows {
    IrOAuth2Flows {
        implicit: flows.implicit.as_ref().map(|f| IrOAuth2Flow {
            authorization_url: Some(f.authorization_url.to_string()),
            token_url: None,
            refresh_url: f.refresh_url.as_ref().map(|u| u.to_string()),
            scopes: f
                .scopes
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        }),
        password: flows.password.as_ref().map(|f| IrOAuth2Flow {
            authorization_url: None,
            token_url: Some(f.token_url.to_string()),
            refresh_url: f.refresh_url.as_ref().map(|u| u.to_string()),
            scopes: f
                .scopes
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        }),
        client_credentials: flows.client_credentials.as_ref().map(|f| IrOAuth2Flow {
            authorization_url: None,
            token_url: Some(f.token_url.to_string()),
            refresh_url: f.refresh_url.as_ref().map(|u| u.to_string()),
            scopes: f
                .scopes
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        }),
        authorization_code: flows.authorization_code.as_ref().map(|f| IrOAuth2Flow {
            authorization_url: Some(f.authorization_url.to_string()),
            token_url: Some(f.token_url.to_string()),
            refresh_url: f.refresh_url.as_ref().map(|u| u.to_string()),
            scopes: f
                .scopes
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        }),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn lower_yaml(yaml: &str) -> IrSpec {
        let spec = crate::parser::parse_content_yaml_v31(yaml).unwrap();
        lower_v31(&spec).unwrap()
    }

    #[test]
    fn test_lower_minimal_spec() {
        let ir = lower_yaml(
            r#"
openapi: "3.1.0"
info:
  title: Test
  version: "1.0"
paths: {}
"#,
        );
        assert_eq!(ir.info.title, "Test");
        assert_eq!(ir.info.version, "1.0");
        assert!(ir.schemas.is_empty());
        assert!(ir.operations.is_empty());
    }

    #[test]
    fn test_lower_simple_object_schema() {
        let ir = lower_yaml(
            r#"
openapi: "3.1.0"
info:
  title: Test
  version: "1.0"
components:
  schemas:
    User:
      type: object
      required:
        - name
      properties:
        name:
          type: string
        age:
          type: integer
"#,
        );
        assert_eq!(ir.schemas.len(), 1);
        let user = &ir.schemas["User"];
        assert_eq!(user.name, "User");
        if let IrSchemaKind::Object(obj) = &user.kind {
            assert_eq!(obj.properties.len(), 2);
            assert!(obj.properties["name"].required);
            assert!(!obj.properties["age"].required);
        } else {
            panic!("Expected Object schema");
        }
    }

    #[test]
    fn test_lower_string_enum() {
        let ir = lower_yaml(
            r#"
openapi: "3.1.0"
info:
  title: Test
  version: "1.0"
components:
  schemas:
    Status:
      type: string
      enum:
        - active
        - inactive
"#,
        );
        let status = &ir.schemas["Status"];
        if let IrSchemaKind::Enum(e) = &status.kind {
            assert_eq!(e.value_type, IrEnumValueType::String);
            assert_eq!(e.values.len(), 2);
        } else {
            panic!("Expected Enum schema");
        }
    }

    #[test]
    fn test_lower_nullable_type() {
        let ir = lower_yaml(
            r#"
openapi: "3.1.0"
info:
  title: Test
  version: "1.0"
components:
  schemas:
    MaybeName:
      type:
        - string
        - "null"
"#,
        );
        let maybe = &ir.schemas["MaybeName"];
        if let IrSchemaKind::Alias(IrTypeExpr::Nullable(inner)) = &maybe.kind {
            assert_eq!(**inner, IrTypeExpr::Primitive(IrPrimitive::String));
        } else {
            panic!("Expected Nullable alias, got {:?}", maybe.kind);
        }
    }

    #[test]
    fn test_lower_operation() {
        let ir = lower_yaml(
            r#"
openapi: "3.1.0"
info:
  title: Test
  version: "1.0"
paths:
  /users:
    get:
      operationId: listUsers
      parameters:
        - name: limit
          in: query
          schema:
            type: integer
      responses:
        "200":
          description: OK
          content:
            application/json:
              schema:
                type: array
                items:
                  type: string
"#,
        );
        assert_eq!(ir.operations.len(), 1);
        let op = &ir.operations[0];
        assert_eq!(op.operation_id, "listUsers");
        assert_eq!(op.method, "GET");
        assert_eq!(op.path, "/users");
        assert_eq!(op.parameters.len(), 1);
        assert_eq!(op.parameters[0].name, "limit");
        assert_eq!(op.responses.len(), 1);
        assert_eq!(op.responses[0].status, "200");
    }

    #[test]
    fn test_lower_array_alias() {
        let ir = lower_yaml(
            r#"
openapi: "3.1.0"
info:
  title: Test
  version: "1.0"
components:
  schemas:
    Tags:
      type: array
      items:
        type: string
"#,
        );
        let tags = &ir.schemas["Tags"];
        if let IrSchemaKind::Alias(IrTypeExpr::Array(inner)) = &tags.kind {
            assert_eq!(**inner, IrTypeExpr::Primitive(IrPrimitive::String));
        } else {
            panic!("Expected Array alias");
        }
    }

    #[test]
    fn test_lower_ref_schema() {
        let ir = lower_yaml(
            r##"
openapi: "3.1.0"
info:
  title: Test
  version: "1.0"
components:
  schemas:
    Pet:
      type: object
      properties:
        name:
          type: string
    MyPet:
      $ref: "#/components/schemas/Pet"
"##,
        );
        assert_eq!(ir.schemas.len(), 2);
        let my_pet = &ir.schemas["MyPet"];
        if let IrSchemaKind::Alias(IrTypeExpr::Named(name)) = &my_pet.kind {
            assert_eq!(name, "Pet");
        } else {
            panic!("Expected Named alias");
        }
    }
}
