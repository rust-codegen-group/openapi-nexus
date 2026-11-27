//! Encoding/json codecs for generalized inferred internal Go unions.

use std::collections::{BTreeSet, HashSet};

use heck::ToPascalCase;
use sigil_stitch::lang::go::Go;
use sigil_stitch::prelude::*;

use crate::generators::union_wire::{
    TagOwner, TagRead, UnionWirePlan, UnionWirePlans, WireBinding,
};
use crate::ir::types::{
    IrJsonKind, IrSchema, IrSchemaKind, IrSpec, IrTaggedUnion, IrTypeExpr, IrWireShape,
    TaggingStyle,
};

use super::config::TaggedUnionStrategy;
use super::sigil_emit::{go_type_name, package_header};
use super::wire_matching::{WireMatching, path_type, shape_name};

pub(super) struct TaggedWirePlans<'ir> {
    ir: &'ir IrSpec,
    pub(super) strategy: TaggedUnionStrategy,
    shared: UnionWirePlans<'ir>,
}

impl<'ir> TaggedWirePlans<'ir> {
    pub(super) fn new(ir: &'ir IrSpec, strategy: TaggedUnionStrategy) -> Self {
        Self {
            ir,
            strategy,
            shared: UnionWirePlans::build(ir, strategy.into()),
        }
    }

    pub(super) fn get(&self, name: &str) -> Option<&UnionWirePlan<'ir>> {
        if self.strategy == TaggedUnionStrategy::Dynamic {
            return None;
        }
        let IrSchemaKind::TaggedUnion(union) = &self.ir.schemas.get(name)?.kind else {
            return None;
        };
        if !matches!(union.tagging, TaggingStyle::Internal) {
            return None;
        }
        let plan = self.shared.get(name)?;
        (!plan.has_distinct_fixed_tags()).then_some(plan)
    }

    pub(super) fn is_raw_field(&self, expr: &'ir IrTypeExpr) -> bool {
        if self.strategy != TaggedUnionStrategy::Raw {
            return false;
        }
        let mut seen = HashSet::new();
        let mut expr = expr.representation();
        while let IrTypeExpr::Named(name) = expr {
            if !seen.insert(name.as_str()) {
                break;
            }
            if self.get(name).is_some() {
                return true;
            }
            let Some(IrSchemaKind::Alias(inner)) =
                self.ir.schemas.get(name).map(|schema| &schema.kind)
            else {
                break;
            };
            expr = inner.representation();
        }
        false
    }
}

struct Case<'ir> {
    field: String,
    payload: TypeName,
    authored: String,
    case_owns_tag: bool,
    native_object_owns_tag: bool,
    check_native_codec: Option<(TypeName, TagRead)>,
    required_nullable: BTreeSet<&'ir str>,
}

fn json_raw() -> TypeName {
    TypeName::importable_type("encoding/json", "RawMessage")
}

fn bytes_type() -> TypeName {
    TypeName::slice(TypeName::primitive("byte"))
}

pub(super) fn emit<'ir>(
    schema: &'ir IrSchema,
    union: &'ir IrTaggedUnion,
    wire: &'ir UnionWirePlan<'ir>,
    plans: &TaggedWirePlans<'ir>,
    matching: &mut WireMatching<'ir>,
) -> Result<String, SigilStitchError> {
    let name = schema.name.to_pascal_case();
    if plans.strategy == TaggedUnionStrategy::Raw {
        let raw = json_raw();
        let doc = format!("{name} preserves the original JSON value without selecting a branch.");
        let alias = sigil_quote!(Go {
            $comment(doc)
            type $N(&name) = $T(raw)
        })?;
        return FileSpec::builder_with("tagged_union.go", Go::new())
            .header(package_header()?)
            .add_code(alias)
            .build()?
            .render(100);
    }
    let field = &union.discriminator_field;
    let mut used = BTreeSet::from([
        "Tag".to_owned(),
        "MarshalJSON".to_owned(),
        "UnmarshalJSON".to_owned(),
    ]);
    let common = wire
        .common_shape()
        .filter(|shape| !matches!(shape, IrWireShape::Any));
    let common_id = common.map(|shape| matching.register(shape, plans.ir));
    let common_fields =
        common.is_some_and(|shape| !matches!(shape, IrWireShape::Kind(IrJsonKind::Object)));
    let cases = union
        .variants
        .iter()
        .enumerate()
        .map(|(index, variant)| {
            let native = native_shell(&variant.content_type, plans.ir);
            let Some(WireBinding::Internal { owner, read, .. }) = wire.binding(index) else {
                unreachable!("adopted internal union")
            };
            let domain = wire
                .tag_domain(index)
                .expect("adopted internal tag has a domain");
            let nested_codec = if let Some(IrSchemaKind::TaggedUnion(nested)) = native {
                named_shell(&variant.content_type, plans.ir)
                    .filter(|name| plans.get(name).is_some())
                    .map(|_| nested)
            } else {
                None
            };
            let native_owns = owner == TagOwner::Payload
                && match native {
                    Some(IrSchemaKind::Object(object)) => {
                        object.additional_properties.is_none()
                            && domain.values.as_ref().is_some_and(|values| {
                                values.iter().all(serde_json::Value::is_string)
                            })
                    }
                    Some(IrSchemaKind::TaggedUnion(_)) => {
                        nested_codec.is_some_and(|nested| nested.discriminator_field == *field)
                    }
                    _ => false,
                };
            // The projected use-site payload must not pretend the original codec can
            // retain competing readers, an optional null tag, or envelope fields.
            let nested_nullable = has_nested_required_nullable(&variant.content_type, plans.ir);
            let raw = common_fields
                || (read == TagRead::Keep && !native_owns)
                || nested_nullable
                || match variant.content_type.representation() {
                    IrTypeExpr::Any | IrTypeExpr::Union(_) => true,
                    IrTypeExpr::Named(_) => matches!(native, None | Some(IrSchemaKind::Union(_))),
                    _ => false,
                };
            let required_nullable = if raw {
                BTreeSet::new()
            } else {
                required_nullable_keys(&variant.content_type, plans.ir)
            };
            let stem = variant.discriminator_value.to_pascal_case();
            let stem = if stem.starts_with(|ch: char| ch.is_ascii_alphabetic()) {
                stem
            } else {
                format!("Variant{}", index + 1)
            };
            let field_name = if used.insert(stem.clone()) {
                stem
            } else {
                (2..)
                    .map(|suffix| format!("{stem}{suffix}"))
                    .find(|candidate| used.insert(candidate.clone()))
                    .expect("case name space")
            };
            Case {
                field: field_name,
                payload: if raw {
                    json_raw()
                } else {
                    go_type_name(&variant.content_type)
                },
                authored: shape_name(matching.register(&wire.branches()[index], plans.ir)),
                case_owns_tag: raw || !native_owns,
                native_object_owns_tag: !raw
                    && native_owns
                    && matches!(native, Some(IrSchemaKind::Object(_))),
                check_native_codec: (raw
                    && (nested_codec.is_some()
                        || ((common_fields || nested_nullable)
                            && matches!(native, Some(IrSchemaKind::Object(_))))))
                .then(|| (go_type_name(&variant.content_type), read)),
                required_nullable: required_nullable
                    .into_iter()
                    .filter(|key| *key != field)
                    .collect(),
            }
        })
        .collect::<Vec<_>>();
    let has_tag = cases.iter().any(|case| case.case_owns_tag);
    let root = TypeName::primitive(&name);
    let mut declaration = TypeSpec::builder(&name, TypeKind::Struct).doc(&format!(
        "{name} holds one selected branch. Set exactly one member before marshaling."
    ));
    if has_tag {
        declaration = declaration.add_field(
            FieldSpec::builder("Tag", json_raw())
                .doc("Case-owned discriminator: nil means absent; JSON null is a present value.")
                .build()?,
        );
    }
    for case in &cases {
        declaration = declaration.add_field(
            FieldSpec::builder(&case.field, TypeName::pointer(case.payload.clone())).build()?,
        );
    }
    let selection_name = format!("nexusTaggedSelect{name}");
    let selection = FunSpec::builder(&selection_name)
        .add_param(ParameterSpec::of("value", TypeName::primitive("any")))
        .returns(vec![
            TypeName::primitive("int"),
            TypeName::primitive("error"),
        ])
        .body(select_body(&name, common_id, &cases, plans.strategy)?)
        .build()?;
    let marshal = FunSpec::builder("MarshalJSON")
        .receiver(ParameterSpec::of("u", root.clone()))
        .returns(vec![bytes_type(), TypeName::primitive("error")])
        .body(marshal_body(&name, field, common_id, &cases, has_tag)?)
        .build()?;
    let unmarshal = FunSpec::builder("UnmarshalJSON")
        .receiver(ParameterSpec::of("u", TypeName::pointer(root.clone())))
        .add_param(ParameterSpec::of("data", bytes_type()))
        .returns(vec![TypeName::primitive("error")])
        .body(unmarshal_body(&root, &selection_name, field, &cases)?)
        .build()?;
    FileSpec::builder_with("tagged_union.go", Go::new())
        .header(package_header()?)
        .add_import(ImportSpec::named("encoding/json", "json"))
        .add_import(ImportSpec::named("fmt", "fmt"))
        .add_type(declaration.build()?)
        .add_function(selection)
        .add_function(marshal)
        .add_function(unmarshal)
        .build()?
        .render(100)
}

fn named_shell<'ir>(mut expr: &'ir IrTypeExpr, ir: &'ir IrSpec) -> Option<&'ir str> {
    let mut seen = HashSet::new();
    while let IrTypeExpr::Named(name) = expr.representation() {
        if !seen.insert(name.as_str()) {
            return None;
        }
        let schema = ir.schemas.get(name)?;
        match &schema.kind {
            IrSchemaKind::Alias(inner) => expr = inner,
            _ => return Some(name),
        }
    }
    None
}

fn native_shell<'ir>(expr: &'ir IrTypeExpr, ir: &'ir IrSpec) -> Option<&'ir IrSchemaKind> {
    ir.schemas
        .get(named_shell(expr, ir)?)
        .map(|schema| &schema.kind)
}

fn required_nullable_keys<'ir>(expr: &'ir IrTypeExpr, ir: &'ir IrSpec) -> BTreeSet<&'ir str> {
    let Some(IrSchemaKind::Object(object)) = native_shell(expr, ir) else {
        return BTreeSet::new();
    };
    object
        .properties
        .iter()
        .filter_map(|(key, property)| {
            (property.required && property.nullable).then_some(key.as_str())
        })
        .collect()
}

/// Ordinary Go objects omit required nullable fields when their pointer is nil.
/// Restore root keys on write; deeper keys need use-site projection so reusable
/// standalone models and nested tagged codecs keep their own JSON behavior.
fn has_nested_required_nullable(expr: &IrTypeExpr, ir: &IrSpec) -> bool {
    fn visit<'ir>(
        expr: &'ir IrTypeExpr,
        at_root: bool,
        ir: &'ir IrSpec,
        seen: &mut HashSet<(&'ir str, bool)>,
    ) -> bool {
        match expr.representation() {
            IrTypeExpr::Named(name) if seen.insert((name, at_root)) => ir
                .schemas
                .get(name)
                .is_some_and(|schema| match &schema.kind {
                    IrSchemaKind::Object(object) => {
                        object.properties.values().any(|property| {
                            (!at_root && property.required && property.nullable)
                                || visit(&property.type_expr, false, ir, seen)
                        }) || object
                            .additional_properties
                            .as_ref()
                            .is_some_and(|values| visit(values, false, ir, seen))
                    }
                    IrSchemaKind::Alias(expr) => visit(expr, at_root, ir, seen),
                    IrSchemaKind::Intersection(intersection) => intersection
                        .members
                        .iter()
                        .any(|member| visit(member, false, ir, seen)),
                    _ => false,
                }),
            IrTypeExpr::Nullable(inner) => visit(inner, at_root, ir, seen),
            IrTypeExpr::Array(inner) | IrTypeExpr::Map(inner) => visit(inner, false, ir, seen),
            _ => false,
        }
    }
    visit(expr, true, ir, &mut HashSet::new())
}

fn select_body(
    name: &str,
    common: Option<usize>,
    cases: &[Case<'_>],
    strategy: TaggedUnionStrategy,
) -> Result<CodeBlock, SigilStitchError> {
    let path = path_type();
    let common = common.map(shape_name);
    let no_match = format!("value does not match any member of inferred tagged union {name}");
    let ambiguous =
        format!("ambiguous inferred tagged union {name}: multiple concrete members match");
    let strict = strategy == TaggedUnionStrategy::Strict;
    sigil_quote!(Go {
        path := make($T(path))
        $if(common.is_some()) {
            if $N(common.as_ref().unwrap())(value, path, 0) == nexusTaggedNoMatch {
                return -1, fmt.Errorf($S(&no_match))
            }
        }
        fallback := -1
        $if(strict) { selected := -1 }
        $for((index, case) in cases.iter().enumerate()) {
            switch $N(&case.authored)(value, path, 0) {
                case nexusTaggedConcrete:
                    $if(strict) {
                        if selected != -1 { return -1, fmt.Errorf($S(&ambiguous)) }
                        selected = $L(index.to_string())
                    } $else {
                        return $L(index.to_string()), nil
                    }
                case nexusTaggedOpaque:
                    if fallback == -1 { fallback = $L(index.to_string()) }
            }
        }
        $if(strict) { if selected != -1 { return selected, nil } }
        if fallback != -1 { return fallback, nil }
        return -1, fmt.Errorf($S(no_match))
    })
}

fn unmarshal_body(
    root: &TypeName,
    selector: &str,
    field: &str,
    cases: &[Case<'_>],
) -> Result<CodeBlock, SigilStitchError> {
    let case_bodies = cases.iter().enumerate().map(|(index, case)| {
        let field_name = &case.field;
        let payload = &case.payload;
        let nested_input = case.check_native_codec.as_ref().map(|(_, read)| match read {
            TagRead::Keep => "data",
            TagRead::Strip => "projected",
        });
        sigil_quote!(Go {
            case $L(index.to_string()):
                $if(case.case_owns_tag) {
                    projected, tag, err := nexusTaggedProject(data, $S(field), true)
                    if err != nil { return err }
                    next.Tag = tag
                } $else_if(case.native_object_owns_tag) {
                    projected, _, err := nexusTaggedProject(data, $S(field), false)
                    if err != nil { return err }
                }
                $if(case.check_native_codec.is_some()) {
                    $comment("Keep the selected native codec's errors even when this case stores projected JSON.")
                    var checked $T(case.check_native_codec.as_ref().unwrap().0.clone())
                    if err := json.Unmarshal($N(nested_input.unwrap()), &checked); err != nil { return err }
                }
                next.$N(field_name) = new($T(payload.clone()))
                $if(case.case_owns_tag || case.native_object_owns_tag) {
                    if err := json.Unmarshal(projected, next.$N(field_name)); err != nil { return err }
                } $else {
                    if err := json.Unmarshal(data, next.$N(field_name)); err != nil { return err }
                }
        })
    }).collect::<Result<Vec<_>, _>>()?;
    sigil_quote!(Go {
        value, err := nexusTaggedDecode(data)
        if err != nil { return err }
        selected, err := $N(selector)(value)
        if err != nil { return err }
        var next $T(root.clone())
        switch selected {
            $C_each(case_bodies)
        }
        *u = next
        return nil
    })
}

fn marshal_body(
    name: &str,
    field: &str,
    common: Option<usize>,
    cases: &[Case<'_>],
    has_tag: bool,
) -> Result<CodeBlock, SigilStitchError> {
    let common = common.map(shape_name);
    let path = path_type();
    let invalid_selection = format!("inferred tagged union {name} requires exactly one branch");
    let invalid_value =
        format!("serialized value does not match its member of inferred tagged union {name}");
    let statements = cases.iter().enumerate().map(|(index, case)| {
        let member = &case.field;
        let matches = &case.authored;
        let add_nulls = !case.required_nullable.is_empty();
        let native_check = case.check_native_codec.as_ref().map(|(payload, read)| {
            let strip = *read == TagRead::Strip;
            let source = if strip { "projected" } else { "data" };
            sigil_quote!(Go {
                $comment("Check the selected native codec with the assembled wire value, as on read.")
                $if(strip) {
                    projected, _, err := nexusTaggedProject(data, $S(field), true)
                    if err != nil { return nil, err }
                }
                var checked $T(payload.clone())
                if err := json.Unmarshal($N(source), &checked); err != nil { return nil, err }
            })
        }).transpose()?;
        sigil_quote!(Go {
            case $L(index.to_string()):
                $if(has_tag && !case.case_owns_tag) {
                    if u.Tag != nil { return nil, fmt.Errorf("selected payload owns its discriminator") }
                }
                data, err := json.Marshal(u.$N(member))
                if err != nil { return nil, err }
                $if(case.case_owns_tag) {
                    data, err = nexusTaggedAssemble(data, $S(field), u.Tag, true $for(key in &case.required_nullable) { , $S(key) })
                    if err != nil { return nil, err }
                } $else_if(add_nulls) {
                    data, err = nexusTaggedAssemble(data, $S(field), nil, false $for(key in &case.required_nullable) { , $S(key) })
                    if err != nil { return nil, err }
                }
                value, err := nexusTaggedDecode(data)
                if err != nil { return nil, err }
                path := make($T(path.clone()))
                $if(common.is_some()) {
                    if $N(common.as_ref().unwrap())(value, path, 0) == nexusTaggedNoMatch {
                        return nil, fmt.Errorf($S(&invalid_value))
                    }
                }
                if $N(matches)(value, path, 0) == nexusTaggedNoMatch {
                    return nil, fmt.Errorf($S(&invalid_value))
                }
                $if(native_check.is_some()) { $C(native_check.unwrap()) }
                return data, nil
        })
    }).collect::<Result<Vec<_>, _>>()?;
    sigil_quote!(Go {
        selected := -1
        $for((index, case) in cases.iter().enumerate()) {
            if u.$N(&case.field) != nil {
                if selected != -1 { return nil, fmt.Errorf($S(&invalid_selection)) }
                selected = $L(index.to_string())
            }
        }
        switch selected {
            $C_each(statements)
        }
        return nil, fmt.Errorf($S(invalid_selection))
    })
}
