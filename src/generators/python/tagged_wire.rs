//! Python's adoption and model projection of shared inferred tagged wire plans.

use std::collections::{BTreeMap, BTreeSet};

use heck::{ToPascalCase, ToSnakeCase};
use serde::{Deserialize, Serialize};
use sigil_stitch::error::SigilStitchError;
use sigil_stitch::prelude::*;

use crate::generators::union_wire::{
    Strategy, TagOwner, TagRead, UnionWirePlan, UnionWirePlans, WireBinding,
};
use crate::ir::types::{
    IrJsonKind, IrObject, IrSchema, IrSchemaKind, IrSpec, IrTypeExpr, IrWireShape, TaggingStyle,
};

use super::emit_models::{future_annotations_header, python_field_name, type_alias_spec};
use super::value::{Direction, ValuePlan, model_dictionary_type, schema_expressions};
use super::wire_matching::WireMatching;

/// Structural conversion policy for generalized inferred internal discriminators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaggedUnionStrategy {
    #[default]
    FirstMatch,
    Strict,
    Raw,
}

impl From<TaggedUnionStrategy> for Strategy {
    fn from(policy: TaggedUnionStrategy) -> Self {
        match policy {
            TaggedUnionStrategy::FirstMatch => Self::FirstMatch,
            TaggedUnionStrategy::Strict => Self::Strict,
            TaggedUnionStrategy::Raw => Self::Raw,
        }
    }
}

pub(super) struct TaggedPlans<'ir> {
    shared: UnionWirePlans<'ir>,
    cases: BTreeMap<&'ir str, Vec<Case<'ir>>>,
    pub(super) strategy: TaggedUnionStrategy,
}

struct Case<'ir> {
    name: String,
    payload: Option<&'ir IrTypeExpr>,
    validate: Option<(&'ir IrTypeExpr, TagRead)>,
    whole: bool,
    tag: Option<CaseTag<'ir>>,
    required_nullable: Vec<&'ir str>,
}

struct CaseTag<'ir> {
    field: &'ir str,
    optional: bool,
    ty: TypeName,
    literal: bool,
}

impl<'ir> TaggedPlans<'ir> {
    pub(super) fn build(ir: &'ir IrSpec, strategy: TaggedUnionStrategy) -> Self {
        let shared = UnionWirePlans::build(ir, strategy.into());
        let adopted = ir
            .schemas
            .iter()
            .filter_map(|(name, schema)| {
                let IrSchemaKind::TaggedUnion(union) = &schema.kind else {
                    return None;
                };
                let plan = shared.get(name)?;
                (matches!(union.tagging, TaggingStyle::Internal) && !plan.has_distinct_fixed_tags())
                    .then_some(name.as_str())
            })
            .collect::<BTreeSet<_>>();
        let mut used = ir
            .schemas
            .keys()
            .map(|name| name.to_pascal_case())
            .collect::<BTreeSet<_>>();
        let cases = adopted
            .iter()
            .map(|&name| {
                let IrSchemaKind::TaggedUnion(union) = &ir.schemas[name].kind else {
                    unreachable!()
                };
                let plan = shared.get(name).expect("adopted inferred shape");
                let union_name = name.to_pascal_case();
                let cases = if strategy == TaggedUnionStrategy::Raw {
                    Vec::new()
                } else {
                    union
                        .variants
                        .iter()
                        .enumerate()
                        .map(|(index, variant)| {
                            let Some(WireBinding::Internal { field, owner, read }) =
                                plan.binding(index)
                            else {
                                unreachable!()
                            };
                            let domain = plan
                                .tag_domain(index)
                                .expect("internal discriminator domain");
                            let base = variant.discriminator_value.to_pascal_case();
                            let base = if base.is_empty() {
                                format!("{union_name}Case")
                            } else {
                                format!("{union_name}{base}Case")
                            };
                            let mut name = base.clone();
                            for suffix in 2.. {
                                if used.insert(name.clone()) {
                                    break;
                                }
                                name = format!("{base}{suffix}");
                            }
                            let expr = &variant.content_type;
                            let codec = native_payload(expr, ir, &adopted, &mut BTreeSet::new());
                            let whole = admits_non_object(&plan.branches()[index], ir, &mut BTreeSet::new())
                                && plan.common_shape().is_none_or(|shape| admits_non_object(shape, ir, &mut BTreeSet::new()));
                            let nested_null = matches!(codec, Some(NativePayload::Object(object)) if nested_omits_required_null(object, ir, &adopted));
                            let project = whole || has_common_constraints(plan) || nested_null;
                            let payload_owns = !project
                                && owner == TagOwner::Payload
                                && read == TagRead::Keep
                                && match codec {
                                    Some(NativePayload::Nested) => true,
                                    Some(NativePayload::Object(object)) => {
                                        object.properties.get(field).is_some_and(|property| {
                                            // Ordinary dataclasses omit None; they can retain a
                                            // non-null tag (including optional absence).
                                            !property.nullable
                                                && domain.values.as_ref().is_some_and(|values| {
                                                    values.iter().all(|value| !value.is_null())
                                                })
                                        })
                                    }
                                    None => false,
                                };
                            let typed = !project
                                && codec.is_some()
                                && (payload_owns
                                    || owner == TagOwner::Case && read == TagRead::Strip);
                            let required_nullable = if typed {
                                match codec {
                                    Some(NativePayload::Object(object)) => object
                                        .properties
                                        .iter()
                                        .filter(|(key, prop)| {
                                            key.as_str() != field && prop.required && prop.nullable
                                        })
                                        .map(|(key, _)| key.as_str())
                                        .collect(),
                                    _ => Vec::new(),
                                }
                            } else {
                                Vec::new()
                            };
                            let tag = (!payload_owns).then(|| {
                                let optional = whole || domain.allows_absent;
                                let (ty, literal) = tag_type(domain.values.as_deref(), optional);
                                CaseTag {
                                    field,
                                    optional,
                                    ty,
                                    literal,
                                }
                            });
                            Case {
                                name,
                                payload: typed.then_some(expr),
                                validate: (!typed && codec.is_some() && !(whole && matches!(codec, Some(NativePayload::Object(_))))).then_some((expr, read)),
                                whole,
                                tag,
                                required_nullable,
                            }
                        })
                        .collect()
                };
                (name, cases)
            })
            .collect();
        Self {
            shared,
            cases,
            strategy,
        }
    }

    pub(super) fn exports(&self) -> impl Iterator<Item = (String, String)> + '_ {
        self.cases.iter().flat_map(|(union, cases)| {
            let module = format!(".{}", union.to_snake_case());
            cases
                .iter()
                .map(move |case| (module.clone(), case.name.clone()))
        })
    }

    pub(super) fn is_empty(&self) -> bool {
        self.cases.is_empty()
    }

    pub(super) fn contains(&self, name: &str) -> bool {
        self.cases.contains_key(name)
    }

    pub(super) fn uses_literal(&self) -> bool {
        self.cases
            .values()
            .flatten()
            .any(|case| case.tag.as_ref().is_some_and(|tag| tag.literal))
    }

    fn plan(&self, name: &str) -> &UnionWirePlan<'ir> {
        self.shared.get(name).expect("adopted inferred wire plan")
    }

    pub(super) fn match_body(
        &self,
        name: &str,
        direction: Direction,
        concrete: bool,
        fallback: bool,
        matching: &mut WireMatching,
    ) -> Result<CodeBlock, SigilStitchError> {
        if self.strategy == TaggedUnionStrategy::Raw {
            return sigil_quote!(Python { return $if(concrete) { False } $else { True } });
        }
        if direction == Direction::To {
            let module = name.to_snake_case();
            let checks = self.cases[name].iter().map(|case| {
                let name = case.name.as_str();
                sigil_quote!(Python { if isinstance(value, _case_model.$N(name)) { return True } })
            }).collect::<Result<Vec<_>, _>>()?;
            return if fallback {
                sigil_quote!(Python { return False })
            } else {
                sigil_quote!(Python {
                    from . import $N(module.as_str()) as _case_model
                    $C_each(checks)
                    return False
                })
            };
        }
        let plan = self.plan(name);

        let common = matching.predicate(plan.common_shape().unwrap_or(&IrWireShape::Any));
        let branches = plan
            .branches()
            .iter()
            .map(|shape| matching.predicate(shape))
            .collect::<Vec<_>>();
        sigil_quote!(Python {
            branches = [$for(branch in &branches; separator = ", ") { $N(branch.as_str()) }]
            evidence = _tagged_support.evidence(value, $N(common), branches)
            return $if(concrete) { evidence == 2 } $else_if(fallback) { evidence == 1 } $else { evidence != 0 }
        })
    }

    pub(super) fn convert(
        &self,
        name: &str,
        values: &ValuePlan<'_>,
        direction: Direction,
        matching: &mut WireMatching,
    ) -> Result<CodeBlock, SigilStitchError> {
        if self.strategy == TaggedUnionStrategy::Raw {
            return sigil_quote!(Python { return value });
        }
        let plan = self.plan(name);
        let common = matching.predicate(plan.common_shape().unwrap_or(&IrWireShape::Any));
        let branches = plan
            .branches()
            .iter()
            .map(|shape| matching.predicate(shape))
            .collect::<Vec<_>>();
        let cases = &self.cases[name];
        let module = name.to_snake_case();
        let name = name.to_pascal_case();
        let strict = self.strategy == TaggedUnionStrategy::Strict;
        let mut imports = vec![(module.clone(), "_case_model".to_owned())];
        let codecs = cases.iter().enumerate().map(|(index, case)| {
            let class = &case.name;
            let class_ref = TypeName::associated_type(TypeName::primitive("_case_model"), None, class);
            match direction {
                Direction::From => {
                    let index = index.to_string();
                    let source = match &case.tag {
                        Some(tag) if case.whole => sigil_quote!(Python { _tagged_support.project_whole(value, $S(tag.field)) })?,
                        Some(tag) => sigil_quote!(Python { _tagged_support.project(data, $S(tag.field)) })?,
                        None => sigil_quote!(Python { data })?,
                    };
                    let payload = if let Some(expr) = case.payload {
                        let IrTypeExpr::Named(target) = expr else { unreachable!("native payload is named") };
                        let model = target.to_snake_case();
                        let symbol = if model == module {
                            "_case_model".to_owned()
                        } else {
                            let alias = format!("_payload_model_{index}");
                            imports.push((model, alias.clone()));
                            alias
                        };
                        let ty = TypeName::associated_type(TypeName::primitive(&symbol), None, &target.to_pascal_case());
                        cast(ty, values.invoke(expr, source, direction)?)?
                    } else { source };
                    let tag = case.tag.as_ref().map(|tag| {
                        let field = tag.field;
                        let raw = if case.whole {
                            sigil_quote!(Python { _tagged_support.read_whole_tag(value, $S(field)) })?
                        } else if tag.optional {
                            sigil_quote!(Python { data.get($S(field), _tagged_support.AbsentDiscriminator.ABSENT) })?
                        } else {
                            sigil_quote!(Python { data[$S(field)] })?
                        };
                        cast(tag.ty.clone(), raw)
                    }).transpose()?;
                    let validation = case.validate.map(|(expr, read)| {
                        let input = sigil_quote!(Python {
                            $if(read == TagRead::Keep) { value } $else { payload }
                        })?;
                        values.invoke(expr, input, direction)
                    }).transpose()?;
                    sigil_quote!(Python {
                        if selected == $L(index) {
                            payload = $C(payload)
                            $if(validation.is_some()) { $C(validation.unwrap()) }
                            $if(tag.is_some()) {
                                discriminator = $C(tag.unwrap())
                                return $T(class_ref)(payload, discriminator)
                            } $else {
                                return $T(class_ref)(payload)
                            }
                        }
                    })
                }
                Direction::To => {
                    let branch = &branches[index];
                    let payload = sigil_quote!(Python { value.payload })?;
                    let output = if let Some(expr) = case.payload { values.invoke(expr, payload, direction)? } else { payload };
                    let tag = case.tag.as_ref().map(|tag| tag.field);
                    let check = format!("{name}.{class}");
                    let required_nullable = &case.required_nullable;
                    let whole = case.whole;
                    let finish = sigil_quote!(Python { _tagged_support.finish(result, $N(common.as_str()), $N(branch.as_str()), $S(check)) })?;
                    let write = if let Some((expr, read)) = case.validate {
                        // Check the assembled output using the same selected codec as on read.
                        let input = match read {
                            TagRead::Keep => sigil_quote!(Python { result })?,
                            TagRead::Strip => {
                                let field = tag.expect("projected case owns its discriminator");
                                sigil_quote!(Python { _tagged_support.project_whole(result, $S(field)) })?
                            }
                        };
                        let validation = values.invoke(expr, input, Direction::From)?;
                        sigil_quote!(Python {
                            $C(finish)
                            $C(validation)
                            return result
                        })?
                    } else {
                        sigil_quote!(Python { return $C(finish) })?
                    };
                    sigil_quote!(Python {
                        if isinstance(value, $T(class_ref)) {
                            payload = $C(output)
                            $if(whole) {
                                result = _tagged_support.write_whole_tag(payload, $S(tag.expect("case owns the whole-value tag")), value.discriminator)
                            } $else {
                                if not _union_support.is_dict(payload) {
                                    raise ValueError("Inferred tagged union payload must be an object")
                                }
                                result = dict(payload)
                                $if(tag.is_some()) { _tagged_support.write_tag(result, $S(tag.unwrap()), value.discriminator) }
                                $for(key in required_nullable) { result.setdefault($S(*key), None) }
                            }
                            $C(write)
                        }
                    })
                }
            }
        }).collect::<Result<Vec<_>, SigilStitchError>>()?;
        let invalid = format!("Invalid case for inferred tagged union {name}");
        let mut import_block = CodeBlock::builder();
        for (module, alias) in &imports {
            import_block.add_code(
                sigil_quote!(Python { from . import $N(module.as_str()) as $N(alias.as_str()) })?,
            );
        }
        import_block.add_line();
        let import_block = import_block.build()?;
        sigil_quote!(Python {
            $C(import_block)
            $if(direction == Direction::From) {
                branches = [$for(branch in &branches; separator = ", ") { $N(branch.as_str()) }]
                selected = _tagged_support.select(value, $N(common.as_str()), branches, $if(strict) { True } $else { False }, $S(&name))
                data = _union_support.as_dict(value)
            }
            $C_each(codecs)
            raise ValueError($S(invalid))
        })
    }
}

/// Only affects the public projection; emitted evidence checks decide actual
/// eligibility. A conservative allowance does not add a candidate.
fn admits_non_object<'ir>(
    shape: &'ir IrWireShape,
    ir: &'ir IrSpec,
    seen: &mut BTreeSet<&'ir str>,
) -> bool {
    match shape {
        IrWireShape::Never | IrWireShape::Kind(IrJsonKind::Object) | IrWireShape::Object(_) => {
            false
        }
        IrWireShape::Finite(values) => values.iter().any(|value| !value.is_object()),
        IrWireShape::AllOf(parts) => parts.iter().all(|part| admits_non_object(part, ir, seen)),
        IrWireShape::OneOf(parts) | IrWireShape::AnyOf(parts) => {
            parts.iter().any(|part| admits_non_object(part, ir, seen))
        }
        IrWireShape::Reference(name) if seen.insert(name) => {
            let admits = ir
                .wire_schemas
                .get(name)
                .is_some_and(|shape| admits_non_object(shape, ir, seen));
            seen.remove(name.as_str());
            admits
        }
        IrWireShape::Reference(_) => false,
        IrWireShape::Any | IrWireShape::Kind(_) | IrWireShape::Array(_) => true,
    }
}

fn nested_omits_required_null(root: &IrObject, ir: &IrSpec, adopted: &BTreeSet<&str>) -> bool {
    fn visit<'ir>(
        ty: &'ir IrTypeExpr,
        ir: &'ir IrSpec,
        adopted: &BTreeSet<&str>,
        seen: &mut BTreeSet<&'ir str>,
    ) -> bool {
        match ty.representation() {
            IrTypeExpr::Array(child) | IrTypeExpr::Map(child) | IrTypeExpr::Nullable(child) => visit(child, ir, adopted, seen),
            IrTypeExpr::Union(members) => members.iter().any(|member| visit(member, ir, adopted, seen)),
            // A nested authored codec owns its own preservation and validation.
            IrTypeExpr::Named(name) if !adopted.contains(name.as_str()) && seen.insert(name) => {
                ir.schemas.get(name).is_some_and(|schema| {
                    matches!(&schema.kind, IrSchemaKind::Object(object) if object.properties.values().any(|prop| prop.required && prop.nullable))
                        || schema_expressions(&schema.kind).into_iter().any(|child| visit(child, ir, adopted, seen))
                })
            }
            _ => false,
        }
    }
    // Root required nullable properties are restored by the typed case codec;
    // ordinary descendants do not get to use that restoration.
    let mut seen = BTreeSet::new();
    root.properties
        .values()
        .map(|property| &property.type_expr)
        .chain(root.additional_properties.iter())
        .any(|child| visit(child, ir, adopted, &mut seen))
}

fn has_common_constraints(plan: &UnionWirePlan<'_>) -> bool {
    plan.common_shape().is_some_and(|shape| {
        !matches!(
            shape,
            IrWireShape::Any | IrWireShape::Kind(IrJsonKind::Object)
        )
    })
}

#[derive(Clone, Copy)]
enum NativePayload<'ir> {
    Object(&'ir IrObject),
    Nested,
}

fn native_payload<'ir>(
    expr: &'ir IrTypeExpr,
    ir: &'ir IrSpec,
    adopted: &BTreeSet<&str>,
    visiting: &mut BTreeSet<&'ir str>,
) -> Option<NativePayload<'ir>> {
    let IrTypeExpr::Named(name) = expr else {
        return None;
    };
    if !visiting.insert(name) {
        return None;
    }
    match &ir.schemas.get(name)?.kind {
        IrSchemaKind::Object(object)
            if object.additional_properties.is_some() || !object.properties.is_empty() =>
        {
            // A collision in Python's field-name normalization is not a
            // faithful projection of the authored object at this use site.
            let unique = object
                .properties
                .values()
                .map(|prop| python_field_name(&prop.name))
                .collect::<BTreeSet<_>>();
            (unique.len() == object.properties.len()).then_some(NativePayload::Object(object))
        }
        IrSchemaKind::Alias(inner) => native_payload(inner, ir, adopted, visiting),
        IrSchemaKind::TaggedUnion(_) if adopted.contains(name.as_str()) => {
            Some(NativePayload::Nested)
        }
        _ => None,
    }
}

fn absent_discriminator() -> TypeName {
    TypeName::importable("..runtime._tagged_support", "AbsentDiscriminator")
}

fn tag_type(values: Option<&[serde_json::Value]>, optional: bool) -> (TypeName, bool) {
    let Some(values) = values
        .filter(|values| !values.is_empty() && values.iter().all(|v| v.is_null() || v.is_string()))
    else {
        return (TypeName::primitive("object"), false);
    };
    let mut members = values
        .iter()
        .map(|value| match value.as_str() {
            Some(name) => TypeName::string_literal(name.to_owned()),
            None => TypeName::primitive("None"),
        })
        .collect::<Vec<_>>();
    let literal = values.iter().any(|value| value.is_string());
    if optional {
        members.push(absent_discriminator());
    }
    let ty = if members.len() == 1 {
        members.remove(0)
    } else {
        TypeName::union(members)
    };
    (ty, literal)
}

fn cast(ty: TypeName, value: CodeBlock) -> Result<CodeBlock, SigilStitchError> {
    if ty == TypeName::primitive("object") {
        return Ok(value);
    }
    sigil_quote!(Python { $T(TypeName::importable("typing", "cast"))($T(ty), $C(value)) })
}

pub(super) fn emit(
    schema: &IrSchema,
    values: &ValuePlan<'_>,
) -> Result<FileSpec, SigilStitchError> {
    let plans = &values.tagged;
    let raw = plans.strategy == TaggedUnionStrategy::Raw;
    let name = schema.name.to_pascal_case();
    let snake = schema.name.to_snake_case();
    let cases = &plans.cases[schema.name.as_str()];
    let whole = cases.iter().any(|case| case.whole);
    let mut file = FileSpec::builder_with("model.py", sigil_stitch::lang::python::Python::new())
        .header(future_annotations_header())
        .add_import(ImportSpec::named(".", "_union"));
    if let Some(description) = &schema.description {
        for line in description.lines() {
            file = file.add_code(sigil_quote!(Python { $comment(line) })?);
        }
    }
    if cases
        .iter()
        .any(|case| case.tag.as_ref().is_some_and(|tag| tag.literal))
    {
        file = file.add_import(ImportSpec::named("typing", "Literal"));
    }
    for case in cases {
        let payload = if case.whole {
            TypeName::primitive("object")
        } else {
            case.payload.map_or_else(model_dictionary_type, |expr| {
                values.model_type(expr, &schema.name)
            })
        };
        let mut ty = TypeSpec::builder(&case.name, TypeKind::Class)
            .annotate(AnnotationSpec::importable(TypeName::importable(
                "dataclasses",
                "dataclass",
            )))
            .add_field(FieldSpec::of("payload", payload));
        if let Some(tag) = &case.tag {
            let mut field = FieldSpec::builder("discriminator", tag.ty.clone());
            if tag.optional {
                field = field.initializer(
                    sigil_quote!(Python { $T(absent_discriminator()).ABSENT })?
                        .__sigil_trim_trailing_newline(),
                );
            }
            ty = ty.add_field(field.build()?);
        }
        file = file.add_type(ty.build()?);
    }
    let target = if raw {
        vec![TypeName::primitive("object")]
    } else {
        cases
            .iter()
            .map(|case| TypeName::primitive(&case.name))
            .collect()
    };
    file = file.add_type(type_alias_spec(&name, target));
    for direction in [Direction::From, Direction::To] {
        let alias = TypeName::primitive(&name);
        let (input, result) = match direction {
            Direction::From => (TypeName::primitive("object"), alias),
            Direction::To => (
                alias,
                if raw || whole {
                    TypeName::primitive("object")
                } else {
                    model_dictionary_type()
                },
            ),
        };
        let converted = values.call(
            &IrTypeExpr::Named(schema.name.clone()),
            sigil_quote!(Python { value })?,
            direction,
        );
        let converted = if raw {
            converted
        } else {
            cast(result.clone(), converted)?
        };
        file = file.add_function(
            FunSpec::builder(&format!("{snake}_{}", direction.suffix()))
                .add_param(ParameterSpec::of("value", input))
                .returns(vec![result])
                .body(sigil_quote!(Python { return $C(converted) })?)
                .build()?,
        );
    }
    file.build()
}
