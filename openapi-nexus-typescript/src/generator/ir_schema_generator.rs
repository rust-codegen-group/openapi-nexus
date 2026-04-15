//! IR-based schema generator: converts `IrSchema` → TypeScript AST nodes.
//!
//! This replaces `schema_generator.rs` for generators that consume the IR pipeline.
//! No `ObjectOrReference` matching, no `$ref` resolution, no `SchemaContext` —
//! the IR has already done all of that.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use heck::{ToLowerCamelCase as _, ToPascalCase as _};

use openapi_nexus_ir::types::{
    IrEnum, IrIntersection, IrObject, IrPrimitive, IrProperty, IrSchema, IrSchemaKind,
    IrTaggedUnion, IrTypeExpr, IrUnion, TaggingStyle,
};

use crate::ast::ty::ts_type_alias_definition::{IntersectionMemberInfo, UnionMemberInfo};
use crate::ast::{
    ObjectProperty, TsDocComment, TsEnumDefinition, TsEnumValue, TsEnumVariant, TsExpression,
    TsInterfaceDefinition, TsInterfaceSignature, TsPrimitive, TsProperty, TsTypeAliasDefinition,
    TsTypeDefinition,
};

/// Converts IR schemas to TypeScript AST type definitions.
pub struct IrSchemaGenerator;

/// Collected output from schema generation.
pub struct IrSchemaOutput {
    /// Main type definitions keyed by TS name (PascalCase).
    pub type_definitions: HashMap<String, TsTypeDefinition>,
    /// Enum discriminator info: interface name → (property name, enum value).
    pub enum_discriminators: HashMap<String, (String, String)>,
}

/// Result of generating a tagged union. Includes the main type alias,
/// wrapper interfaces for each variant, an optional Kind type alias,
/// and discriminator info for wrapper interfaces.
struct TaggedUnionResult {
    main_type: TsTypeDefinition,
    wrapper_interfaces: Vec<(String, TsTypeDefinition)>,
    kind_type: Option<(String, TsTypeDefinition)>,
    discriminators: Vec<(String, (String, String))>,
}

impl IrSchemaGenerator {
    /// Convert all IR schemas to TypeScript type definitions.
    pub fn generate(schemas: &indexmap::IndexMap<String, IrSchema>) -> IrSchemaOutput {
        let mut type_definitions = HashMap::new();
        let mut enum_discriminators = HashMap::new();

        for (name, schema) in schemas {
            let ts_name = name.to_pascal_case();

            match &schema.kind {
                IrSchemaKind::TaggedUnion(tagged) => {
                    let docs = schema.description.as_deref().map(TsDocComment::new);
                    let result = Self::tagged_union_to_types(&ts_name, name, tagged, docs, schemas);
                    // Insert main type alias
                    type_definitions.insert(ts_name, result.main_type);
                    // Insert wrapper interfaces
                    for (wrapper_name, wrapper_def) in result.wrapper_interfaces {
                        type_definitions.insert(wrapper_name, wrapper_def);
                    }
                    // Insert Kind type alias
                    if let Some((kind_name, kind_def)) = result.kind_type {
                        type_definitions.insert(kind_name, kind_def);
                    }
                    // Collect discriminators for wrapper interfaces
                    for (iface_name, disc_info) in result.discriminators {
                        enum_discriminators.insert(iface_name, disc_info);
                    }
                }
                _ => {
                    let type_def = Self::schema_to_type_def(name, schema, schemas);
                    type_definitions.insert(ts_name, type_def);
                }
            }
        }

        // Collect discriminator info from tagged unions for content type interfaces
        for (_name, schema) in schemas {
            if let IrSchemaKind::TaggedUnion(tagged) = &schema.kind {
                Self::collect_discriminators(tagged, &mut enum_discriminators);
            }
        }

        IrSchemaOutput {
            type_definitions,
            enum_discriminators,
        }
    }

    /// Convert a single IR schema to a TypeScript type definition.
    fn schema_to_type_def(
        name: &str,
        schema: &IrSchema,
        schemas: &indexmap::IndexMap<String, IrSchema>,
    ) -> TsTypeDefinition {
        let ts_name = name.to_pascal_case();
        let original_name = name.to_string();
        let docs = schema.description.as_deref().map(TsDocComment::new);

        match &schema.kind {
            IrSchemaKind::Object(obj) => TsTypeDefinition::Interface(Self::object_to_interface(
                &ts_name,
                &original_name,
                obj,
                docs,
            )),
            IrSchemaKind::Enum(e) => {
                TsTypeDefinition::Enum(Self::enum_to_ts_enum(&ts_name, &original_name, e, docs))
            }
            IrSchemaKind::TaggedUnion(_) => {
                // Handled in generate() directly; this shouldn't be called for tagged unions
                unreachable!("tagged unions are handled in generate()")
            }
            IrSchemaKind::Union(union_type) => {
                Self::union_to_type_alias(&ts_name, &original_name, union_type, docs, schemas)
            }
            IrSchemaKind::Intersection(intersection) => {
                Self::intersection_to_type_alias(&ts_name, &original_name, intersection, docs)
            }
            IrSchemaKind::Alias(type_expr) => {
                let ts_expr = Self::type_expr_to_ts(type_expr);
                TsTypeDefinition::TypeAlias(TsTypeAliasDefinition {
                    ts_name,
                    original_name,
                    type_expr: ts_expr,
                    generics: vec![],
                    documentation: docs,
                    union_members: None,
                    intersection_members: None,
                })
            }
        }
    }

    // =========================================================================
    // Object → Interface
    // =========================================================================

    fn object_to_interface(
        ts_name: &str,
        original_name: &str,
        obj: &IrObject,
        docs: Option<TsDocComment>,
    ) -> TsInterfaceDefinition {
        let signature = TsInterfaceSignature {
            is_export: true,
            ts_name: ts_name.to_string(),
            original_name: original_name.to_string(),
            generics: vec![],
            extends: vec![],
        };

        let mut properties: Vec<TsProperty> = obj
            .properties
            .iter()
            .map(|(_key, prop)| Self::property_to_ts(prop))
            .collect();

        // Handle additionalProperties as index signature
        if let Some(ap_type) = &obj.additional_properties {
            let mut value_type = Self::type_expr_to_ts(ap_type);

            // If there are also named properties, union their types with the index sig value
            // But skip if AP type is `any` since it already subsumes all types
            if !obj.properties.is_empty() && !matches!(ap_type, IrTypeExpr::Any) {
                let mut unique_types: BTreeSet<TsExpression> = obj
                    .properties
                    .values()
                    .map(|p| Self::type_expr_to_ts(&p.type_expr))
                    .collect();
                unique_types.insert(value_type.clone());
                value_type = TsExpression::Union(unique_types);
            }

            let index_name = "[key: string]".to_string();
            properties.push(TsProperty {
                ts_name: index_name.clone(),
                original_name: index_name,
                type_expr: value_type,
                optional: false,
                is_index_signature: true,
                documentation: Some(TsDocComment::new("Additional properties")),
            });
        }

        let mut iface = TsInterfaceDefinition::new(signature);
        iface.properties = properties;
        iface.documentation = docs;
        iface
    }

    fn property_to_ts(prop: &IrProperty) -> TsProperty {
        let mut type_expr = Self::type_expr_to_ts(&prop.type_expr);

        // If the property is nullable, wrap with null union
        if prop.nullable {
            let mut union = BTreeSet::new();
            union.insert(type_expr);
            union.insert(TsExpression::Primitive(TsPrimitive::Null));
            type_expr = TsExpression::Union(union);
        }

        TsProperty {
            ts_name: prop.name.to_lower_camel_case(),
            original_name: prop.name.clone(),
            type_expr,
            optional: !prop.required,
            is_index_signature: false,
            documentation: prop.description.as_deref().map(TsDocComment::new),
        }
    }

    // =========================================================================
    // Enum → TsEnumDefinition
    // =========================================================================

    fn enum_to_ts_enum(
        ts_name: &str,
        original_name: &str,
        ir_enum: &IrEnum,
        docs: Option<TsDocComment>,
    ) -> TsEnumDefinition {
        let mut def = TsEnumDefinition::new(ts_name.to_string(), original_name.to_string());
        def.documentation = docs;

        for ev in &ir_enum.values {
            let ts_value = TsEnumValue::from_json_value(&ev.value);
            let variant_name = ts_value.generate_enum_name();

            def.variants.push(TsEnumVariant {
                name: variant_name,
                value: Some(ts_value),
                documentation: ev.description.as_deref().map(TsDocComment::new),
            });
        }

        def
    }

    // =========================================================================
    // Tagged Union → TypeAlias + wrapper interfaces + Kind type
    // =========================================================================

    fn tagged_union_to_types(
        ts_name: &str,
        original_name: &str,
        tagged: &IrTaggedUnion,
        docs: Option<TsDocComment>,
        schemas: &indexmap::IndexMap<String, IrSchema>,
    ) -> TaggedUnionResult {
        let tag_field = &tagged.discriminator_field;
        let mut union_types = BTreeSet::new();
        let mut union_members = Vec::new();
        let mut wrapper_interfaces = Vec::new();
        let mut discriminators = Vec::new();

        for (idx, variant) in tagged.variants.iter().enumerate() {
            let disc_value = &variant.discriminator_value;
            let content_ts = Self::type_expr_to_ts(&variant.content_type);
            let disc_literal = TsExpression::Literal(format!("\"{}\"", disc_value));
            let variant_docs = variant.description.as_deref().map(TsDocComment::new);

            // Build the wrapper interface name
            let wrapper_name = Self::wrapper_interface_name(
                ts_name,
                &variant.content_type,
                disc_value,
                idx,
                &tagged.tagging,
                schemas,
            );

            match &tagged.tagging {
                TaggingStyle::Internal => {
                    // Type alias member: ({ tag: "LITERAL" } & ContentType)
                    let tag_obj =
                        Self::make_single_prop_object(tag_field, tag_field, disc_literal.clone());
                    let intersection =
                        TsExpression::Intersection(BTreeSet::from([tag_obj, content_ts.clone()]));
                    union_types.insert(intersection.clone());
                    union_members.push(UnionMemberInfo {
                        ts_name: wrapper_name.clone(),
                        type_expr: intersection,
                        is_primitive: false,
                        is_interface: true,
                    });

                    // Wrapper interface: flattened content properties + tag field
                    let wrapper_iface = Self::build_internal_wrapper(
                        &wrapper_name,
                        tag_field,
                        disc_value,
                        &variant.content_type,
                        schemas,
                    );
                    wrapper_interfaces.push((
                        wrapper_name.clone(),
                        TsTypeDefinition::Interface(wrapper_iface),
                    ));
                    discriminators.push((wrapper_name, (tag_field.clone(), disc_value.clone())));
                }
                TaggingStyle::Adjacent { content_field } => {
                    // Type alias member: { data: ContentType; tag: "LITERAL" }
                    let mut props = BTreeMap::new();
                    let content_field_camel = content_field.to_lower_camel_case();
                    props.insert(
                        content_field_camel.clone(),
                        ObjectProperty {
                            ts_name: content_field_camel.clone(),
                            original_name: content_field.clone(),
                            type_expr: content_ts.clone(),
                        },
                    );
                    let tag_camel = tag_field.to_lower_camel_case();
                    props.insert(
                        tag_camel.clone(),
                        ObjectProperty {
                            ts_name: tag_camel.clone(),
                            original_name: tag_field.clone(),
                            type_expr: disc_literal.clone(),
                        },
                    );
                    let obj_expr = TsExpression::Object(props);

                    union_types.insert(obj_expr.clone());
                    union_members.push(UnionMemberInfo {
                        ts_name: wrapper_name.clone(),
                        type_expr: obj_expr,
                        is_primitive: false,
                        is_interface: true,
                    });

                    // Wrapper interface: { content_field: ContentType, tag_field: "LITERAL" }
                    let mut wrapper_iface = Self::build_adjacent_wrapper(
                        &wrapper_name,
                        tag_field,
                        disc_value,
                        content_field,
                        &content_ts,
                    );
                    wrapper_iface.documentation = variant_docs.clone();
                    wrapper_interfaces.push((
                        wrapper_name.clone(),
                        TsTypeDefinition::Interface(wrapper_iface),
                    ));
                    discriminators.push((wrapper_name, (tag_field.clone(), disc_value.clone())));
                }
                TaggingStyle::External => {
                    // Type alias member: { camelCasedVariantName: ContentType }
                    let variant_key = disc_value.to_lower_camel_case();
                    let obj_expr =
                        Self::make_single_prop_object(&variant_key, disc_value, content_ts.clone());

                    union_types.insert(obj_expr.clone());
                    union_members.push(UnionMemberInfo {
                        ts_name: wrapper_name.clone(),
                        type_expr: obj_expr,
                        is_primitive: false,
                        is_interface: true,
                    });

                    // Wrapper interface: { camelCasedVariantName: ContentType }
                    let mut wrapper_iface =
                        Self::build_external_wrapper(&wrapper_name, disc_value, &content_ts);
                    wrapper_iface.documentation = variant_docs.clone();
                    wrapper_interfaces.push((
                        wrapper_name.clone(),
                        TsTypeDefinition::Interface(wrapper_iface),
                    ));
                    // External style doesn't have a separate discriminator property
                }
            }
        }

        let main_type = TsTypeDefinition::TypeAlias(TsTypeAliasDefinition {
            ts_name: ts_name.to_string(),
            original_name: original_name.to_string(),
            type_expr: TsExpression::Union(union_types),
            generics: vec![],
            documentation: docs,
            union_members: Some(union_members),
            intersection_members: None,
        });

        // Generate Kind type for Internal and Adjacent styles
        let kind_type = if !matches!(tagged.tagging, TaggingStyle::External) {
            let kind_name = format!("{}Kind", ts_name);
            let kind_original = format!("{}Kind", original_name);
            let mut kind_members = BTreeSet::new();
            for variant in &tagged.variants {
                kind_members.insert(TsExpression::Literal(format!(
                    "\"{}\"",
                    variant.discriminator_value
                )));
            }
            Some((
                kind_name.clone(),
                TsTypeDefinition::TypeAlias(TsTypeAliasDefinition {
                    ts_name: kind_name,
                    original_name: kind_original,
                    type_expr: TsExpression::Union(kind_members),
                    generics: vec![],
                    documentation: Some(TsDocComment::new(format!(
                        "Kind type for {} discriminator union",
                        ts_name
                    ))),
                    union_members: None,
                    intersection_members: None,
                }),
            ))
        } else {
            None
        };

        TaggedUnionResult {
            main_type,
            wrapper_interfaces,
            kind_type,
            discriminators,
        }
    }

    /// Compute wrapper interface name for a tagged union variant.
    fn wrapper_interface_name(
        parent_ts_name: &str,
        content_type: &IrTypeExpr,
        disc_value: &str,
        idx: usize,
        tagging: &TaggingStyle,
        schemas: &indexmap::IndexMap<String, IrSchema>,
    ) -> String {
        let base_name = match tagging {
            TaggingStyle::Internal | TaggingStyle::Adjacent { .. } => {
                // Name from discriminator value: ParentVariantName
                let variant_pascal = disc_value.to_pascal_case();
                if variant_pascal.starts_with(parent_ts_name) {
                    variant_pascal
                } else {
                    format!("{}{}", parent_ts_name, variant_pascal)
                }
            }
            TaggingStyle::External => {
                // Externally tagged: use Member1, Member2, etc.
                // (matching legacy behavior which uses member index)
                match content_type {
                    IrTypeExpr::Named(n) => {
                        let content_pascal = n.to_pascal_case();
                        if content_pascal.starts_with(parent_ts_name) {
                            content_pascal
                        } else {
                            format!("{}Member{}", parent_ts_name, idx + 1)
                        }
                    }
                    _ => format!("{}Member{}", parent_ts_name, idx + 1),
                }
            }
        };

        // Collision check: if the name matches a component schema, prefix with parent
        if schemas.contains_key(base_name.as_str())
            || schemas.keys().any(|k| k.to_pascal_case() == base_name)
        {
            format!("{}{}", parent_ts_name, base_name)
        } else {
            base_name
        }
    }

    /// Build a wrapper interface for internally tagged variants.
    /// Flattens the content type's properties alongside the discriminator field.
    fn build_internal_wrapper(
        wrapper_name: &str,
        tag_field: &str,
        disc_value: &str,
        content_type: &IrTypeExpr,
        schemas: &indexmap::IndexMap<String, IrSchema>,
    ) -> TsInterfaceDefinition {
        let signature = TsInterfaceSignature {
            is_export: true,
            ts_name: wrapper_name.to_string(),
            original_name: wrapper_name.to_string(),
            generics: vec![],
            extends: vec![],
        };

        let mut properties = Vec::new();

        // Flatten content type properties
        if let IrTypeExpr::Named(schema_name) = content_type
            && let Some(schema) = schemas.get(schema_name.as_str())
            && let IrSchemaKind::Object(obj) = &schema.kind
        {
            for (_key, prop) in &obj.properties {
                properties.push(Self::property_to_ts(prop));
            }
        }

        // Add discriminator field with literal type
        let tag_camel = tag_field.to_lower_camel_case();
        properties.push(TsProperty {
            ts_name: tag_camel,
            original_name: tag_field.to_string(),
            type_expr: TsExpression::Literal(format!("\"{}\"", disc_value)),
            optional: false,
            is_index_signature: false,
            documentation: None,
        });

        let mut iface = TsInterfaceDefinition::new(signature);
        iface.properties = properties;
        iface
    }

    /// Build a wrapper interface for adjacently tagged variants.
    /// Has a content field and a tag field.
    fn build_adjacent_wrapper(
        wrapper_name: &str,
        tag_field: &str,
        disc_value: &str,
        content_field: &str,
        content_ts: &TsExpression,
    ) -> TsInterfaceDefinition {
        let signature = TsInterfaceSignature {
            is_export: true,
            ts_name: wrapper_name.to_string(),
            original_name: wrapper_name.to_string(),
            generics: vec![],
            extends: vec![],
        };

        let content_camel = content_field.to_lower_camel_case();
        let tag_camel = tag_field.to_lower_camel_case();

        let properties = vec![
            TsProperty {
                ts_name: content_camel,
                original_name: content_field.to_string(),
                type_expr: content_ts.clone(),
                optional: false,
                is_index_signature: false,
                documentation: None,
            },
            TsProperty {
                ts_name: tag_camel,
                original_name: tag_field.to_string(),
                type_expr: TsExpression::Literal(format!("\"{}\"", disc_value)),
                optional: false,
                is_index_signature: false,
                documentation: None,
            },
        ];

        let mut iface = TsInterfaceDefinition::new(signature);
        iface.properties = properties;
        iface
    }

    /// Build a wrapper interface for externally tagged variants.
    /// Has a single property whose key is the camelCased variant name.
    fn build_external_wrapper(
        wrapper_name: &str,
        disc_value: &str,
        content_ts: &TsExpression,
    ) -> TsInterfaceDefinition {
        let signature = TsInterfaceSignature {
            is_export: true,
            ts_name: wrapper_name.to_string(),
            original_name: wrapper_name.to_string(),
            generics: vec![],
            extends: vec![],
        };

        let variant_key = disc_value.to_lower_camel_case();
        let properties = vec![TsProperty {
            ts_name: variant_key,
            original_name: disc_value.to_string(),
            type_expr: content_ts.clone(),
            optional: false,
            is_index_signature: false,
            documentation: None,
        }];

        let mut iface = TsInterfaceDefinition::new(signature);
        iface.properties = properties;
        iface
    }

    /// Helper: create a single-property Object expression.
    fn make_single_prop_object(
        ts_name: &str,
        original_name: &str,
        type_expr: TsExpression,
    ) -> TsExpression {
        let prop = ObjectProperty {
            ts_name: ts_name.to_string(),
            original_name: original_name.to_string(),
            type_expr,
        };
        TsExpression::Object(BTreeMap::from([(ts_name.to_string(), prop)]))
    }

    fn collect_discriminators(
        tagged: &IrTaggedUnion,
        discriminators: &mut HashMap<String, (String, String)>,
    ) {
        for variant in &tagged.variants {
            if let IrTypeExpr::Named(schema_name) = &variant.content_type {
                let iface_name = schema_name.to_pascal_case();
                discriminators.insert(
                    iface_name,
                    (
                        tagged.discriminator_field.clone(),
                        variant.discriminator_value.clone(),
                    ),
                );
            }
        }
    }

    // =========================================================================
    // Union → TypeAlias with union_members
    // =========================================================================

    fn union_to_type_alias(
        ts_name: &str,
        original_name: &str,
        union_type: &IrUnion,
        docs: Option<TsDocComment>,
        schemas: &indexmap::IndexMap<String, IrSchema>,
    ) -> TsTypeDefinition {
        let mut union_types = BTreeSet::new();
        let mut union_members = Vec::new();

        for member in &union_type.members {
            let ts_expr = Self::type_expr_to_ts(member);
            let member_name = match member {
                IrTypeExpr::Named(n) => n.to_pascal_case(),
                IrTypeExpr::Primitive(p) => Self::primitive_ts_name(p),
                IrTypeExpr::StringLiteral(val) => val.clone(),
                _ => ts_expr.to_string_formatted(),
            };
            let is_primitive = matches!(
                member,
                IrTypeExpr::Primitive(_) | IrTypeExpr::StringLiteral(_)
            );

            // A Named reference is an interface only if it refers to an Object schema
            let is_interface = match member {
                IrTypeExpr::Named(n) => schemas
                    .get(n.as_str())
                    .is_some_and(|s| matches!(s.kind, IrSchemaKind::Object(_))),
                _ => false,
            };

            union_types.insert(ts_expr.clone());
            union_members.push(UnionMemberInfo {
                ts_name: member_name,
                type_expr: ts_expr,
                is_primitive,
                is_interface,
            });
        }

        // Add null to the union if the union is nullable
        if union_type.nullable {
            union_types.insert(TsExpression::Primitive(TsPrimitive::Null));
            union_members.push(UnionMemberInfo {
                ts_name: "null".to_string(),
                type_expr: TsExpression::Primitive(TsPrimitive::Null),
                is_primitive: true,
                is_interface: false,
            });
        }

        TsTypeDefinition::TypeAlias(TsTypeAliasDefinition {
            ts_name: ts_name.to_string(),
            original_name: original_name.to_string(),
            type_expr: TsExpression::Union(union_types),
            generics: vec![],
            documentation: docs,
            union_members: Some(union_members),
            intersection_members: None,
        })
    }

    // =========================================================================
    // Intersection → TypeAlias with intersection_members
    // =========================================================================

    fn intersection_to_type_alias(
        ts_name: &str,
        original_name: &str,
        intersection: &IrIntersection,
        docs: Option<TsDocComment>,
    ) -> TsTypeDefinition {
        let mut intersection_types = BTreeSet::new();
        let mut intersection_members = Vec::new();

        for member in &intersection.members {
            let ts_expr = Self::type_expr_to_ts(member);
            let is_reference = matches!(member, IrTypeExpr::Named(_));
            let member_name = match member {
                IrTypeExpr::Named(n) => n.to_pascal_case(),
                _ => ts_expr.to_string_formatted(),
            };

            intersection_types.insert(ts_expr.clone());
            intersection_members.push(IntersectionMemberInfo {
                ts_name: member_name,
                type_expr: ts_expr,
                is_reference,
                is_object: false,
                object_properties: None,
            });
        }

        TsTypeDefinition::TypeAlias(TsTypeAliasDefinition {
            ts_name: ts_name.to_string(),
            original_name: original_name.to_string(),
            type_expr: TsExpression::Intersection(intersection_types),
            generics: vec![],
            documentation: docs,
            union_members: None,
            intersection_members: Some(intersection_members),
        })
    }

    // =========================================================================
    // IrTypeExpr → TsExpression
    // =========================================================================

    pub fn type_expr_to_ts(type_expr: &IrTypeExpr) -> TsExpression {
        match type_expr {
            IrTypeExpr::Named(name) => TsExpression::Reference(name.to_pascal_case()),
            IrTypeExpr::Primitive(p) => Self::primitive_to_ts(p),
            IrTypeExpr::StringLiteral(val) => TsExpression::Literal(format!("\"{}\"", val)),
            IrTypeExpr::StringEnum(values) => {
                let mut union = BTreeSet::new();
                for v in values {
                    union.insert(TsExpression::Literal(format!("\"{}\"", v)));
                }
                TsExpression::Union(union)
            }
            IrTypeExpr::Array(inner) => TsExpression::Array(Box::new(Self::type_expr_to_ts(inner))),
            IrTypeExpr::Map(inner) => {
                let value_type = Self::type_expr_to_ts(inner);
                let index_key = "[key: string]".to_string();
                let prop = ObjectProperty {
                    ts_name: index_key.clone(),
                    original_name: index_key.clone(),
                    type_expr: value_type,
                };
                TsExpression::Object(BTreeMap::from([(index_key, prop)]))
            }
            IrTypeExpr::Nullable(inner) => {
                let inner_ts = Self::type_expr_to_ts(inner);
                let mut union = BTreeSet::new();
                union.insert(inner_ts);
                union.insert(TsExpression::Primitive(TsPrimitive::Null));
                TsExpression::Union(union)
            }
            IrTypeExpr::Union(members) => {
                let ts_members: BTreeSet<TsExpression> =
                    members.iter().map(Self::type_expr_to_ts).collect();
                TsExpression::Union(ts_members)
            }
            IrTypeExpr::Any => TsExpression::Primitive(TsPrimitive::Any),
        }
    }

    fn primitive_to_ts(p: &IrPrimitive) -> TsExpression {
        match p {
            IrPrimitive::String
            | IrPrimitive::Date
            | IrPrimitive::DateTime
            | IrPrimitive::Uuid
            | IrPrimitive::Binary
            | IrPrimitive::StringWithFormat(_) => TsExpression::Primitive(TsPrimitive::String),
            IrPrimitive::Integer
            | IrPrimitive::Number
            | IrPrimitive::IntegerWithFormat(_)
            | IrPrimitive::NumberWithFormat(_) => TsExpression::Primitive(TsPrimitive::Number),
            IrPrimitive::Boolean => TsExpression::Primitive(TsPrimitive::Boolean),
        }
    }

    fn primitive_ts_name(p: &IrPrimitive) -> String {
        match p {
            IrPrimitive::String
            | IrPrimitive::Date
            | IrPrimitive::DateTime
            | IrPrimitive::Uuid
            | IrPrimitive::Binary
            | IrPrimitive::StringWithFormat(_) => "string".to_string(),
            IrPrimitive::Integer
            | IrPrimitive::Number
            | IrPrimitive::IntegerWithFormat(_)
            | IrPrimitive::NumberWithFormat(_) => "number".to_string(),
            IrPrimitive::Boolean => "boolean".to_string(),
        }
    }
}
