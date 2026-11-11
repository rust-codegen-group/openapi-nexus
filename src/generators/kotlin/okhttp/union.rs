//! Original-member selection before a single ordinary Kotlin Gson delegation.

use sigil_stitch::lang::kotlin::Kotlin;
use sigil_stitch::prelude::*;

use crate::generators::closed_sum::ClosedSumPlan;
use crate::generators::type_application;
use crate::generators::union_match::plan_union_members;
use crate::ir::types::{IrSchema, IrSchemaKind, IrSpec};

use super::config::UntaggedUnionStrategy;
use super::matching::MatchPlan;
use super::sigil_emit::{kotlin_adapter_read_value, kotlin_type_name};

pub(super) fn adapter(
    schema: &IrSchema,
    plan: &ClosedSumPlan,
    ir: &IrSpec,
    strategy: UntaggedUnionStrategy,
) -> Result<CodeBlock, SigilStitchError> {
    let raw = strategy == UntaggedUnionStrategy::Raw;
    let mut matching = MatchPlan::default();
    let root = TypeName::primitive(&plan.name);
    let read_body = if raw {
        let case = &plan.cases[0].name;
        sigil_quote!(Kotlin { return $N(&plan.name).$N(case)(element) })?
    } else {
        typed_read(schema, plan, ir, strategy, &mut matching)?
    };
    let read = FunSpec::builder("read")
        .is_override()
        .add_param(ParameterSpec::of(
            "reader",
            TypeName::importable("com.google.gson.stream", "JsonReader"),
        ))
        .returns(vec![TypeName::optional(root.clone())])
        .body(sigil_quote!(Kotlin {
            val element = JsonParser.parseReader(reader)
            $C(read_body)
        })?)
        .build()?;
    let cases =
        if raw {
            Vec::new()
        } else {
            plan.cases
            .iter()
            .map(|case| {
                let body = if let Some(payload) = &case.payload {
                    let payload = kotlin_type_name(payload);
                    // Kotlin sealed subclasses do not inherit the root's Gson
                    // annotation. Delegate through the declared member type, not
                    // reflection over its runtime Variant* wrapper.
                    sigil_quote!(Kotlin {
                        gson.toJson(value.value0, object : TypeToken<$T(payload)>() {}.type, writer)
                    })?
                } else { sigil_quote!(Kotlin { writer.nullValue() })? };
                let case_type = TypeName::associated_type(root.clone(), None, &case.name);
                sigil_quote!(Kotlin { is $T(case_type) -> { $C(body) } })
            })
            .collect::<Result<Vec<_>, _>>()?
        };
    let write_body = if raw {
        let case_type = TypeName::associated_type(root.clone(), None, &plan.cases[0].name);
        sigil_quote!(Kotlin {
            val serializeNulls = writer.serializeNulls
            writer.serializeNulls = true
            try { gson.getAdapter(JsonElement::class.java).write(writer, (value as $T(case_type)).value0) }
            finally { writer.serializeNulls = serializeNulls }
        })?
    } else {
        sigil_quote!(Kotlin { when (value) { $C_each(cases) } })?
    };
    let write = FunSpec::builder("write")
        .is_override()
        .add_param(ParameterSpec::of(
            "writer",
            TypeName::importable("com.google.gson.stream", "JsonWriter"),
        ))
        .add_param(ParameterSpec::of("value", TypeName::optional(root.clone())))
        .body(sigil_quote!(Kotlin {
            if (value == null) { writer.nullValue(); return }
            $C(write_body)
        })?)
        .build()?;
    let field = FieldSpec::builder("gson", TypeName::importable("com.google.gson", "Gson"))
        .visibility(Visibility::Private)
        .is_readonly()
        .initializer(sigil_quote!(Kotlin { Gson() })?.__sigil_trim_trailing_newline())
        .build()?;
    let mut adapter = TypeSpec::builder(&format!("{}JsonAdapter", plan.name), TypeKind::Class)
        .visibility(Visibility::Private)
        .extends(type_application(
            TypeName::importable("com.google.gson", "TypeAdapter"),
            vec![root],
        ))
        .add_field(field)
        .add_method(read)
        .add_method(write);
    for helper in matching.functions(ir)? {
        adapter = adapter.add_method(helper);
    }
    let declarations = adapter.build()?.emit(&Kotlin::new())?;
    sigil_quote!(Kotlin { $C_each(declarations) })
}

fn typed_read(
    schema: &IrSchema,
    plan: &ClosedSumPlan,
    ir: &IrSpec,
    strategy: UntaggedUnionStrategy,
    matching: &mut MatchPlan,
) -> Result<CodeBlock, SigilStitchError> {
    let IrSchemaKind::Union(union) = &schema.kind else {
        unreachable!("untagged union schema")
    };
    let members = plan_union_members(&schema.name, union, ir);
    for member in &members {
        matching.register(&member.shape, ir);
    }
    let null = match plan.cases.iter().find(|case| case.is_null) {
        Some(case) => sigil_quote!(Kotlin { return $N(&plan.name).$N(&case.name) })?,
        None => sigil_quote!(Kotlin { return null })?,
    };
    let strict = strategy == UntaggedUnionStrategy::Strict;
    let mut selection = Vec::new();
    for member in &members {
        let check = matching.condition(&member.shape, sigil_quote!(Kotlin { element })?)?;
        let index = member.source_index.to_string();
        selection.push(if strict && !member.fallback {
            sigil_quote!(Kotlin {
                if ($C(check)) {
                    if (selected >= 0) { throw JsonParseException($S(format!("ambiguous untagged union {}: multiple concrete members match", plan.name))) }
                    selected = $L(index)
                }
            })?
        } else {
            sigil_quote!(Kotlin { if (selected < 0 && $C(check)) { selected = $L(index) } })?
        });
    }
    let delegates = plan
        .cases
        .iter()
        .enumerate()
        .filter_map(|(index, case)| {
            case.payload.as_ref().map(|payload| {
                let read = kotlin_adapter_read_value(
                    plan,
                    case,
                    payload,
                    sigil_quote!(Kotlin { element })?,
                )?;
                sigil_quote!(Kotlin { if (selected == $L(index.to_string())) { return $C(read) } })
            })
        })
        .collect::<Result<Vec<_>, SigilStitchError>>()?;
    let memo = MatchPlan::memo_type();
    sigil_quote!(Kotlin {
        if (element.isJsonNull) { $C(null) }
        $if(matching.is_recursive()) { val memo = $T(memo)() }
        var selected = -1
        $C_each(selection)
        $C_each(delegates)
        throw JsonParseException($S(format!("unknown {} union value", plan.name)))
    })
}
