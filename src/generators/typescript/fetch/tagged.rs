//! Structured tagged declarations and payload delegation through the value plan.

use heck::{ToLowerCamelCase, ToPascalCase};
use sigil_stitch::error::SigilStitchError;
use sigil_stitch::lang::typescript::TypeScript;
use sigil_stitch::prelude::{CodeBlock, TypeName, sigil_quote};
use sigil_stitch::spec::modifiers::Visibility;
use sigil_stitch::spec::{file_spec::FileSpec, fun_spec::FunSpec, parameter_spec::ParameterSpec};

use super::config::typescript_fetch_config::TaggedUnionStrategy;
use crate::generators::union_match::{JsonShape, plan_tagged_variant_shape};
use crate::generators::union_wire::{TagOwner, WireBinding};
use crate::generators::{closed_sum::is_tagged_unit_variant, type_application};
use crate::ir::types::{IrSchema, IrSpec, IrTaggedUnion, IrTaggedVariant, TaggingStyle};
use serde_json::Value;

use super::layout;
use super::plan::{Direction, Scope, TaggedMatch, TypeScriptPlan, union_type};
use super::sigil_emit::{
    build_type_alias, fn_base_name, is_unspecified_variant, variant_guard_name,
};

fn record(plan: &TypeScriptPlan<'_>, key: &str, value: TypeName) -> TypeName {
    type_application(
        plan.utility_type("Record"),
        vec![TypeName::string_literal(key.to_string()), value],
    )
}

fn discriminator_values(variant: &IrTaggedVariant) -> Vec<String> {
    if variant.discriminator_values.is_empty() {
        vec![variant.discriminator_value.clone()]
    } else {
        variant.discriminator_values.clone()
    }
}

fn discriminator_type(
    plan: &TypeScriptPlan<'_>,
    field: &str,
    variant: &IrTaggedVariant,
) -> TypeName {
    let values = union_type(
        discriminator_values(variant)
            .into_iter()
            .map(TypeName::string_literal)
            .collect(),
    );
    let tag = record(plan, field, values);
    if variant.discriminator_optional {
        type_application(plan.utility_type("Partial"), vec![tag])
    } else {
        tag
    }
}

fn authored_tag_type(values: Option<Vec<Value>>) -> TypeName {
    let Some(values) = values else {
        return TypeName::primitive("unknown");
    };
    if values.is_empty() {
        return TypeName::primitive("never");
    }
    union_type(
        values
            .into_iter()
            .map(|value| match value {
                Value::String(value) => TypeName::string_literal(value),
                Value::Null => TypeName::primitive("null"),
                Value::Bool(value) => TypeName::primitive(if value { "true" } else { "false" }),
                Value::Number(value) if !value.is_f64() => {
                    TypeName::integer_literal(value.to_string())
                }
                Value::Number(_) => TypeName::primitive("number"),
                _ => TypeName::primitive("unknown"),
            })
            .collect(),
    )
}

fn tag_projection(
    plan: &TypeScriptPlan<'_>,
    schema: &str,
    index: usize,
    disc: &str,
    variant: &IrTaggedVariant,
) -> (TypeName, TypeName, bool) {
    let domain = plan
        .authored_tag_plan(schema)
        .and_then(|wire| wire.tag_domain(index));
    if let Some(domain) = domain {
        let value = authored_tag_type(domain.values);
        let tag = record(plan, disc, value.clone());
        let tag = if domain.allows_absent {
            type_application(plan.utility_type("Partial"), vec![tag])
        } else {
            tag
        };
        (tag, value, domain.allows_absent)
    } else {
        (
            discriminator_type(plan, disc, variant),
            TypeName::primitive("unknown"),
            variant.discriminator_optional,
        )
    }
}

fn captured_tag_output(target_field: &str) -> Result<CodeBlock, SigilStitchError> {
    // The shared domain keeps missing separate from null. At the TypeScript
    // JSON seam an optional undefined value is also omitted on output.
    let present = layout::inline_object(&[sigil_quote!(TypeScript { [$S(target_field)]: tag })?])?;
    sigil_quote!(TypeScript { ...(tag === undefined ? {} : $C(present)) })
}

fn erased_record(plan: &TypeScriptPlan<'_>) -> TypeName {
    type_application(
        plan.utility_type("Record"),
        vec![
            TypeName::primitive("string"),
            TypeName::primitive("unknown"),
        ],
    )
}

fn capture_tag(
    source: &str,
    field: &str,
    plan: &TypeScriptPlan<'_>,
    body: CodeBlock,
) -> Result<CodeBlock, SigilStitchError> {
    sigil_quote!(TypeScript {
        const tag = ($N(source) as unknown as $T(erased_record(plan)))[$S(field)];
        $C(body)
    })
}

fn discriminator_condition(
    source: &str,
    field: &str,
    variant: &IrTaggedVariant,
) -> Result<CodeBlock, SigilStitchError> {
    let values = discriminator_values(variant);
    let has_values = !values.is_empty();
    let mut checks = CodeBlock::builder();
    for (index, value) in values.into_iter().enumerate() {
        if index > 0 {
            checks.add(" || ", ());
        }
        checks.add_code(layout::inline(sigil_quote!(TypeScript {
            $N(source)[$S(field)] === $S(value)
        })?));
    }
    if variant.discriminator_optional {
        if has_values {
            checks.add(" || ", ());
        }
        checks.add_code(layout::inline(sigil_quote!(TypeScript {
            $N(source)[$S(field)] === undefined
        })?));
    }
    checks.build()
}

fn discriminator_output(
    source: &str,
    source_field: &str,
    target_field: &str,
    variant: &IrTaggedVariant,
) -> Result<CodeBlock, SigilStitchError> {
    if variant.discriminator_optional {
        let source_tag = sigil_quote!(TypeScript {
            $N(source)[$S(source_field)]
        })?;
        let key = sigil_quote!(TypeScript { [$S(target_field)] })?;
        let object = CodeBlock::of(
            "{ %L: %L }",
            vec![
                sigil_stitch::code_block::Arg::Code(layout::inline(key)),
                sigil_stitch::code_block::Arg::Code(layout::inline(source_tag.clone())),
            ],
        )?;
        return sigil_quote!(TypeScript {
            ...($C(source_tag.clone()) !== undefined
                ? $C(object)
                : {})
        });
    }
    if !variant.discriminator_values.is_empty() {
        let source_tag = sigil_quote!(TypeScript {
            $N(source)[$S(source_field)]
        })?;
        return sigil_quote!(TypeScript {
            [$S(target_field)]: $C(source_tag)
        });
    }
    sigil_quote!(TypeScript {
        [$S(target_field)]: $S(variant.discriminator_value)
    })
}

fn uses_extended_tag_facts(variant: &IrTaggedVariant) -> bool {
    variant.discriminator_optional || variant.discriminator_values.len() > 1
}

fn discriminator_values_for_overlap(variant: &IrTaggedVariant) -> Vec<&str> {
    if variant.discriminator_values.is_empty() {
        vec![variant.discriminator_value.as_str()]
    } else {
        variant
            .discriminator_values
            .iter()
            .map(String::as_str)
            .collect()
    }
}

fn variants_can_overlap(left: &IrTaggedVariant, right: &IrTaggedVariant) -> bool {
    if left.discriminator_optional && right.discriminator_optional {
        return true;
    }
    let left_values = discriminator_values_for_overlap(left);
    let right_values = discriminator_values_for_overlap(right);
    left_values
        .iter()
        .any(|left| right_values.iter().any(|right| left == right))
}

fn variant_condition(
    source: &str,
    direction: Direction,
    union: &IrTaggedUnion,
    variant: &IrTaggedVariant,
    plan: &TypeScriptPlan<'_>,
) -> Result<CodeBlock, SigilStitchError> {
    let disc = match direction {
        Direction::FromWire => union.discriminator_field.clone(),
        Direction::ToWire => union.discriminator_field.to_lower_camel_case(),
    };
    match union.tagging {
        TaggingStyle::External => sigil_quote!(TypeScript {
            Object.prototype.hasOwnProperty.call($N(source), $S(variant.discriminator_value))
        }),
        _ if uses_extended_tag_facts(variant) => plan.matching_condition(
            &plan_tagged_variant_shape(union, variant, plan.ir()),
            sigil_quote!(TypeScript { $N(source) })?,
            direction,
        ),
        _ => discriminator_condition(source, &disc, variant),
    }
}

fn selected_variant_condition(
    source: &str,
    direction: Direction,
    union: &IrTaggedUnion,
    index: usize,
    plan: &TypeScriptPlan<'_>,
) -> Result<CodeBlock, SigilStitchError> {
    if !union.variants.iter().any(uses_extended_tag_facts) {
        return variant_condition(source, direction, union, &union.variants[index], plan);
    }
    let own = variant_condition(source, direction, union, &union.variants[index], plan)?;
    let mut other = CodeBlock::builder();
    let mut count = 0;
    for (other_index, variant) in union.variants.iter().enumerate() {
        if other_index == index || !variants_can_overlap(&union.variants[index], variant) {
            continue;
        }
        if count > 0 {
            other.add(" || ", ());
        }
        other.add_code(layout::inline(variant_condition(
            source, direction, union, variant, plan,
        )?));
        count += 1;
    }
    if count == 0 {
        return Ok(own);
    }
    let other = other.build()?;
    if count == 1 {
        sigil_quote!(TypeScript { $C(own) && !$C(other) })
    } else {
        sigil_quote!(TypeScript { $C(own) && !($C(other)) })
    }
}

pub(super) fn open_external_variant_type(
    plan: &TypeScriptPlan<'_>,
    key: &str,
    payload: TypeName,
    additional: Option<TypeName>,
) -> TypeName {
    let required = type_application(
        plan.utility_type("Readonly"),
        vec![record(plan, key, payload.clone())],
    );
    match additional {
        Some(additional) => {
            let dictionary = type_application(
                plan.utility_type("Record"),
                vec![
                    TypeName::primitive("string"),
                    union_type(vec![additional, payload]),
                ],
            );
            TypeName::intersection(vec![
                required,
                type_application(plan.utility_type("Readonly"), vec![dictionary]),
            ])
        }
        None => required,
    }
}

fn variant_type(
    schema: &IrSchema,
    ir: &IrSpec,
    union: &IrTaggedUnion,
    variant: &IrTaggedVariant,
    index: usize,
    plan: &TypeScriptPlan<'_>,
    wire: bool,
) -> TypeName {
    let rename = !wire && plan.flags().property_naming_camel_case;
    let disc = if rename {
        union.discriminator_field.to_lower_camel_case()
    } else {
        union.discriminator_field.clone()
    };
    let value = plan.value(&variant.content_type, Scope::model(&schema.name));
    let mut payload = if wire {
        value.wire_type()
    } else {
        value.public_type()
    };
    let (tag, tag_value, optional) = tag_projection(plan, &schema.name, index, &disc, variant);
    if let Some(WireBinding::Internal {
        owner: TagOwner::Case,
        ..
    }) = plan
        .authored_tag_plan(&schema.name)
        .and_then(|wire| wire.binding(index))
        && let crate::ir::types::IrTypeExpr::Named(name) = variant.content_type.representation()
        && let Some(crate::ir::types::IrSchemaKind::Object(object)) =
            ir.schemas.get(name).map(|model| &model.kind)
        && object.additional_properties.is_some()
    {
        let keys = union_type(
            object
                .properties
                .keys()
                .map(|key| {
                    TypeName::string_literal(if rename {
                        key.to_lower_camel_case()
                    } else {
                        key.clone()
                    })
                })
                .collect(),
        );
        let mut known = if object.properties.is_empty() {
            vec![]
        } else {
            vec![type_application(
                plan.utility_type("Pick"),
                vec![payload, keys],
            )]
        };
        let mut extra = vec![tag_value];
        if optional {
            extra.push(TypeName::primitive("undefined"));
        }
        known.push(
            super::sigil_emit::additional_properties_type(
                object,
                &schema.name,
                wire,
                plan,
                plan.unknown_aliases(),
                extra,
            )
            .expect("open payload"),
        );
        payload = if known.len() == 1 {
            known.pop().unwrap()
        } else {
            TypeName::intersection(known)
        };
    }
    match &union.tagging {
        TaggingStyle::Internal if is_tagged_unit_variant(ir, union, variant) => tag,
        TaggingStyle::Internal => {
            let mut members = vec![tag];
            if let TypeName::Intersection(payload) = payload {
                members.extend(payload);
            } else {
                members.push(payload);
            }
            TypeName::intersection(members)
        }
        TaggingStyle::Adjacent { .. } if is_tagged_unit_variant(ir, union, variant) => tag,
        TaggingStyle::Adjacent { content_field } => {
            let content = if rename {
                content_field.to_lower_camel_case()
            } else {
                content_field.clone()
            };
            TypeName::intersection(vec![tag, record(plan, &content, payload)])
        }
        TaggingStyle::External => {
            if !union
                .variants
                .iter()
                .any(|variant| variant.additional_properties.is_some())
            {
                return record(plan, &variant.discriminator_value, payload);
            }
            let additional = variant.additional_properties.as_ref().map(|expr| {
                let value = plan.value(expr, Scope::model(&schema.name));
                if wire {
                    value.wire_type()
                } else {
                    value.public_type()
                }
            });
            open_external_variant_type(plan, &variant.discriminator_value, payload, additional)
        }
    }
}

pub(super) fn guards(
    schema: &IrSchema,
    ir: &IrSpec,
    union: &IrTaggedUnion,
    plan: &TypeScriptPlan<'_>,
) -> Result<Vec<CodeBlock>, SigilStitchError> {
    union
        .variants
        .iter()
        .enumerate()
        .filter(|(_, variant)| {
            !is_tagged_unit_variant(ir, union, variant)
                && !is_unspecified_variant(
                    &variant.discriminator_value,
                    &union.discriminator_field,
                    &variant.content_type,
                )
        })
        .map(|(index, variant)| {
            let guard = variant_guard_name(variant);
            let parent = TypeName::primitive(&schema.name.to_pascal_case());
            let narrowed = variant_type(
                schema,
                ir,
                union,
                variant,
                index,
                plan,
                plan.is_raw_tagged(&schema.name),
            );
            let check = if plan.authored_tag_plan(&schema.name).is_some() {
                let direction = if plan.flags().property_naming_camel_case
                    && !plan.is_raw_tagged(&schema.name)
                {
                    Direction::ToWire
                } else {
                    Direction::FromWire
                };
                plan.matching_condition(
                    plan.authored_tag_shape(&schema.name, index, TaggedMatch::Member),
                    sigil_quote!(TypeScript { value })?,
                    direction,
                )?
            } else {
                selected_variant_condition("value", Direction::ToWire, union, index, plan)?
            };
            layout::type_guard(
                &guard,
                parent,
                narrowed,
                sigil_quote!(TypeScript { return $C(check); })?,
            )
        })
        .collect()
}

struct InternalCodec<'a, 'ir> {
    owner: TagOwner,
    unit: bool,
    source_field: &'a str,
    target_field: &'a str,
    payload_type: TypeName,
    output_type: TypeName,
    value: &'a super::plan::ValuePlan<'a, 'ir>,
    direction: Direction,
    plan: &'a TypeScriptPlan<'ir>,
}

impl InternalCodec<'_, '_> {
    fn body(self) -> Result<CodeBlock, SigilStitchError> {
        let source = self.direction.source();
        let source_field = self.source_field;
        let payload_type = self.payload_type;
        let output_type = self.output_type;
        let record = erased_record(self.plan);
        let tag = captured_tag_output(self.target_field)?;
        if self.unit {
            let result = layout::return_object(&[tag], Some(output_type), 4)?;
            return capture_tag(source, source_field, self.plan, result);
        }
        if self.owner == TagOwner::Payload {
            let content = self.value.convert(
                sigil_quote!(TypeScript { $N(source) as unknown as $T(payload_type) })?,
                self.direction,
            )?;
            return sigil_quote!(TypeScript { return $C(content) as $T(output_type); });
        }
        let projecting = self.owner == TagOwner::Case;
        let pattern = layout::inline_object(&[
            sigil_quote!(TypeScript { [$S(source_field)]: tag })?,
            sigil_quote!(TypeScript { ...payload })?,
        ])?;
        let access = if projecting { "payload" } else { source };
        let content = self.value.convert(
            sigil_quote!(TypeScript { $N(access) as unknown as $T(payload_type) })?,
            self.direction,
        )?;
        let result = layout::return_object(
            &[sigil_quote!(TypeScript { ...$C(content) })?, tag],
            Some(output_type),
            4,
        )?;
        sigil_quote!(TypeScript {
            $if(projecting) {
                const $C(pattern) = $N(source) as unknown as $T(record);
            } $else {
                const tag = ($N(source) as unknown as $T(record))[$S(source_field)];
            }
            $C(result)
        })
    }
}

pub(super) fn emit(
    schema: &IrSchema,
    ir: &IrSpec,
    union: &IrTaggedUnion,
    plan: &TypeScriptPlan<'_>,
    ts: &TypeScript,
) -> Result<FileSpec, SigilStitchError> {
    let name = schema.name.to_pascal_case();
    let wire_name = format!("{name}$Wire");
    let mut file = FileSpec::builder_with(&format!("{name}.ts"), ts.clone());
    let camel = plan.flags().property_naming_camel_case;
    for wire in [true, false].into_iter().filter(|wire| !*wire || camel) {
        let variants = union
            .variants
            .iter()
            .enumerate()
            .map(|(index, variant)| {
                variant_type(
                    schema,
                    ir,
                    union,
                    variant,
                    index,
                    plan,
                    wire || plan.is_raw_tagged(&schema.name),
                )
            })
            .collect();
        file = file.add_type(build_type_alias(
            if wire { &wire_name } else { &name },
            union_type(variants),
            schema.description.as_deref(),
        )?);
    }
    for direction in [Direction::FromWire, Direction::ToWire]
        .into_iter()
        .filter(|_| camel)
    {
        let source = direction.source();
        let (input, output) = match direction {
            Direction::FromWire => (&wire_name, &name),
            Direction::ToWire => (&name, &wire_name),
        };
        let output_type = TypeName::primitive(output);
        if plan.is_raw_tagged(&schema.name) {
            file = file.add_function(
                FunSpec::builder(&format!("{}{}", fn_base_name(&name), direction.suffix()))
                    .visibility(Visibility::Public)
                    .add_param(ParameterSpec::of(source, TypeName::primitive(input)))
                    .returns(vec![output_type])
                    .body(sigil_quote!(TypeScript { return $N(source); })?)
                    .build()?,
            );
            continue;
        }
        let authored = plan.authored_tag_plan(&schema.name);
        let strict =
            authored.is_some() && plan.flags().tagged_union_strategy == TaggedUnionStrategy::Strict;
        let strict_checks = if strict {
            union
                .variants
                .iter()
                .enumerate()
                .map(|(index, _)| {
                    plan.matching_condition(
                        plan.authored_tag_shape(&schema.name, index, TaggedMatch::Concrete),
                        sigil_quote!(TypeScript { $N(source) })?,
                        direction,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?
        } else {
            Vec::new()
        };
        let mut branches = Vec::new();
        let mut fallback = Vec::new();
        for (index, variant) in union.variants.iter().enumerate() {
            let disc = match direction {
                Direction::FromWire => union.discriminator_field.clone(),
                Direction::ToWire => union.discriminator_field.to_lower_camel_case(),
            };
            let target_disc = match direction {
                Direction::FromWire => union.discriminator_field.to_lower_camel_case(),
                Direction::ToWire => union.discriminator_field.clone(),
            };
            let value = plan.value(&variant.content_type, Scope::model(&schema.name));
            let content_type = match direction {
                Direction::FromWire => value.wire_type(),
                Direction::ToWire => value.public_type(),
            };
            let binding = authored.and_then(|wire| wire.binding(index));
            let tag_output = || {
                if binding.is_some() {
                    captured_tag_output(&target_disc)
                } else {
                    discriminator_output(source, &disc, &target_disc, variant)
                }
            };

            let body = if let Some(WireBinding::Internal { owner, .. }) = binding {
                InternalCodec {
                    owner,
                    unit: is_tagged_unit_variant(ir, union, variant),
                    source_field: &disc,
                    target_field: &target_disc,
                    payload_type: content_type,
                    output_type: output_type.clone(),
                    value: &value,
                    direction,
                    plan,
                }
                .body()?
            } else if is_tagged_unit_variant(ir, union, variant)
                && !matches!(union.tagging, TaggingStyle::External)
            {
                let tag = tag_output()?;
                layout::return_object(&[tag], Some(output_type.clone()), 4)?
            } else {
                match &union.tagging {
                    TaggingStyle::Internal => {
                        let content = value.convert(
                            sigil_quote!(TypeScript { $N(source) as unknown as $T(content_type) })?,
                            direction,
                        )?;
                        let tag = tag_output()?;
                        layout::return_object(
                            &[sigil_quote!(TypeScript { ...$C(content) })?, tag],
                            Some(output_type.clone()),
                            4,
                        )?
                    }
                    TaggingStyle::Adjacent { content_field } => {
                        let source_key = match direction {
                            Direction::FromWire => content_field.clone(),
                            Direction::ToWire => content_field.to_lower_camel_case(),
                        };
                        let target_key = match direction {
                            Direction::FromWire => content_field.to_lower_camel_case(),
                            Direction::ToWire => content_field.clone(),
                        };
                        let column =
                            6 + layout::fragment_width(
                                &sigil_quote!(TypeScript { [$S(target_key.clone())]: })?,
                            )? + 1;
                        let access = layout::indexed_assertion(
                            sigil_quote!(TypeScript { $N(source) })?,
                            record(plan, &source_key, content_type),
                            &source_key,
                            8,
                        )?;
                        let content = value.convert_at(access, direction, column)?;
                        let tag = tag_output()?;
                        layout::return_object(
                            &[
                                tag,
                                sigil_quote!(TypeScript { [$S(target_key)]: $C(content) })?,
                            ],
                            Some(output_type.clone()),
                            4,
                        )?
                    }
                    TaggingStyle::External => {
                        let tag = &variant.discriminator_value;
                        let column = 6
                            + layout::fragment_width(&sigil_quote!(TypeScript { [$S(tag)]: })?)?
                            + 1;
                        let access = layout::indexed_assertion(
                            sigil_quote!(TypeScript { $N(source) })?,
                            record(plan, tag, content_type),
                            tag,
                            8,
                        )?;
                        let content = value.convert_at(access, direction, column)?;
                        layout::return_object(
                            &[sigil_quote!(TypeScript { [$S(tag)]: $C(content) })?],
                            Some(output_type.clone()),
                            4,
                        )?
                    }
                }
            };
            let body = if matches!(binding, Some(WireBinding::Adjacent { .. })) {
                capture_tag(source, &disc, plan, body)?
            } else {
                body
            };
            if authored.is_some() {
                let primary = plan.authored_tag_shape(&schema.name, index, TaggedMatch::Concrete);
                let opaque = plan.authored_tag_shape(&schema.name, index, TaggedMatch::Fallback);
                if *primary != JsonShape::Never {
                    let condition = if strict {
                        // sigil-stitch numeric expressions take the exact literal token.
                        sigil_quote!(TypeScript { matches[$L(index.to_string())] })?
                    } else {
                        plan.matching_condition(
                            primary,
                            sigil_quote!(TypeScript { $N(source) })?,
                            direction,
                        )?
                    };
                    branches.push(
                        sigil_quote!(TypeScript { if ($C(condition)) { $C(body.clone()) } })?,
                    );
                }
                if *opaque != JsonShape::Never {
                    let condition = plan.matching_condition(
                        opaque,
                        sigil_quote!(TypeScript { $N(source) })?,
                        direction,
                    )?;
                    fallback.push(sigil_quote!(TypeScript { if ($C(condition)) { $C(body) } })?);
                }
            } else {
                let condition = selected_variant_condition(source, direction, union, index, plan)?;
                branches.push(sigil_quote!(TypeScript { if ($C(condition)) { $C(body) } })?);
            }
        }
        let checks = layout::array(&strict_checks, 18)?;
        let body = sigil_quote!(TypeScript {
            $if(strict) {
                const matches = $C(checks);
                if (matches.filter(Boolean).length > 1) { throw new TypeError($S(format!("Ambiguous {name} tagged union value"))); }
            }
            $C_each(branches)
            $C_each(fallback)
            throw new Error($S("Unknown variant"));
        })?;
        file = file.add_function(
            FunSpec::builder(&format!("{}{}", fn_base_name(&name), direction.suffix()))
                .visibility(Visibility::Public)
                .add_param(ParameterSpec::of(source, TypeName::primitive(input)))
                .returns(vec![output_type])
                .body(body)
                .build()?,
        );
    }
    if plan.flags().emit_type_guards {
        for guard in guards(schema, ir, union, plan)? {
            file = file.add_code(guard);
        }
    }
    file.build()
}
