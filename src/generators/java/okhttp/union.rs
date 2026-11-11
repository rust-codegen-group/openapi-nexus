//! Typed untagged Gson adapters with original-member selection boundaries.

use sigil_stitch::error::SigilStitchError;
use sigil_stitch::lang::java::Java;
use sigil_stitch::prelude::*;

use crate::generators::closed_sum::ClosedSumPlan;
use crate::generators::type_application;
use crate::generators::union_match::plan_union_members;
use crate::ir::types::{IrSchema, IrSchemaKind, IrSpec};

use super::config::UntaggedUnionStrategy;
use super::matching::MatchPlan;
use super::sigil_emit::{java_adapter_read_value, java_adapter_write_case};

pub(super) fn adapter(
    schema: &IrSchema,
    plan: &ClosedSumPlan,
    ir: &IrSpec,
    strategy: UntaggedUnionStrategy,
) -> Result<CodeBlock, SigilStitchError> {
    let raw = strategy == UntaggedUnionStrategy::Raw;
    let raw_case = &plan.cases[0].name;
    let mut matching = MatchPlan::default();
    let root = TypeName::primitive(&plan.name);
    let field = FieldSpec::builder("gson", TypeName::importable("com.google.gson", "Gson"))
        .visibility(Visibility::Private)
        .is_readonly()
        .initializer(sigil_quote!(Java { new Gson() })?.__sigil_trim_trailing_newline())
        .build()?;
    let read_body = if raw {
        sigil_quote!(Java { return new $N(&plan.name).$N(raw_case)(element); })?
    } else {
        typed_read(schema, plan, ir, strategy, &mut matching)?
    };
    let read = FunSpec::builder("read")
        .visibility(Visibility::Public)
        .annotate(AnnotationSpec::new("Override"))
        .add_param(ParameterSpec::of(
            "reader",
            TypeName::importable("com.google.gson.stream", "JsonReader"),
        ))
        .returns(vec![root.clone()])
        .throws(vec![TypeName::qualified("java.io", "IOException")])
        .body(sigil_quote!(Java {
            JsonElement element = JsonParser.parseReader(reader);
            $C(read_body)
        })?)
        .build()?;
    let write_cases = if raw {
        Vec::new()
    } else {
        plan.cases
            .iter()
            .map(|case| java_adapter_write_case(plan, case))
            .collect::<Result<Vec<_>, _>>()?
    };
    let write = FunSpec::builder("write")
        .visibility(Visibility::Public)
        .annotate(AnnotationSpec::new("Override"))
        .add_param(ParameterSpec::of(
            "writer",
            TypeName::importable("com.google.gson.stream", "JsonWriter"),
        ))
        .add_param(ParameterSpec::of("value", root.clone()))
        .returns(vec![TypeName::primitive("void")])
        .throws(vec![TypeName::qualified("java.io", "IOException")])
        .body(sigil_quote!(Java {
            if (value == null) {
                writer.nullValue();
                return;
            }
            $if(raw) {
                JsonElement element = (($N(&plan.name).$N(raw_case)) value).value0();
                if (element == null) { element = JsonNull.INSTANCE; }
                // Raw trees retain null-valued object entries even when Gson's
                // ordinary model serializer omits null fields. Use the native
                // tree adapter with both stream writers and JsonTreeWriter.
                boolean serializeNulls = writer.getSerializeNulls();
                writer.setSerializeNulls(true);
                try {
                    gson.getAdapter(JsonElement.class).write(writer, element);
                } finally {
                    writer.setSerializeNulls(serializeNulls);
                }
            } $else {
                $C_each(write_cases)
            }
        })?)
        .build()?;
    let parent = type_application(
        TypeName::importable("com.google.gson", "TypeAdapter"),
        vec![root],
    );
    let name = format!("{}JsonAdapter", plan.name);
    let mut adapter = TypeSpec::builder(&name, TypeKind::Class)
        .extends(parent)
        .add_field(field)
        .add_method(read)
        .add_method(write);
    for helper in matching.functions(ir)? {
        adapter = adapter.add_method(helper);
    }
    let declaration = adapter
        .build()?
        .emit(&Java::new())?
        .into_iter()
        .next()
        .expect("one Java adapter class declaration");
    // sigil-stitch 0.7.5 lacks a final-class modifier. Keep only that token
    // outside TypeSpec so declaration structure and imports remain semantic.
    sigil_quote!(Java { final $C(declaration) })
}

fn typed_read(
    schema: &IrSchema,
    plan: &ClosedSumPlan,
    ir: &IrSpec,
    strategy: UntaggedUnionStrategy,
    matching: &mut MatchPlan,
) -> Result<CodeBlock, SigilStitchError> {
    let IrSchemaKind::Union(union) = &schema.kind else {
        unreachable!("untagged selection owns a union schema")
    };
    let members = plan_union_members(&schema.name, union, ir);
    for member in &members {
        matching.register(&member.shape, ir);
    }
    let null = match plan.cases.iter().find(|case| case.is_null) {
        Some(case) => sigil_quote!(Java { return $N(&plan.name).$N(&case.name).INSTANCE; })?,
        None => sigil_quote!(Java { return null; })?,
    };
    let strict = strategy == UntaggedUnionStrategy::Strict;
    let ambiguous = format!(
        "ambiguous untagged union {}: multiple concrete members match",
        plan.name
    );
    let mut selection = Vec::new();
    for member in &members {
        let check = matching.condition(&member.shape, sigil_quote!(Java { element })?)?;
        let index = member.source_index.to_string();
        selection.push(if strict && !member.fallback {
            sigil_quote!(Java {
                if ($C(check)) {
                    if (selected >= 0) { throw new JsonParseException($S(&ambiguous)); }
                    selected = $L(index);
                }
            })?
        } else {
            sigil_quote!(Java {
                if (selected < 0 && $C(check)) { selected = $L(index); }
            })?
        });
    }
    let delegates = plan
        .cases
        .iter()
        .enumerate()
        .filter_map(|(index, case)| {
            case.payload.as_ref().map(|payload| {
                let read =
                    java_adapter_read_value(plan, case, payload, sigil_quote!(Java { element })?)?;
                sigil_quote!(Java {
                    if (selected == $L(index.to_string())) { $C(read) }
                })
            })
        })
        .collect::<Result<Vec<_>, SigilStitchError>>()?;
    let memo = MatchPlan::memo_type();
    sigil_quote!(Java {
        if (element.isJsonNull()) { $C(null) }
        int selected = -1;
        $if(matching.is_recursive()) {
            $T(memo.clone()) memo = new $T(memo)();
        }
        $C_each(selection)
        $C_each(delegates)
        throw new JsonParseException($S(format!("unknown {} union value", plan.name)));
    })
}
