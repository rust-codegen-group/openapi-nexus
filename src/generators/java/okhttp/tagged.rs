//! Native Gson codec for the Java representation of an authored internal tag.

use sigil_stitch::prelude::*;

use crate::generators::closed_sum::ClosedSumPlan;
use crate::generators::jvm::{TaggedCase, TypedWire};
use crate::generators::type_application;
use crate::generators::union_wire::TagRead;
use crate::ir::types::IrSpec;

use super::matching::MatchPlan;
use super::sigil_emit::{java_adapter_tree_gson, java_type_name};

pub(super) fn adapter(
    sum: &ClosedSumPlan,
    wire: &TypedWire<'_>,
    ir: &IrSpec,
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
    let read = FunSpec::builder("read")
        .visibility(Visibility::Public)
        .annotate(AnnotationSpec::new("Override"))
        .add_param(ParameterSpec::of(
            "reader",
            TypeName::importable("com.google.gson.stream", "JsonReader"),
        ))
        .returns(vec![TypeName::primitive(&sum.name)])
        .throws(vec![TypeName::qualified("java.io", "IOException")])
        .body(read_body(sum, wire, &matching, &selection)?)
        .build()?;
    let branches = sum.cases.iter().zip(&wire.cases).map(|(case, authored)| {
        let case_ty = TypeName::associated_type(TypeName::primitive(&sum.name), None, &case.name);
        let payload = payload_type(case, authored);
        let check = write_case(wire, authored, &matching, &case.name)?;
        sigil_quote!(Java {
            if (value instanceof $T(case_ty) selected) {
                                $if(authored.projected) {
                    JsonElement candidate = selected.value0().deepCopy();
                } $else {
                    JsonElement candidate = gson.toJsonTree(selected.value0(), new TypeToken<$T(payload)>() {}.getType());
                }
                $C(check)
                treeGson.toJson(candidate, writer);
                return;
            }
        })
    }).collect::<Result<Vec<_>, _>>()?;
    let write = FunSpec::builder("write")
        .visibility(Visibility::Public)
        .annotate(AnnotationSpec::new("Override"))
        .add_param(ParameterSpec::of(
            "writer",
            TypeName::importable("com.google.gson.stream", "JsonWriter"),
        ))
        .add_param(ParameterSpec::of("value", TypeName::primitive(&sum.name)))
        .returns(vec![TypeName::primitive("void")])
        .throws(vec![TypeName::qualified("java.io", "IOException")])
        .body(sigil_quote!(Java {
            if (value == null) { writer.nullValue(); return; }
            $C_each(branches)
            throw new JsonParseException("unknown inferred tagged union variant");
        })?)
        .build()?;
    let gson = FieldSpec::builder("gson", TypeName::importable("com.google.gson", "Gson"))
        .visibility(Visibility::Private)
        .is_readonly()
        .initializer(sigil_quote!(Java { new Gson() })?.__sigil_trim_trailing_newline())
        .build()?;
    let mut adapter = TypeSpec::builder(&format!("{}JsonAdapter", sum.name), TypeKind::Class)
        .extends(type_application(
            TypeName::importable("com.google.gson", "TypeAdapter"),
            vec![TypeName::primitive(&sum.name)],
        ))
        .add_field(gson)
        .add_field(java_adapter_tree_gson()?)
        .add_method(read)
        .add_method(write);
    for helper in matching.functions(ir)? {
        adapter = adapter.add_method(helper);
    }
    super::union::final_adapter(adapter.build()?)
}

fn read_body(
    sum: &ClosedSumPlan,
    wire: &TypedWire<'_>,
    matching: &MatchPlan,
    selection: &[(usize, crate::generators::union_match::JsonShape, bool)],
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
        .map(|shape| matching.condition(shape, sigil_quote!(Java { candidate })?))
        .transpose()?;
    let branches = selection
        .iter()
        .map(|(index, shape, opaque)| {
            let check = matching.condition(shape, sigil_quote!(Java { candidate })?)?;
            if strict && !*opaque {
                sigil_quote!(Java {
                    if ($C(check)) {
                        if (selected >= 0) { throw new JsonParseException($S(&ambiguous)); }
                        selected = $L(index.to_string());
                    }
                })
            } else {
                sigil_quote!(Java {
                    if (selected < 0 && $C(check)) { selected = $L(index.to_string()); }
                })
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    let decode = sum.cases.iter().zip(&wire.cases).enumerate().map(|(index, (case, authored))| {
        let payload = payload_type(case, authored);
        let case_ty = TypeName::associated_type(TypeName::primitive(&sum.name), None, &case.name);
        let codec = |ty: TypeName, source: &str| {
            let token = type_application(TypeName::importable("com.google.gson.reflect", "TypeToken"), vec![ty]);
            sigil_quote!(Java { gson.fromJson($N(source), new $T(token)() {}.getType()) })
        };
        let native_read = authored.projected_reader.as_ref().map(|expr| {
            let source = if authored.read == TagRead::Keep { "candidate" } else { "input" };
            let decode = codec(java_type_name(expr, true), source)?;
            sigil_quote!(Java { $C(decode); })
        }).transpose()?;
        let payload = if authored.projected {
            if authored.projects_object {
                sigil_quote!(Java { input.getAsJsonObject() })?
            } else {
                sigil_quote!(Java { input })?
            }
        } else {
            codec(payload, "input")?
        };
        let field = wire.field;
        // All branches have already been selected on the untouched tree.
        sigil_quote!(Java {
            if (selected == $L(index.to_string())) {
                $if(authored.case_owns_tag) {
                    JsonElement discriminator = candidate.isJsonObject() ? candidate.getAsJsonObject().get($S(field)) : null;
                }
                JsonElement input = candidate;
                $if(authored.strip) {
                    if (candidate.isJsonObject() && candidate.getAsJsonObject().has($S(field))) {
                        input = candidate.deepCopy();
                        input.getAsJsonObject().remove($S(field));
                    }
                }
                $if(native_read.is_some()) { $C(native_read.unwrap()) }
                return new $T(case_ty)($C(payload)$if(authored.case_owns_tag) { , discriminator });
            }
        })
    }).collect::<Result<Vec<_>, _>>()?;
    let memo = MatchPlan::memo_type();
    let recursive = matching.is_recursive();
    sigil_quote!(Java {
        JsonElement candidate = JsonParser.parseReader(reader);
        $if(recursive) { $T(memo) memo = new java.util.IdentityHashMap<>(); }
        $if(common.is_some()) {
            if (!$C(common.unwrap())) { throw new JsonParseException($S(&unmatched)); }
        }
        int selected = -1;
        $C_each(branches)
        $C_each(decode)
        throw new JsonParseException($S(&unmatched));
    })
}

fn payload_type(
    case: &crate::generators::closed_sum::ClosedSumCasePlan,
    authored: &TaggedCase,
) -> TypeName {
    authored.projected_type().unwrap_or_else(|| {
        java_type_name(case.payload.as_ref().expect("tagged case payload"), true)
    })
}

fn write_case(
    wire: &TypedWire<'_>,
    case: &TaggedCase,
    matching: &MatchPlan,
    name: &str,
) -> Result<CodeBlock, SigilStitchError> {
    let field = wire.field;
    let condition = matching.condition(&case.shape, sigil_quote!(Java { candidate })?)?;
    let common = wire
        .common
        .as_ref()
        .map(|shape| matching.condition(shape, sigil_quote!(Java { candidate })?))
        .transpose()?;
    let recursive = matching.is_recursive();
    let memo = MatchPlan::memo_type();
    let invalid = format!("serialized value does not match inferred tagged union branch {name}");
    let required_nullable = &case.required_nullable;
    sigil_quote!(Java {
        $if(case.case_owns_tag) {
            if (!candidate.isJsonObject() && selected.discriminator() != null) { throw new JsonParseException("inferred discriminator requires an object"); }
        }
        $if(!required_nullable.is_empty() || case.case_owns_tag) {
            if (candidate.isJsonObject()) {
                JsonObject object = candidate.getAsJsonObject();
                $for(key in required_nullable) {
                    if (!object.has($S(key))) { object.add($S(key), JsonNull.INSTANCE); }
                }
                $if(case.case_owns_tag) {
                    JsonElement tag = selected.discriminator();
                    if (object.has($S(field))) {
                        if (!object.get($S(field)).equals(tag)) { throw new JsonParseException("conflicting inferred discriminator in payload"); }
                    } else if (tag != null) {
                        object.add($S(field), tag);
                    }
                }
            }
        }
        $if(recursive) { $T(memo) memo = new java.util.IdentityHashMap<>(); }
        $if(common.is_some()) {
            if (!$C(common.unwrap())) { throw new JsonParseException($S(&invalid)); }
        }
        if (!$C(condition)) { throw new JsonParseException($S(&invalid)); }
    })
}
