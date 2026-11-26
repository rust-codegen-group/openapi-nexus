//! Native Gson codec for the Kotlin representation of an authored internal tag.

use sigil_stitch::lang::kotlin::Kotlin;
use sigil_stitch::prelude::*;

use crate::generators::closed_sum::ClosedSumPlan;
use crate::generators::jvm::{TaggedCase, TypedWire};
use crate::generators::type_application;
use crate::generators::union_match::JsonShape;
use crate::generators::union_wire::TagRead;
use crate::ir::types::IrSpec;

use super::matching::MatchPlan;
use super::sigil_emit::kotlin_type_name;

pub(super) fn adapter(
    sum: &ClosedSumPlan,
    wire: &TypedWire<'_>,
    ir: &IrSpec,
    adapter_name: &str,
) -> Result<CodeBlock, SigilStitchError> {
    let mut matching = MatchPlan::default();
    let selection = wire.selection();
    for shape in wire
        .common
        .iter()
        .chain(wire.cases.iter().map(|case| &case.shape))
        .chain(selection.iter().map(|(_, shape, _)| shape))
    {
        matching.register(shape, ir);
    }
    let root = TypeName::primitive(&sum.name);
    let read = FunSpec::builder("read")
        .is_override()
        .add_param(ParameterSpec::of(
            "reader",
            TypeName::importable("com.google.gson.stream", "JsonReader"),
        ))
        .returns(vec![root.clone()])
        .body(read_body(sum, wire, &matching, &selection)?)
        .build()?;
    let branches = sum.cases.iter().zip(&wire.cases).map(|(case, authored)| {
        let case_ty = TypeName::associated_type(root.clone(), None, &case.name);
        let payload = payload_type(case, authored);
        let check = write_case(wire, authored, &matching, &case.name)?;
        sigil_quote!(Kotlin {
            is $T(case_ty) -> {
                                $if(authored.projected) {
                    val candidate = value.value0.deepCopy()
                } $else {
                    val candidate = gson.toJsonTree(value.value0, object : TypeToken<$T(payload)>() {}.type)
                }
                $C(check)
                treeGson.toJson(candidate, writer)
            }
        })
    }).collect::<Result<Vec<_>, _>>()?;
    let write = FunSpec::builder("write")
        .is_override()
        .add_param(ParameterSpec::of(
            "writer",
            TypeName::importable("com.google.gson.stream", "JsonWriter"),
        ))
        .add_param(ParameterSpec::of("value", TypeName::optional(root.clone())))
        .body(sigil_quote!(Kotlin {
            if (value == null) { writer.nullValue(); return }
            when (value) { $C_each(branches) }
        })?)
        .build()?;
    let gson_type = TypeName::importable("com.google.gson", "Gson");
    let gson = FieldSpec::builder("gson", gson_type.clone())
        .visibility(Visibility::Private)
        .is_readonly()
        .initializer(sigil_quote!(Kotlin { Gson() })?.__sigil_trim_trailing_newline())
        .build()?;
    let tree_gson = FieldSpec::builder("treeGson", gson_type)
        .visibility(Visibility::Private)
        .is_readonly()
        .initializer(
            sigil_quote!(Kotlin { gson.newBuilder().serializeNulls().create() })?
                .__sigil_trim_trailing_newline(),
        )
        .build()?;
    let mut adapter = TypeSpec::builder(adapter_name, TypeKind::Class)
        .visibility(Visibility::Private)
        .extends(type_application(
            TypeName::importable("com.google.gson", "TypeAdapter"),
            vec![root],
        ))
        .add_field(gson)
        .add_field(tree_gson)
        .add_method(read)
        .add_method(write);
    for helper in matching.functions(ir)? {
        adapter = adapter.add_method(helper);
    }
    let declarations = adapter.build()?.emit(&Kotlin::new())?;
    sigil_quote!(Kotlin { $C_each(declarations) })
}

fn read_body(
    sum: &ClosedSumPlan,
    wire: &TypedWire<'_>,
    matching: &MatchPlan,
    selection: &[(usize, JsonShape, bool)],
) -> Result<CodeBlock, SigilStitchError> {
    let strict = wire.strict;
    let ambiguous = format!(
        "ambiguous inferred tagged union {}: multiple concrete members match",
        sum.name
    );
    let unmatched = format!(
        "value does not match any member of inferred tagged union {}",
        sum.name
    );
    let common = wire
        .common
        .as_ref()
        .map(|shape| matching.condition(shape, sigil_quote!(Kotlin { candidate })?))
        .transpose()?;
    let branches = selection
        .iter()
        .map(|(index, shape, opaque)| {
            let check = matching.condition(shape, sigil_quote!(Kotlin { candidate })?)?;
            if strict && !*opaque {
                sigil_quote!(Kotlin {
                    if ($C(check)) {
                        if (selected >= 0) { throw JsonParseException($S(&ambiguous)) }
                        selected = $L(index.to_string())
                    }
                })
            } else {
                sigil_quote!(Kotlin {
                    if (selected < 0 && $C(check)) { selected = $L(index.to_string()) }
                })
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    let decode = sum.cases.iter().zip(&wire.cases).enumerate().map(|(index, (case, authored))| {
        let codec = |ty: TypeName, source: &str| sigil_quote!(Kotlin { gson.fromJson($N(source), object : TypeToken<$T(ty)>() {}.type) });
        let native_read = authored.projected_reader.as_ref().map(|expr| {
            let source = if authored.read == TagRead::Keep { "candidate" } else { "input" };
            let payload = kotlin_type_name(expr);
            sigil_quote!(Kotlin { gson.fromJson<$T(payload.clone())>($N(source), object : TypeToken<$T(payload)>() {}.type) })
        }).transpose()?;
        let payload = if authored.projected {
            if authored.projects_object {
                sigil_quote!(Kotlin { input.asJsonObject })?
            } else {
                sigil_quote!(Kotlin { input })?
            }
        } else {
            codec(payload_type(case, authored), "input")?
        };
        let case_ty = TypeName::associated_type(TypeName::primitive(&sum.name), None, &case.name);
        let field = wire.field;
        sigil_quote!(Kotlin {
            if (selected == $L(index.to_string())) {
                $if(authored.case_owns_tag) {
                    val discriminator = if (candidate.isJsonObject) { candidate.asJsonObject.get($S(field)) } else { null }
                }
                $if(authored.strip) {
                    val input = if (candidate.isJsonObject && candidate.asJsonObject.has($S(field))) {
                        candidate.asJsonObject.deepCopy().apply { remove($S(field)) }
                    } else { candidate }
                } $else { val input = candidate }
                $if(native_read.is_some()) { $C(native_read.unwrap()) }
                return $T(case_ty)($C(payload)$if(authored.case_owns_tag) { , discriminator })
            }
        })
    }).collect::<Result<Vec<_>, _>>()?;
    let memo = MatchPlan::memo_type();
    let recursive = matching.is_recursive();
    sigil_quote!(Kotlin {
        val candidate = JsonParser.parseReader(reader)
        $if(recursive) { val memo = $T(memo)() }
        $if(common.is_some()) {
            if (!$C(common.unwrap())) { throw JsonParseException($S(&unmatched)) }
        }
        var selected = -1
        $C_each(branches)
        $C_each(decode)
        throw JsonParseException($S(&unmatched))
    })
}

fn payload_type(
    case: &crate::generators::closed_sum::ClosedSumCasePlan,
    authored: &TaggedCase,
) -> TypeName {
    authored
        .projected_type()
        .unwrap_or_else(|| kotlin_type_name(case.payload.as_ref().expect("tagged case payload")))
}

fn write_case(
    wire: &TypedWire<'_>,
    case: &TaggedCase,
    matching: &MatchPlan,
    name: &str,
) -> Result<CodeBlock, SigilStitchError> {
    let field = wire.field;
    let condition = matching.condition(&case.shape, sigil_quote!(Kotlin { candidate })?)?;
    let common = wire
        .common
        .as_ref()
        .map(|shape| matching.condition(shape, sigil_quote!(Kotlin { candidate })?))
        .transpose()?;
    let recursive = matching.is_recursive();
    let memo = MatchPlan::memo_type();
    let invalid = format!("serialized value does not match inferred tagged union branch {name}");
    let required_nullable = &case.required_nullable;
    sigil_quote!(Kotlin {
        $if(case.case_owns_tag) {
            if (!candidate.isJsonObject && value.discriminator != null) { throw JsonParseException("inferred discriminator requires an object") }
        }
        $if(!required_nullable.is_empty() || case.case_owns_tag) {
            if (candidate.isJsonObject) {
                val output = candidate.asJsonObject
                $for(key in required_nullable) {
                    if (!output.has($S(key))) { output.add($S(key), JsonNull.INSTANCE) }
                }
                $if(case.case_owns_tag) {
                    val tag = value.discriminator
                    if (output.has($S(field))) {
                        if (output.get($S(field)) != tag) { throw JsonParseException("conflicting inferred discriminator in payload") }
                    } else if (tag != null) {
                        output.add($S(field), tag)
                    }
                }
            }
        }
        $if(recursive) { val memo = $T(memo)() }
        $if(common.is_some()) {
            if (!$C(common.unwrap())) { throw JsonParseException($S(&invalid)) }
        }
        if (!$C(condition)) { throw JsonParseException($S(&invalid)) }
    })
}
