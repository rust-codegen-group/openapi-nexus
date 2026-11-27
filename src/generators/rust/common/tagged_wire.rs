//! Native Serde codecs for inferred internally tagged union wire plans.

use std::collections::{BTreeSet, HashSet};

use heck::{ToPascalCase, ToSnakeCase};
use sigil_stitch::error::SigilStitchError;
use sigil_stitch::lang::rust::Rust;
use sigil_stitch::prelude::{CodeBlock, TypeName, sigil_quote};
use sigil_stitch::spec::{
    annotation_spec::AnnotationSpec,
    closed_sum_case_spec::ClosedSumCaseSpec,
    closed_sum_spec::ClosedSumSpec,
    field_spec::FieldSpec,
    file_spec::FileSpec,
    fun_spec::FunSpec,
    modifiers::{DeclarationContext, Visibility},
    parameter_spec::ParameterSpec,
    where_spec::GenericParamSpec,
};

use crate::generators::type_application;
use crate::generators::union_wire::{
    Strategy, TagOwner, TagRead, UnionWirePlan, UnionWirePlans, WireBinding,
};
use crate::ir::types::{
    IrJsonKind, IrSchema, IrSchemaKind, IrSpec, IrTaggedUnion, IrTypeExpr, IrWireShape,
    TaggingStyle,
};

use super::config::ExtraDeriveConfig;
use super::emit_models::{build_utoipa_one_of_impl, resolve_object, rust_model_type};
use super::wire_matching::{WireMatching, value_type};

mod nullable_payload;
use nullable_payload::{DeepNulls, Restoration};

fn needs_common_projection(plan: &UnionWirePlan<'_>) -> bool {
    plan.common_shape().is_some_and(|shape| {
        !matches!(
            shape,
            IrWireShape::Any | IrWireShape::Kind(IrJsonKind::Object)
        )
    })
}

fn adopts(union: &IrTaggedUnion, plan: &UnionWirePlan<'_>, ir: &IrSpec) -> bool {
    if !matches!(union.tagging, TaggingStyle::Internal) {
        return false;
    }
    if needs_common_projection(plan) {
        return true;
    }
    // Keep the compatible legacy representation for fixed labels. The shared
    // fact proves tag identity; a native case must also avoid duplicate owners.
    if !plan.has_distinct_fixed_tags() {
        return true;
    }
    !union.variants.iter().enumerate().all(|(index, case)| {
        match plan.binding(index) {
            Some(WireBinding::Internal {
                owner: TagOwner::Case,
                ..
            }) => true,
            Some(WireBinding::Internal {
                owner: TagOwner::Payload,
                ..
            }) => {
                // The old enum can remove an existing tag only when it inlines
                // the actual object, not a newtype alias or allOf wrapper.
                resolve_object(&case.content_type, ir).is_some()
            }
            _ => false,
        }
    })
}

pub(super) struct TaggedWirePlans<'ir> {
    shared: UnionWirePlans<'ir>,
    adopted: HashSet<&'ir str>,
    pub(super) strategy: Strategy,
}

impl<'ir> TaggedWirePlans<'ir> {
    pub(super) fn build(ir: &'ir IrSpec, strategy: Strategy) -> Self {
        let shared = UnionWirePlans::build(ir, strategy);
        let adopted = ir
            .schemas
            .iter()
            .filter_map(|(name, schema)| {
                let IrSchemaKind::TaggedUnion(union) = &schema.kind else {
                    return None;
                };
                let plan = shared.get(name)?;
                adopts(union, plan, ir).then_some(name.as_str())
            })
            .collect();
        Self {
            shared,
            adopted,
            strategy,
        }
    }

    pub(super) fn get(&self, name: &str) -> Option<&UnionWirePlan<'ir>> {
        self.adopted
            .contains(name)
            .then(|| self.shared.get(name))
            .flatten()
    }

    /// Only raw adopted unions are allowed to consume an explicit null before
    /// the enclosing model's optional field turns it into absence.
    pub(super) fn uses_raw_value(&self, mut expr: &'ir IrTypeExpr, ir: &'ir IrSpec) -> bool {
        if self.strategy != Strategy::Raw {
            return false;
        }
        let mut visited = HashSet::new();
        loop {
            match expr.representation() {
                IrTypeExpr::Nullable(inner) => expr = inner,
                IrTypeExpr::Named(name) if self.get(name).is_some() => return true,
                IrTypeExpr::Named(name) if visited.insert(name) => {
                    let Some(IrSchemaKind::Alias(inner)) =
                        ir.schemas.get(name).map(|schema| &schema.kind)
                    else {
                        return false;
                    };
                    expr = inner;
                }
                _ => return false,
            }
        }
    }
}

struct Variant<'a> {
    name: String,
    payload: TypeName,
    tag: Option<CaseTag<'a>>,
    projected_decoder: Option<(TypeName, TagRead)>,
    restore_nulls: Option<&'a IrTypeExpr>,
    required_nullable_keys: BTreeSet<&'a str>,
}

struct CaseTag<'a> {
    wire_name: &'a str,
    ty: TypeName,
    optional: bool,
    allows_whole_value: bool,
}

impl CaseTag<'_> {
    fn field_type(&self) -> TypeName {
        if self.optional {
            TypeName::optional(self.ty.clone())
        } else {
            self.ty.clone()
        }
    }
}

pub(super) fn emit(
    schema: &IrSchema,
    union: &IrTaggedUnion,
    plan: &UnionWirePlan<'_>,
    wire_plans: &TaggedWirePlans<'_>,
    extra: Option<&ExtraDeriveConfig>,
    ir: &IrSpec,
    utoipa: bool,
) -> Result<FileSpec, SigilStitchError> {
    let strategy = wire_plans.strategy;
    let name = schema.name.to_pascal_case();
    let mut file = FileSpec::builder(&format!("{}.rs", schema.name.to_snake_case()));
    let raw = strategy == Strategy::Raw;
    let mut sum = ClosedSumSpec::builder(&name)
        .visibility(Visibility::Public)
        .annotate(derives(extra, raw, utoipa));
    if let Some(description) = &schema.description {
        for line in description.lines() {
            sum = sum.doc(line);
        }
    }
    if raw {
        sum = sum.annotate(AnnotationSpec::new("serde").arg("untagged"));
        let case = ClosedSumCaseSpec::positional("Raw", vec![value_type()])?;
        file = file.add_closed_sum(sum.add_case(case).build()?);
        if utoipa {
            file = file.add_code(build_utoipa_one_of_impl(&name, &[value_type()])?);
        }
        return file.build();
    }

    let mut used = HashSet::new();
    let mut reserved_tag_types = ir
        .schemas
        .values()
        .map(|schema| schema.name.to_pascal_case())
        .collect();
    let mut variants = Vec::new();
    // The generated member models have no fields for union-level constraints.
    // Preserve that scope without manufacturing fields on reusable models.
    let needs_common_projection = needs_common_projection(plan);
    for (index, variant) in union.variants.iter().enumerate() {
        let base = variant.discriminator_value.to_pascal_case();
        let base = if base.is_empty()
            || base == "Self"
            || base.starts_with(|ch: char| ch.is_ascii_digit())
        {
            format!("Variant{index}")
        } else {
            base
        };
        let case_name = unique_name(base, &mut used);
        let Some(WireBinding::Internal { field, owner, read }) = plan.binding(index) else {
            unreachable!("inferred internal wire binding");
        };
        let domain = plan
            .tag_domain(index)
            .expect("internal inferred discriminator domain");
        let root = plan.possible_root_kinds(index);
        let finite_nonnull = domain
            .values
            .as_ref()
            .is_some_and(|values| values.iter().all(|value| !value.is_null()));
        let unit = root.object
            && crate::generators::closed_sum::is_tagged_unit_variant(ir, union, variant);
        let deep_nulls = nullable_payload::plan(&variant.content_type, ir, wire_plans);
        let payload_can_hold_tag = !needs_common_projection
            && deep_nulls != DeepNulls::Project
            && owner == TagOwner::Payload
            && read == TagRead::Keep
            && payload_preserves_tag(
                &variant.content_type,
                field,
                finite_nonnull,
                ir,
                wire_plans,
                &mut HashSet::new(),
            );
        let raw_payload = needs_common_projection
            || deep_nulls == DeepNulls::Project
            || owner == TagOwner::CaseWithPayloadProjection
            || owner == TagOwner::Payload && !payload_can_hold_tag
            || unit;
        // A tag-only shell does not decode the projected payload. Likewise,
        // multiple flattened readers can consume the same key in Serde; their
        // original allOf model is not a compatible decoder for this projection.
        let projected_decoder =
            (raw_payload && !unit && owner != TagOwner::CaseWithPayloadProjection)
                .then(|| (rust_model_type(&variant.content_type), read));
        let mut required_nullable_keys = BTreeSet::new();
        let payload = if raw_payload {
            // A projected JSON object can drop the discriminator even when the
            // original typed codec would have needed to read it. The case owns
            // the tag when native flattening or null elision would lose it.
            value_type()
        } else {
            collect_required_nullable_keys(
                &variant.content_type,
                ir,
                &mut HashSet::new(),
                &mut required_nullable_keys,
            );
            required_nullable_keys.remove(field);
            rust_model_type(&variant.content_type)
        };
        let tag = if payload_can_hold_tag || !root.object {
            None
        } else {
            let (ty, declaration) = tag_type(
                &name,
                index,
                domain.values.as_deref(),
                &mut reserved_tag_types,
            )?;
            if let Some(declaration) = declaration {
                file = file.add_closed_sum(declaration);
            }
            Some(CaseTag {
                wire_name: field,
                ty,
                optional: domain.allows_absent || root.non_object,
                allows_whole_value: root.non_object,
            })
        };
        variants.push(Variant {
            name: case_name,
            payload,
            tag,
            projected_decoder,
            restore_nulls: (!raw_payload && deep_nulls == DeepNulls::Restore)
                .then_some(&variant.content_type),
            required_nullable_keys,
        });
    }
    for variant in &variants {
        let case = if let Some(tag) = &variant.tag {
            ClosedSumCaseSpec::record(
                &variant.name,
                vec![
                    FieldSpec::of("discriminator", tag.field_type()),
                    FieldSpec::of("payload", variant.payload.clone()),
                ],
            )?
        } else {
            ClosedSumCaseSpec::positional(&variant.name, vec![variant.payload.clone()])?
        };
        sum = sum.add_case(case);
    }
    file = file.add_closed_sum(sum.build()?);

    let mut matcher = WireMatching::default();
    for shape in plan.branches().iter().chain(plan.common_shape()) {
        matcher.register(shape, ir);
    }
    let common = plan
        .common_shape()
        .map(|shape| matcher.evidence(shape, sigil_quote!(RustLang { &candidate })?))
        .transpose()?;
    let mut select = Vec::new();
    let strict = strategy == Strategy::Strict;
    let ambiguous =
        format!("ambiguous inferred tagged union {name}: multiple concrete members match");
    let unmatched = format!("value does not match any member of inferred tagged union {name}");
    for (index, branch) in plan.branches().iter().enumerate() {
        let evidence = matcher.evidence(branch, sigil_quote!(RustLang { &candidate })?)?;
        let index = index.to_string();
        let branch = sigil_quote!(RustLang {
            match $C(evidence) {
                2 => {
                    $if(strict) {
                        if selected.is_some() { return Err(serde::de::Error::custom($S(&ambiguous))); }
                    }
                    selected = Some($L(index.as_str()));
                }
                1 => { fallback.get_or_insert($L(index.as_str())); }
                _ => {}
            }
        })?;
        select.push(if strict {
            branch
        } else {
            sigil_quote!(RustLang {
                if selected.is_none() { $C(branch) }
            })?
        });
    }
    let decode = variants.iter().enumerate().map(|(index, variant)| {
        let name = &variant.name;
        let payload = &variant.payload;
        let index = index.to_string();
        if let Some(tag) = &variant.tag {
            let wire = tag.wire_name;
            let tag_type = &tag.ty;
            let optional = tag.optional;
            let validate_before = variant.projected_decoder.as_ref().and_then(|(ty, read)| (*read == TagRead::Keep).then_some(ty));
            let validate_after = variant.projected_decoder.as_ref().and_then(|(ty, read)| (*read == TagRead::Strip).then_some(ty));
            sigil_quote!(RustLang {
                Some($L(index)) => {
                    $if(validate_before.is_some()) {
                        let _: $T(validate_before.unwrap().clone()) = serde::Deserialize::deserialize(&candidate).map_err(serde::de::Error::custom)?;
                    }
                    let mut candidate = candidate;
                    let discriminator = candidate.as_object_mut().and_then(|object| object.remove($S(wire)));
                    $if(validate_after.is_some()) {
                        let _: $T(validate_after.unwrap().clone()) = serde::Deserialize::deserialize(&candidate).map_err(serde::de::Error::custom)?;
                    }
                    $if(optional) {
                        let discriminator = discriminator.map(serde_json::from_value::<$T(tag_type.clone())>)
                            .transpose().map_err(serde::de::Error::custom)?;
                    } $else {
                        let discriminator = discriminator.ok_or_else(|| serde::de::Error::missing_field($S(wire)))?;
                        let discriminator = serde_json::from_value::<$T(tag_type.clone())>(discriminator)
                            .map_err(serde::de::Error::custom)?;
                    }
                    let payload = serde_json::from_value::<$T(payload.clone())>(candidate).map_err(serde::de::Error::custom)?;
                    Ok(Self::$N(name) { discriminator, payload })
                }
            })
        } else if let Some((decoder, _)) = &variant.projected_decoder {
            // A whole-value alternative has no tag to remove before its codec.
            sigil_quote!(RustLang {
                Some($L(index)) => {
                    let _: $T(decoder.clone()) = serde::Deserialize::deserialize(&candidate).map_err(serde::de::Error::custom)?;
                    Ok(Self::$N(name)(candidate))
                }
            })
        } else {
            sigil_quote!(RustLang {
                Some($L(index)) => serde_json::from_value::<$T(payload.clone())>(candidate)
                    .map(Self::$N(name)).map_err(serde::de::Error::custom),
            })
        }
    }).collect::<Result<Vec<_>, _>>()?;
    let deserialize = sigil_quote!(RustLang {
        let candidate = <serde_json::Value as serde::Deserialize>::deserialize(deserializer)?;
        $if(matcher.uses_memo()) { let memo = &mut std::collections::HashMap::new(); }
        $if(common.is_some()) {
            if $C(common.unwrap()) == 0 { return Err(serde::de::Error::custom($S(&unmatched))); }
        }
        let mut selected = None;
        let mut fallback = None;
        $C_each(select)
        match selected.or(fallback) {
            $C_each(decode)
            _ => Err(serde::de::Error::custom($S(unmatched))),
        }
    })?;
    let deserialize = serde_method(
        "deserialize",
        "D",
        "Deserializer",
        "deserializer",
        None,
        deserialize,
    )?;
    file = file.add_code(sigil_quote!(RustLang {
        impl<'de> serde::Deserialize<'de> for $N(&name) { $C(deserialize) }
    })?);

    let mut nulls = Restoration::new(ir, wire_plans);
    let encode = variants.iter().enumerate().map(|(index, variant)| {
        let name = &variant.name;
        let source = sigil_quote!(RustLang { &value })?;
        let condition = matcher.evidence(&plan.branches()[index], source.clone())?;
        let common_check = plan.common_shape().map(|shape| matcher.evidence(shape, source)).transpose()?;
        let invalid = format!("serialized value does not match inferred tagged union branch {name}");
        let use_memo = std::iter::once(&plan.branches()[index]).chain(plan.common_shape())
            .any(WireMatching::needs_memo);
        let nullable_keys = &variant.required_nullable_keys;
        let restore_nulls = variant.restore_nulls.map(|expr| nulls.root(expr, &union.discriminator_field)).transpose()?.flatten();
        // Projection changes storage, not nested selection policy. Check the
        // assembled value before returning it, without reserializing it.
        let projected_check = variant.projected_decoder.as_ref().map(|(ty, read)| {
            let field = variant.tag.as_ref().and_then(|tag| (*read == TagRead::Strip).then_some(tag.wire_name));
            sigil_quote!(RustLang {
                $if(field.is_some()) {
                    let mut projected = value.clone();
                    if let Some(object) = projected.as_object_mut() { object.remove($S(field.unwrap())); }
                }
                let _: $T(ty.clone()) = serde::Deserialize::deserialize($if(field.is_some()) { &projected } $else { &value })
                    .map_err(serde::ser::Error::custom)?;
            })
        }).transpose()?;
        let check = sigil_quote!(RustLang {
            $if(restore_nulls.is_some()) {
                let mut value = value;
                $C(restore_nulls.unwrap())
            } $else_if(!nullable_keys.is_empty()) {
                let mut value = value;
                if let Some(object) = value.as_object_mut() {
                    $for(key in nullable_keys) {
                        object.entry($S(*key)).or_insert(serde_json::Value::Null);
                    }
                }
            }
            $if(use_memo) { let memo = &mut std::collections::HashMap::new(); }
            $if(common_check.is_some()) {
                if $C(common_check.unwrap()) == 0 { return Err(serde::ser::Error::custom($S(&invalid))); }
            }
            if $C(condition) == 0 { return Err(serde::ser::Error::custom($S(invalid))); }
            $if(projected_check.is_some()) { $C(projected_check.unwrap()) }
            value
        })?;
        if let Some(tag) = &variant.tag {
            let wire = tag.wire_name;
            let optional = tag.optional;
            let allows_whole_value = tag.allows_whole_value;
            let write_tag = sigil_quote!(RustLang {
                if let Some(existing) = object.get($S(wire)) {
                    if Some(existing) != discriminator.as_ref() {
                        return Err(serde::ser::Error::custom("conflicting inferred discriminator in payload"));
                    }
                } else if let Some(tag) = discriminator {
                    object.insert($S(wire).to_owned(), tag);
                }
            })?;
            sigil_quote!(RustLang {
                Self::$N(name) { discriminator, payload } => {
                    let mut value = serde_json::to_value(payload).map_err(serde::ser::Error::custom)?;
                    $if(!allows_whole_value) {
                        let object = value.as_object_mut().ok_or_else(|| serde::ser::Error::custom("inferred tagged payload must be an object"))?;
                    }
                    $if(optional) {
                        let discriminator = discriminator.as_ref().map(serde_json::to_value).transpose().map_err(serde::ser::Error::custom)?;
                    } $else {
                        let discriminator = Some(serde_json::to_value(discriminator).map_err(serde::ser::Error::custom)?);
                    }
                    $if(allows_whole_value) {
                        if let Some(object) = value.as_object_mut() {
                            $C(write_tag)
                        } else if discriminator.is_some() {
                            return Err(serde::ser::Error::custom("inferred discriminator requires an object payload"));
                        }
                    } $else { $C(write_tag) }
                    $C(check)
                }
            })
        } else {
            sigil_quote!(RustLang {
                Self::$N(name)(payload) => {
                    let value = serde_json::to_value(payload).map_err(serde::ser::Error::custom)?;
                    $C(check)
                }
            })
        }
    }).collect::<Result<Vec<_>, _>>()?;
    let serialize = sigil_quote!(RustLang {
        let value = match self { $C_each(encode) };
        serde::Serialize::serialize(&value, serializer)
    })?;
    let serialize = serde_method(
        "serialize",
        "S",
        "Serializer",
        "serializer",
        Some(TypeName::qualified("S", "Ok")),
        serialize,
    )?;
    file = file.add_code(sigil_quote!(RustLang {
        impl serde::Serialize for $N(&name) { $C(serialize) }
    })?);
    for function in matcher.functions(ir)?.into_iter().chain(nulls.functions()?) {
        file = file.add_function(function);
    }
    if utoipa {
        let payloads = variants
            .iter()
            .map(|variant| variant.payload.clone())
            .collect::<Vec<_>>();
        file = file.add_code(build_utoipa_one_of_impl(&name, &payloads)?);
    }
    file.build()
}

// The reusable codec writes `Option::None` as omission. At this use site,
// selection rejects a missing required key, so `None` can only mean JSON null.
// Inspect only the same-object model fields, including flattened intersections.
fn collect_required_nullable_keys<'ir>(
    expr: &'ir IrTypeExpr,
    ir: &'ir IrSpec,
    visiting: &mut HashSet<&'ir str>,
    keys: &mut BTreeSet<&'ir str>,
) {
    match expr.representation() {
        IrTypeExpr::Named(name) if visiting.insert(name) => {
            match ir.schemas.get(name).map(|schema| &schema.kind) {
                Some(IrSchemaKind::Object(object)) => keys.extend(
                    object
                        .properties
                        .iter()
                        .filter(|(_, property)| property.required && property.nullable)
                        .map(|(name, _)| name.as_str()),
                ),
                Some(IrSchemaKind::Intersection(intersection)) => {
                    for member in &intersection.members {
                        collect_required_nullable_keys(member, ir, visiting, keys);
                    }
                }
                Some(IrSchemaKind::Alias(inner)) => {
                    collect_required_nullable_keys(inner, ir, visiting, keys);
                }
                _ => {}
            }
            visiting.remove(name.as_str());
        }
        IrTypeExpr::Nullable(inner) => collect_required_nullable_keys(inner, ir, visiting, keys),
        _ => {}
    }
}

/// A direct object can omit a null tag or flatten a competing map entry.
/// A nested authored codec with the same discriminator already preserves the
/// wire state and checks its own output; it can be the sole writer unchanged.
fn payload_preserves_tag<'ir>(
    expr: &'ir IrTypeExpr,
    field: &str,
    finite_nonnull: bool,
    ir: &'ir IrSpec,
    wire_plans: &TaggedWirePlans<'_>,
    visiting: &mut HashSet<&'ir str>,
) -> bool {
    match expr.representation() {
        IrTypeExpr::Named(name) if visiting.insert(name) => {
            let safe = match ir.schemas.get(name).map(|schema| &schema.kind) {
                Some(IrSchemaKind::Object(object)) => {
                    object.additional_properties.is_none()
                        && (finite_nonnull || !object.properties.contains_key(field))
                }
                Some(IrSchemaKind::Intersection(intersection)) => {
                    intersection.members.iter().all(|member| {
                        payload_preserves_tag(
                            member,
                            field,
                            finite_nonnull,
                            ir,
                            wire_plans,
                            visiting,
                        )
                    })
                }
                Some(IrSchemaKind::Alias(inner)) => {
                    payload_preserves_tag(inner, field, finite_nonnull, ir, wire_plans, visiting)
                }
                Some(IrSchemaKind::TaggedUnion(union))
                    if matches!(union.tagging, TaggingStyle::Internal)
                        && union.discriminator_field == field =>
                {
                    wire_plans.get(name).is_some()
                        || finite_nonnull
                            && union.inferred_wire.is_some()
                            && union.variants.iter().all(|variant| {
                                resolve_object(&variant.content_type, ir)
                                    .is_some_and(|object| object.additional_properties.is_none())
                            })
                }
                _ => false,
            };
            visiting.remove(name.as_str());
            safe
        }
        IrTypeExpr::Nullable(inner) => {
            payload_preserves_tag(inner, field, finite_nonnull, ir, wire_plans, visiting)
        }
        _ => false,
    }
}

fn unique_name(base: String, used: &mut HashSet<String>) -> String {
    let mut name = base.clone();
    for suffix in 2.. {
        if used.insert(name.clone()) {
            return name;
        }
        name = format!("{base}{suffix}");
    }
    unreachable!()
}

fn tag_type(
    union: &str,
    index: usize,
    values: Option<&[serde_json::Value]>,
    reserved: &mut HashSet<String>,
) -> Result<
    (
        TypeName,
        Option<sigil_stitch::spec::closed_sum_spec::ClosedSumSpec>,
    ),
    SigilStitchError,
> {
    let Some(values) = values else {
        return Ok((value_type(), None));
    };
    if values
        .iter()
        .any(|value| !value.is_null() && !value.is_string())
    {
        return Ok((value_type(), None));
    }
    let null = values.iter().any(serde_json::Value::is_null);
    let strings: Vec<_> = values
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect();
    if strings.is_empty() {
        return Ok(if null {
            (TypeName::primitive("()"), None)
        } else {
            (value_type(), None)
        });
    }
    let name = unique_name(format!("{union}Discriminator{}", index + 1), reserved);
    let mut sum = ClosedSumSpec::builder(&name)
        .visibility(Visibility::Public)
        .annotate(AnnotationSpec::new("derive").args([
            "Debug",
            "Clone",
            "Copy",
            "PartialEq",
            "Eq",
            "Hash",
            "serde::Serialize",
            "serde::Deserialize",
        ]));
    let mut used = HashSet::new();
    for value in strings {
        let case = value.to_pascal_case();
        let base = if case.is_empty()
            || case == "Self"
            || case.starts_with(|ch: char| ch.is_ascii_digit())
        {
            "Tag".to_string()
        } else {
            case
        };
        let case = ClosedSumCaseSpec::builder(&unique_name(base, &mut used))
            .annotation(
                sigil_quote!(RustLang { #[serde(rename = $S(value))] })?
                    .__sigil_trim_trailing_newline(),
            )
            .build()?;
        sum = sum.add_case(case);
    }
    let ty = TypeName::primitive(&name);
    Ok((
        if null { TypeName::optional(ty) } else { ty },
        Some(sum.build()?),
    ))
}

fn serde_method(
    method: &str,
    param: &str,
    bound: &str,
    argument: &str,
    serialize_result: Option<TypeName>,
    body: CodeBlock,
) -> Result<CodeBlock, SigilStitchError> {
    let trait_type = TypeName::qualified("serde", bound);
    let trait_type = if serialize_result.is_some() {
        trait_type
    } else {
        // sigil-stitch TypeArgument cannot represent a lifetime separately.
        type_application(trait_type, vec![TypeName::raw("'de")])
    };
    let mut function = FunSpec::builder(method)
        .add_generic_param(GenericParamSpec::single(param)?.with_bound(trait_type)?);
    if serialize_result.is_some() {
        function = function.add_param(ParameterSpec::of("&self", TypeName::primitive("")));
    }
    function
        .add_param(ParameterSpec::of(argument, TypeName::primitive(param)))
        .returns(vec![type_application(
            TypeName::primitive("Result"),
            vec![
                serialize_result.unwrap_or_else(|| TypeName::primitive("Self")),
                TypeName::qualified(param, "Error"),
            ],
        )])
        .body(body)
        .build()?
        .emit(&Rust::new(), DeclarationContext::Member)
}

fn derives(extra: Option<&ExtraDeriveConfig>, raw: bool, utoipa: bool) -> AnnotationSpec {
    let mut names = vec!["Debug", "Clone"];
    if raw {
        names.extend(["serde::Serialize", "serde::Deserialize"]);
    }
    for derive in extra.into_iter().flat_map(|extra| &extra.derives) {
        let path = derive.strip_prefix("::").unwrap_or(derive);
        let canonical = path.strip_prefix("serde::").unwrap_or(path);
        if matches!(canonical, "Serialize" | "Deserialize")
            || names.contains(&canonical)
            || utoipa && matches!(path, "utoipa::ToSchema" | "ToSchema")
        {
            continue;
        }
        names.push(derive);
    }
    AnnotationSpec::new("derive").args(names)
}

#[cfg(test)]
mod tests {
    use sigil_stitch::assert_rendered;

    use super::*;

    #[test]
    fn serde_method_preserves_lifetime_and_associated_result_type() {
        let body = sigil_quote!(RustLang { Ok(Self::Empty) }).unwrap();
        let method = serde_method(
            "deserialize",
            "D",
            "Deserializer",
            "deserializer",
            None,
            body,
        )
        .unwrap();
        assert_rendered!(
            Rust::new(),
            width = 100,
            method,
            "fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {\n    Ok(Self::Empty)\n}\n",
        );
    }
}
