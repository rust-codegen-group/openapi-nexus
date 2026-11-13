//! Sigil-stitch emit for IR schemas.
//!
//! Dispatches on `IrSchemaKind` and produces a `FileSpec` per
//! schema. Each model file carries a `@generated` header and the declaration
//! (interface / type alias / union / etc.).
//!
//! Coverage:
//! - `Object` — `export interface`
//! - `Enum` — string-literal union alias (string/integer/number/mixed values)
//! - `Alias` — `export type X = Y;`
//! - `Union` — `export type X = A | B | C [| null];` (untagged oneOf/anyOf)
//! - `Intersection` — `export type X = A & B & C;` (allOf)
//! - `TaggedUnion` — discriminated union across Internal / Adjacent / External
//!   tagging styles, each variant narrows on the discriminator literal.

use crate::codegen::traits::file_writer::FileInfo;
use crate::generators::closed_sum::is_tag_only_variant;
use crate::generators::type_application;
use crate::generators::{ArtifactKind, GeneratorError};
use crate::ir::types::{
    IrEnum, IrEnumValueType, IrIntersection, IrObject, IrPrimitive, IrProperty, IrSchema,
    IrSchemaKind, IrSpec, IrTaggedUnion, IrTaggedVariant, IrTypeExpr, IrUnion, TaggingStyle,
};
use heck::{ToLowerCamelCase, ToPascalCase};
use sigil_stitch::code_block::CodeBlock;
use sigil_stitch::error::SigilStitchError;
use sigil_stitch::lang::RendererLang;
use sigil_stitch::lang::typescript::TypeScript;
use sigil_stitch::prelude::sigil_quote;
use sigil_stitch::spec::field_spec::FieldSpec;
use sigil_stitch::spec::file_spec::FileSpec;
use sigil_stitch::spec::fun_spec::FunSpec;
use sigil_stitch::spec::modifiers::{TypeKind, Visibility};
use sigil_stitch::spec::parameter_spec::ParameterSpec;
use sigil_stitch::spec::type_spec::TypeSpec;
use sigil_stitch::type_name::TypeName;
use std::collections::HashSet;

use super::config::UntaggedUnionStrategy;
use super::plan::{
    Direction as JsonDirection, Scope, TypeScriptPlan,
    representation_is_unknown as type_expr_collapses_to_unknown, union_type as union_typename,
};

/// Flags controlling optional TS emissions (const objects, type guards).
#[derive(Debug, Clone, Copy, Default)]
pub struct EmitFlags {
    pub emit_enum_constants: bool,
    pub emit_type_guards: bool,
    pub property_naming_camel_case: bool,
    pub untagged_union_strategy: UntaggedUnionStrategy,
}

/// Convert a PascalCase name to its lowerCamelCase function base name.
/// e.g., "ContainerImage" → "containerImage"
pub(super) fn fn_base_name(pascal: &str) -> String {
    format!("{}{}", pascal[..1].to_lowercase(), &pascal[1..])
}

/// Return the value-export names a schema contributes to the barrel.
///
/// - Objects with `property_naming_camel_case`: `nameFromJSON` + `nameToJSON`.
/// - Enums with `emit_enum_constants`: the type name itself (the const object).
/// - Tagged unions with `emit_type_guards`: one `is{Variant}` per contentful variant.
/// - Everything else: empty.
pub(super) fn value_exports_for_schema(
    schema: &IrSchema,
    flags: EmitFlags,
    plan: &TypeScriptPlan<'_>,
    ir: &IrSpec,
) -> Vec<String> {
    match &schema.kind {
        IrSchemaKind::Object(_) if flags.property_naming_camel_case => {
            let pascal = schema.name.to_pascal_case();
            let base = fn_base_name(&pascal);
            vec![format!("{base}FromJSON"), format!("{base}ToJSON")]
        }
        IrSchemaKind::TaggedUnion(tu)
            if flags.property_naming_camel_case && !tu.variants.is_empty() =>
        {
            let pascal = schema.name.to_pascal_case();
            let base = fn_base_name(&pascal);
            let mut exports = vec![format!("{base}FromJSON"), format!("{base}ToJSON")];
            if flags.emit_type_guards {
                exports.extend(
                    tu.variants
                        .iter()
                        .filter(|v| {
                            !is_tag_only_variant(
                                ir,
                                &tu.discriminator_field,
                                &v.discriminator_value,
                                &v.content_type,
                            ) && !is_unspecified_variant(
                                &v.discriminator_value,
                                &tu.discriminator_field,
                                &v.content_type,
                            )
                        })
                        .map(variant_guard_name),
                );
            }
            exports
        }
        IrSchemaKind::Alias(_) | IrSchemaKind::Intersection(_)
            if plan.has_converter(&schema.name) =>
        {
            let pascal = schema.name.to_pascal_case();
            let base = fn_base_name(&pascal);
            vec![format!("{base}FromJSON"), format!("{base}ToJSON")]
        }
        IrSchemaKind::Union(_) if plan.has_converter(&schema.name) => {
            let pascal = schema.name.to_pascal_case();
            let base = fn_base_name(&pascal);
            vec![format!("{base}FromJSON"), format!("{base}ToJSON")]
        }
        IrSchemaKind::Enum(_) if flags.emit_enum_constants => {
            vec![schema.name.to_pascal_case()]
        }
        IrSchemaKind::TaggedUnion(tu) if flags.emit_type_guards => tu
            .variants
            .iter()
            .filter(|v| {
                !is_tag_only_variant(
                    ir,
                    &tu.discriminator_field,
                    &v.discriminator_value,
                    &v.content_type,
                ) && !is_unspecified_variant(
                    &v.discriminator_value,
                    &tu.discriminator_field,
                    &v.content_type,
                )
            })
            .map(variant_guard_name)
            .collect(),
        _ => vec![],
    }
}

/// Return extra type-export names for the barrel (beyond the primary type name).
///
/// Objects in camelCase mode also export `Name$Wire`.
pub(super) fn extra_type_exports_for_schema(
    schema: &IrSchema,
    flags: EmitFlags,
    plan: &TypeScriptPlan<'_>,
) -> Vec<String> {
    match &schema.kind {
        IrSchemaKind::Object(_) if flags.property_naming_camel_case => {
            vec![format!("{}$Wire", schema.name.to_pascal_case())]
        }
        IrSchemaKind::TaggedUnion(tu)
            if flags.property_naming_camel_case && !tu.variants.is_empty() =>
        {
            vec![format!("{}$Wire", schema.name.to_pascal_case())]
        }
        IrSchemaKind::Alias(_) | IrSchemaKind::Intersection(_)
            if plan.has_wire_type(&schema.name) =>
        {
            vec![format!("{}$Wire", schema.name.to_pascal_case())]
        }
        IrSchemaKind::Union(_) if plan.has_wire_type(&schema.name) => {
            vec![format!("{}$Wire", schema.name.to_pascal_case())]
        }
        _ => vec![],
    }
}

/// Emit a TypeScript model file for an IR schema. Dispatches on `schema.kind`.
pub(super) fn emit_model_file(
    schema: &IrSchema,
    ir: &IrSpec,
    flags: EmitFlags,
    plan: &TypeScriptPlan<'_>,
    unknown_aliases: &HashSet<String>,
    ts: &TypeScript,
) -> Result<Option<FileSpec>, SigilStitchError> {
    if let IrSchemaKind::Union(union) = &schema.kind
        && flags.property_naming_camel_case
        && plan.has_converter(&schema.name)
    {
        return emit_union_file_camel_case(
            schema,
            union,
            plan,
            unknown_aliases,
            flags.untagged_union_strategy,
            ts,
        )
        .map(Some);
    }
    match &schema.kind {
        IrSchemaKind::Object(obj) => {
            emit_object_file(schema, obj, flags, plan, unknown_aliases, ts).map(Some)
        }
        IrSchemaKind::Enum(en) => Ok(emit_enum_file_from(schema, en, flags, ts)),
        IrSchemaKind::Alias(expr) => emit_alias_file(schema, expr, plan, ts).map(Some),
        IrSchemaKind::Union(u) => emit_union_file(schema, u, plan, ts).map(Some),
        IrSchemaKind::Intersection(i) => {
            emit_intersection_file(schema, ir, i, flags, plan, unknown_aliases, ts)
        }
        IrSchemaKind::TaggedUnion(tu) => {
            emit_tagged_union_file(schema, ir, tu, flags, plan, unknown_aliases, ts)
        }
    }
}

/// Back-compat alias used by the enum prototype test. Prefer `emit_model_file`.
pub fn emit_enum_file(schema: &IrSchema, ts: &TypeScript) -> Option<FileSpec> {
    let IrSchemaKind::Enum(en) = &schema.kind else {
        return None;
    };
    emit_enum_file_from(schema, en, EmitFlags::default(), ts)
}

fn emit_object_file(
    schema: &IrSchema,
    obj: &IrObject,
    flags: EmitFlags,
    plan: &TypeScriptPlan<'_>,
    unknown_aliases: &HashSet<String>,
    ts: &TypeScript,
) -> Result<FileSpec, SigilStitchError> {
    let name = schema.name.to_pascal_case();

    if flags.property_naming_camel_case {
        return emit_object_file_camel_case(schema, obj, &name, plan, unknown_aliases, ts);
    }

    let mut tb = TypeSpec::builder(&name, TypeKind::Interface).visibility(Visibility::Public);
    if let Some(doc) = &schema.description {
        tb = tb.doc(doc);
    }

    for (_json_name, prop) in &obj.properties {
        tb = tb.add_field(build_field(prop, &schema.name, plan, unknown_aliases)?);
    }
    if let Some(dictionary) =
        additional_properties_super_type(obj, &schema.name, false, plan, unknown_aliases)
    {
        tb = tb.extends(dictionary);
    }

    let filename = format!("{}.ts", name);
    FileSpec::builder_with(&filename, ts.clone())
        .add_type(tb.build()?)
        .build()
}

/// Emit an object file with dual types: `Name$Wire` (wire format) + `Name`
/// (camelCase ergonomic) plus `nameFromJSON` / `nameToJSON` converters.
fn emit_object_file_camel_case(
    schema: &IrSchema,
    obj: &IrObject,
    name: &str,
    plan: &TypeScriptPlan<'_>,
    unknown_aliases: &HashSet<String>,
    ts: &TypeScript,
) -> Result<FileSpec, SigilStitchError> {
    let wire_name = format!("{}$Wire", name);
    let filename = format!("{}.ts", name);

    // --- $Wire interface (original wire-format property names) ---
    let mut wire_tb =
        TypeSpec::builder(&wire_name, TypeKind::Interface).visibility(Visibility::Public);
    if let Some(doc) = &schema.description {
        wire_tb = wire_tb.doc(doc);
    }
    for (_json_name, prop) in &obj.properties {
        wire_tb = wire_tb.add_field(build_field_wire(prop, &schema.name, plan, unknown_aliases)?);
    }
    if let Some(dictionary) =
        additional_properties_super_type(obj, &schema.name, true, plan, unknown_aliases)
    {
        wire_tb = wire_tb.extends(dictionary);
    }

    // --- Ergonomic interface (camelCase property names) ---
    let mut ergo_tb = TypeSpec::builder(name, TypeKind::Interface).visibility(Visibility::Public);
    if let Some(doc) = &schema.description {
        ergo_tb = ergo_tb.doc(doc);
    }
    for (_json_name, prop) in &obj.properties {
        ergo_tb = ergo_tb.add_field(build_field_camel(
            prop,
            &schema.name,
            plan,
            unknown_aliases,
        )?);
    }
    if let Some(dictionary) =
        additional_properties_super_type(obj, &schema.name, false, plan, unknown_aliases)
    {
        ergo_tb = ergo_tb.extends(dictionary);
    }

    // --- fromJSON / toJSON functions ---
    let from_json = build_json_converter(
        name,
        &wire_name,
        &schema.name,
        obj,
        plan,
        unknown_aliases,
        JsonDirection::FromWire,
    )?;
    let to_json = build_json_converter(
        name,
        &wire_name,
        &schema.name,
        obj,
        plan,
        unknown_aliases,
        JsonDirection::ToWire,
    )?;

    let mut fb = FileSpec::builder_with(&filename, ts.clone());
    fb = fb.add_type(wire_tb.build()?);
    fb = fb.add_type(ergo_tb.build()?);

    fb = fb.add_function(from_json);
    fb = fb.add_function(to_json);
    fb.build()
}

/// A string index also covers the declared properties in TypeScript. Include
/// their types (and optional undefined) instead of making valid mixed schemas
/// uncompilable. Readonly<Record<...>> expresses the index structurally without
/// pretending an index signature is an ordinary named field.
fn additional_properties_super_type(
    obj: &IrObject,
    current_schema: &str,
    wire: bool,
    plan: &TypeScriptPlan<'_>,
    unknown_aliases: &HashSet<String>,
) -> Option<TypeName> {
    let additional = obj.additional_properties.as_ref()?;
    let value_type = |expr: &IrTypeExpr| {
        if type_expr_collapses_to_unknown(expr, unknown_aliases) {
            TypeName::primitive("unknown")
        } else if wire {
            type_expr_to_typename_wire(expr, current_schema, plan)
        } else {
            plan.value(expr, Scope::model(current_schema)).public_type()
        }
    };
    let mut members = vec![value_type(additional)];
    for prop in obj.properties.values() {
        let mut ty = value_type(&prop.type_expr);
        if prop.nullable {
            ty = nullable_field_type_name(&prop.type_expr, ty, unknown_aliases);
        }
        members.push(ty);
        if !prop.required {
            members.push(TypeName::primitive("undefined"));
        }
    }
    let record = type_application(
        TypeName::primitive("Record"),
        vec![TypeName::primitive("string"), union_typename(members)],
    );
    Some(type_application(
        TypeName::primitive("Readonly"),
        vec![record],
    ))
}

/// Build a field for the $Wire interface (preserves original property name).
fn build_field_wire(
    prop: &IrProperty,
    current_schema: &str,
    plan: &TypeScriptPlan<'_>,
    unknown_aliases: &HashSet<String>,
) -> Result<FieldSpec, SigilStitchError> {
    let field_name = if is_valid_ts_identifier(&prop.name) {
        prop.name.clone()
    } else {
        format!("'{}'", prop.name)
    };
    let inner_ty = type_expr_to_typename_wire(&prop.type_expr, current_schema, plan);
    let field_ty = if prop.nullable {
        nullable_field_type_name(&prop.type_expr, inner_ty, unknown_aliases)
    } else {
        inner_ty
    };

    let mut fb = FieldSpec::builder(&field_name, field_ty).is_readonly();
    if !prop.required {
        fb = fb.is_optional();
    }
    if let Some(desc) = &prop.description {
        fb = fb.doc(desc);
    }
    fb.build()
}

/// Build a field for the ergonomic interface (camelCase property name).
fn build_field_camel(
    prop: &IrProperty,
    current_schema: &str,
    plan: &TypeScriptPlan<'_>,
    unknown_aliases: &HashSet<String>,
) -> Result<FieldSpec, SigilStitchError> {
    let camel = prop.name.to_lower_camel_case();
    let field_name = if is_valid_ts_identifier(&camel) {
        camel
    } else {
        format!("'{}'", camel)
    };
    let inner_ty = plan
        .value(&prop.type_expr, Scope::model(current_schema))
        .public_type();
    let field_ty = if prop.nullable {
        nullable_field_type_name(&prop.type_expr, inner_ty, unknown_aliases)
    } else {
        inner_ty
    };

    let mut fb = FieldSpec::builder(&field_name, field_ty).is_readonly();
    if !prop.required {
        fb = fb.is_optional();
    }
    if let Some(desc) = &prop.description {
        fb = fb.doc(desc);
    }
    fb.build()
}

/// Named refs use `Name$Wire` when the value plan requires its wire declaration.
fn type_expr_to_typename_wire(
    expr: &IrTypeExpr,
    current_schema: &str,
    plan: &TypeScriptPlan<'_>,
) -> TypeName {
    plan.value(expr, Scope::model(current_schema)).wire_type()
}

fn simplify_union_strings(members: Vec<String>) -> String {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for member in members {
        if member == "unknown" {
            return "unknown".to_string();
        }
        if seen.insert(member.clone()) {
            out.push(member);
        }
    }
    out.join(" | ")
}

fn nullable_field_type_name(
    expr: &IrTypeExpr,
    inner_ty: TypeName,
    unknown_aliases: &HashSet<String>,
) -> TypeName {
    if type_expr_collapses_to_unknown(expr, unknown_aliases) {
        inner_ty
    } else {
        union_typename(vec![inner_ty, TypeName::primitive("null")])
    }
}

fn build_json_converter(
    name: &str,
    wire_name: &str,
    current_schema: &str,
    obj: &IrObject,
    plan: &TypeScriptPlan<'_>,
    unknown_aliases: &HashSet<String>,
    direction: JsonDirection,
) -> Result<FunSpec, SigilStitchError> {
    let source = direction.source();
    let (input, output) = match direction {
        JsonDirection::FromWire => (wire_name, name),
        JsonDirection::ToWire => (name, wire_name),
    };
    let mut fields = Vec::new();
    for prop in obj.properties.values() {
        let source_key = direction.source_key(prop);
        let target_key = direction.target_key(prop);
        let access = json_property_access(source, &source_key)?;
        let mut value = json_conversion_expr(
            &prop.type_expr,
            access.clone(),
            current_schema,
            plan,
            direction,
        )?;
        let converted = value.clone();
        let guard_null = prop.nullable
            && needs_conversion(&prop.type_expr, plan)
            && !plan
                .value(&prop.type_expr, Scope::model(current_schema))
                .accepts_null();
        if guard_null {
            value = sigil_quote!(TypeScript {
                $C(access.clone()) === null ? null : $C(value)
            })?;
        }
        if !prop.required && needs_conversion(&prop.type_expr, plan) {
            value = sigil_quote!(TypeScript {
                $C(access) !== undefined ? $C(value) : undefined
            })?;
        }
        let key = if target_key == "__proto__" {
            format!("[{}]", TypeScript::new().render_string_literal(&target_key))
        } else if is_valid_ts_identifier(&target_key) {
            target_key.clone()
        } else {
            TypeScript::new().render_string_literal(&target_key)
        };
        let value = super::layout::inline(value);
        let break_value =
            4 + key.chars().count() + 2 + super::layout::fragment_width(&value)? + 1 > 100;
        if break_value && guard_null && !prop.required && obj.additional_properties.is_none() {
            let key = if target_key == "__proto__" {
                sigil_quote!(TypeScript { [$S(target_key)] })?
            } else if is_valid_ts_identifier(&target_key) {
                sigil_quote!(TypeScript { $N(target_key) })?
            } else {
                sigil_quote!(TypeScript { $S(target_key) })?
            };
            fields.push(super::layout::optional_nullable_property(
                key,
                json_property_access(source, &source_key)?,
                converted,
            )?);
            continue;
        }
        let format = if break_value {
            "%L:\n%>%L,%<\n"
        } else {
            "%L: %L,\n"
        };
        let field = if !prop.required && obj.additional_properties.is_some() {
            let condition = sigil_quote!(TypeScript { Object.prototype.hasOwnProperty.call($N(source), $S(source_key)) })?;
            let object = CodeBlock::of(
                "{ %L: %L }",
                vec![
                    sigil_stitch::code_block::Arg::Literal(key),
                    sigil_stitch::code_block::Arg::Code(value),
                ],
            )?;
            let condition = super::layout::inline(condition);
            let multiline = 4
                + 4
                + super::layout::fragment_width(&condition)?
                + 3
                + super::layout::fragment_width(&object)?
                + 7
                > 100;
            CodeBlock::of(
                if multiline {
                    "...(%L\n%>? %L\n: {}),%<\n"
                } else {
                    "...(%L ? %L : {}),\n"
                },
                vec![
                    sigil_stitch::code_block::Arg::Code(condition),
                    sigil_stitch::code_block::Arg::Code(object),
                ],
            )?
        } else {
            CodeBlock::of(
                format,
                vec![
                    sigil_stitch::code_block::Arg::Literal(key),
                    sigil_stitch::code_block::Arg::Code(value),
                ],
            )?
        };
        fields.push(field);
    }
    let body = if obj.properties.is_empty()
        && !obj
            .additional_properties
            .as_ref()
            .is_some_and(|ty| needs_conversion(ty, plan))
    {
        sigil_quote!(TypeScript { return $N(source) as unknown as $T(TypeName::primitive(output)); })?
    } else if let Some(additional) = &obj.additional_properties {
        build_open_object_conversion(
            obj,
            additional,
            fields,
            current_schema,
            plan,
            unknown_aliases,
            direction,
        )?
    } else {
        sigil_quote!(TypeScript {
            return {
                $C_each(fields)
            };
        })?
    };
    FunSpec::builder(&format!("{}{}", fn_base_name(name), direction.suffix()))
        .visibility(Visibility::Public)
        .add_param(ParameterSpec::of(source, TypeName::primitive(input)))
        .returns(vec![TypeName::primitive(output)])
        .body(body)
        .build()
}

fn build_open_object_conversion(
    obj: &IrObject,
    additional: &IrTypeExpr,
    fields: Vec<CodeBlock>,
    current_schema: &str,
    plan: &TypeScriptPlan<'_>,
    unknown_aliases: &HashSet<String>,
    direction: JsonDirection,
) -> Result<CodeBlock, SigilStitchError> {
    let source = direction.source();
    let source_keys: Vec<_> = obj
        .properties
        .values()
        .map(|prop| direction.source_key(prop))
        .collect();
    let target_keys: Vec<_> = obj
        .properties
        .values()
        .map(|prop| direction.target_key(prop))
        .collect();
    let renamed = obj
        .properties
        .values()
        .any(|prop| prop.name != prop.name.to_lower_camel_case());
    let restore = direction == JsonDirection::ToWire && renamed;
    let retain = direction == JsonDirection::FromWire && renamed;
    let metadata_key = "openapi-nexus.additionalProperties";
    let additional_type = match direction {
        JsonDirection::FromWire => type_expr_to_typename_wire(additional, current_schema, plan),
        JsonDirection::ToWire => plan
            .value(additional, Scope::model(current_schema))
            .public_type(),
    };
    let retained_type = union_typename(vec![
        type_application(
            TypeName::primitive("Record"),
            vec![TypeName::primitive("string"), additional_type.clone()],
        ),
        TypeName::primitive("undefined"),
    ]);
    let cast_additional =
        !obj.properties.is_empty() && !type_expr_collapses_to_unknown(additional, unknown_aliases);
    let converted = json_conversion_expr(
        additional,
        sigil_quote!(TypeScript { additionalValue })?,
        current_schema,
        plan,
        direction,
    )?;
    let keys = |keys: &[String]| {
        keys.iter()
            .map(|key| TypeScript::new().render_string_literal(key))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut cb = CodeBlock::builder();
    if restore {
        cb.add("const retainedProperties = Object.getOwnPropertyDescriptor(\n  %N,\n  Symbol.for(%S),\n)?.value as %T;\n", vec![sigil_stitch::code_block::Arg::Name(source.into()), sigil_stitch::code_block::Arg::StringLit(metadata_key.into()), sigil_stitch::code_block::Arg::TypeName(retained_type)]);
    }
    if source_keys.is_empty() {
        cb.add(
            "const sourceEntries = Object.entries(%N);\n",
            vec![sigil_stitch::code_block::Arg::Name(source.into())],
        );
    } else {
        cb.add("const sourceEntries = Object.entries(%N).filter(([key]) => {\n  return ![%L].includes(key);\n});\n", vec![sigil_stitch::code_block::Arg::Name(source.into()), sigil_stitch::code_block::Arg::Literal(keys(&source_keys))]);
    }
    if restore {
        cb.add("const sourceProperties = Object.fromEntries(sourceEntries);\nconst additionalEntries = Object.entries({ ...sourceProperties, ...retainedProperties });\n", ());
    } else {
        cb.add("const additionalEntries = sourceEntries;\n", ());
    }
    cb.add("const additionalProperties = Object.fromEntries(\n%>additionalEntries.map(([key, value]) => {\n%>const additionalValue = value", ());
    if cast_additional {
        cb.add(" as %T", additional_type);
    }
    if matches!(additional, IrTypeExpr::Map(_)) && needs_conversion(additional, plan) {
        cb.add(
            ";\nconst convertedValue = %L;\nreturn [key, convertedValue] as const;\n%<}),\n%<);\n",
            super::layout::inline(converted),
        );
    } else {
        cb.add(
            ";\nreturn [key, %L] as const;\n%<}),\n%<);\n",
            super::layout::inline(converted),
        );
    }
    if retain {
        cb.add("const visibleProperties = Object.fromEntries(\n  Object.entries(additionalProperties).filter(([key]) => {\n    return ![%L].includes(key);\n  }),\n);\n", keys(&target_keys));
    }
    cb.add("const result = {\n%>", ());
    cb.add(
        if retain {
            "...visibleProperties,\n"
        } else {
            "...additionalProperties,\n"
        },
        (),
    );
    for field in fields {
        cb.add_code(field);
    }
    cb.add("%<};\n", ());
    if retain {
        cb.add("const shadowedProperties = Object.fromEntries(\n  Object.entries(additionalProperties).filter(([key]) => {\n    return [%L].includes(key);\n  }),\n);\n", keys(&target_keys));
        cb.add("if (Object.keys(shadowedProperties).length > 0) {\n  Object.defineProperty(result, Symbol.for(%S), {\n    value: shadowedProperties,\n    enumerable: true,\n  });\n}\n", vec![sigil_stitch::code_block::Arg::StringLit(metadata_key.into())]);
    }
    cb.add("return result;\n", ());
    cb.build()
}

fn json_property_access(object: &str, key: &str) -> Result<CodeBlock, SigilStitchError> {
    if is_unescaped_ts_property(key) && key != "__proto__" {
        sigil_quote!(TypeScript { $N(object).$N(key) })
    } else {
        sigil_quote!(TypeScript { $N(object)[$S(key)] })
    }
}

fn is_unescaped_ts_property(key: &str) -> bool {
    // $N escapes declaration keywords, but property names must retain their
    // wire spelling. String-literal keys and indexed access avoid that escape.
    is_valid_ts_identifier(key) && !TypeScript::new().reserved_words().contains(&key)
}

fn json_conversion_expr(
    expr: &IrTypeExpr,
    access: CodeBlock,
    current_schema: &str,
    plan: &TypeScriptPlan<'_>,
    direction: JsonDirection,
) -> Result<CodeBlock, SigilStitchError> {
    plan.value(expr, Scope::model(current_schema))
        .convert(access, direction)
}

/// Returns true if the type expression contains a Named reference that needs conversion.
fn needs_conversion(expr: &IrTypeExpr, plan: &TypeScriptPlan<'_>) -> bool {
    plan.has_effect(expr)
}

/// Enum file: `export type Name = 'a' | 'b' | 1 | 2 | null;`
///
/// Handles string / integer / number / mixed value types. Returns `None` if
/// any enum value can't be rendered as a TS literal.
fn emit_enum_file_from(
    schema: &IrSchema,
    en: &IrEnum,
    flags: EmitFlags,
    ts: &TypeScript,
) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();
    let union_body = enum_union_body(en)?;

    let type_alias = sigil_quote!(TypeScript {
        export type $N(name.as_str()) = $L(union_body);
    })
    .ok()?;

    let filename = format!("{}.ts", name);
    let mut fb = FileSpec::builder_with(&filename, ts.clone());
    if let Some(doc) = &schema.description {
        // FileSpec has no structural doc slot — use a raw prelude comment.
        fb = fb.add_raw(&format!("/** {doc} */\n"));
    }
    fb = fb.add_code(type_alias);

    if flags.emit_enum_constants {
        fb = fb.add_code(build_enum_const_object(&name, en)?);
    }

    fb.build().ok()
}

/// Build a const object companion for an enum: `export const Name = { KEY: 'val' as const, ... };`
fn build_enum_const_object(name: &str, en: &IrEnum) -> Option<CodeBlock> {
    let entries: Vec<CodeBlock> = en
        .values
        .iter()
        .filter_map(|v| {
            let (key, val_literal) = match en.value_type {
                IrEnumValueType::String => {
                    let s = v.value.as_str()?;
                    (s.to_string(), format!("'{s}' as const"))
                }
                IrEnumValueType::Integer | IrEnumValueType::Number => {
                    let n = v.value.as_number()?;
                    (n.to_string(), format!("{n} as const"))
                }
                IrEnumValueType::Mixed => {
                    let (key, val_literal) = match &v.value {
                        serde_json::Value::String(s) => (s.clone(), format!("'{s}' as const")),
                        serde_json::Value::Number(n) => (n.to_string(), format!("{n} as const")),
                        _ => return None,
                    };
                    (key, val_literal)
                }
            };
            // Quote keys that aren't valid JS identifiers (e.g. dotted resource types).
            // The TypeScript renderer does not re-indent nested object entries;
            // retain this minimal static prefix to preserve the existing output.
            let key_lit = if is_valid_ts_identifier(&key) {
                format!("  {key}")
            } else {
                format!("  '{key}'")
            };
            sigil_quote!(TypeScript {
                $L(key_lit.as_str()): $L(val_literal.as_str()),
            })
            .ok()
        })
        .collect();

    sigil_quote!(TypeScript {
        export const $N(name) = {
            $C_each(entries);
        }
    })
    .ok()
}

/// Return true if `s` is a valid bare TypeScript identifier (no quoting needed).
fn is_valid_ts_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut chars = s.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_alphabetic() && first != '_' && first != '$' {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// Alias file: `export type Name = Inner;` — where `Inner` may be a named ref
/// (import auto-emitted), a primitive, a readonly array, etc.
fn emit_alias_file(
    schema: &IrSchema,
    expr: &IrTypeExpr,
    plan: &TypeScriptPlan<'_>,
    ts: &TypeScript,
) -> Result<FileSpec, SigilStitchError> {
    let name = schema.name.to_pascal_case();
    if plan.has_converter(&schema.name) {
        let wire_name = format!("{name}$Wire");
        let mut file = FileSpec::builder_with(&format!("{name}.ts"), ts.clone())
            .add_type(build_type_alias(
                &wire_name,
                type_expr_to_typename_wire(expr, &schema.name, plan),
                schema.description.as_deref(),
            )?)
            .add_type(build_type_alias(
                &name,
                plan.value(expr, Scope::model(&schema.name)).public_type(),
                schema.description.as_deref(),
            )?);
        for direction in [JsonDirection::FromWire, JsonDirection::ToWire] {
            let source = direction.source();
            let (input, output) = match direction {
                JsonDirection::FromWire => (&wire_name, &name),
                JsonDirection::ToWire => (&name, &wire_name),
            };
            let value = json_conversion_expr(
                expr,
                sigil_quote!(TypeScript { $N(source) })?,
                &schema.name,
                plan,
                direction,
            )?;
            file = file.add_function(
                FunSpec::builder(&format!("{}{}", fn_base_name(&name), direction.suffix()))
                    .visibility(Visibility::Public)
                    .add_param(ParameterSpec::of(source, TypeName::primitive(input)))
                    .returns(vec![TypeName::primitive(output)])
                    .body(sigil_quote!(TypeScript { return $C(value); })?)
                    .build()?,
            );
        }
        return file.build();
    }
    let rhs = plan.value(expr, Scope::model(&schema.name)).public_type();
    let filename = format!("{}.ts", name);
    FileSpec::builder_with(&filename, ts.clone())
        .add_type(build_type_alias(&name, rhs, schema.description.as_deref())?)
        .build()
}

pub(super) fn build_type_alias(
    name: &str,
    target: TypeName,
    description: Option<&str>,
) -> Result<TypeSpec, SigilStitchError> {
    let mut alias = TypeSpec::builder(name, TypeKind::TypeAlias)
        .visibility(Visibility::Public)
        .extends(target);
    if let Some(doc) = description {
        alias = alias.doc(doc);
    }
    alias.build()
}

/// Union file: `export type Name = A | B | C [| null];`
///
/// `nullable` appends a `null` member. Imports for each `IrTypeExpr::Named`
/// member are tracked via `TypeName::Importable` inside `TypeName::union`.
fn emit_union_file(
    schema: &IrSchema,
    union: &IrUnion,
    plan: &TypeScriptPlan<'_>,
    ts: &TypeScript,
) -> Result<FileSpec, SigilStitchError> {
    let name = schema.name.to_pascal_case();

    let mut members: Vec<TypeName> = union
        .members
        .iter()
        .map(|expr| plan.value(expr, Scope::model(&schema.name)).public_type())
        .collect();
    if union.nullable {
        members.push(TypeName::primitive("null"));
    }
    let union_ty = union_typename(members);

    let filename = format!("{}.ts", name);
    FileSpec::builder_with(&filename, ts.clone())
        .add_type(build_type_alias(
            &name,
            union_ty,
            schema.description.as_deref(),
        )?)
        .build()
}

/// Intersection file: `export type Name = A & B & C;`
///
/// Empty `members` is degenerate — skip by returning `None` so the caller
/// reports it as unsupported rather than emitting `export type Name = ;`.
fn emit_intersection_file(
    schema: &IrSchema,
    ir: &IrSpec,
    intersection: &IrIntersection,
    flags: EmitFlags,
    plan: &TypeScriptPlan<'_>,
    unknown_aliases: &HashSet<String>,
    ts: &TypeScript,
) -> Result<Option<FileSpec>, SigilStitchError> {
    if intersection.members.is_empty() {
        return Ok(None);
    }
    let name = schema.name.to_pascal_case();

    if flags.property_naming_camel_case && plan.has_converter(&schema.name) {
        return emit_intersection_file_camel_case(
            schema,
            ir,
            intersection,
            &name,
            plan,
            unknown_aliases,
            ts,
        )
        .map(Some);
    }

    let members: Vec<TypeName> = intersection
        .members
        .iter()
        .map(|expr| plan.value(expr, Scope::model(&schema.name)).public_type())
        .collect();
    let inter_ty = TypeName::intersection(members);
    let inter_ty = if open_intersection_object(intersection, ir, unknown_aliases).is_some() {
        type_application(TypeName::primitive("Readonly"), vec![inter_ty])
    } else {
        inter_ty
    };

    let filename = format!("{}.ts", name);
    FileSpec::builder_with(&filename, ts.clone())
        .add_type(build_type_alias(
            &name,
            inter_ty,
            schema.description.as_deref(),
        )?)
        .build()
        .map(Some)
}

/// Tagged union file: discriminated `export type` across three tagging styles.
///
/// - `Internal`:  `({ tag: 'VAL' } & Content) | ...`
/// - `Adjacent`:  `{ tag: 'VAL'; content: Content } | ...`
/// - `External`:  `{ VAL: Content } | ...`
///
/// Each variant's `content_type` flows through `type_expr_to_typename`, so
/// imports track like any other named ref. An empty variants list is treated
/// as unsupported (degenerate — skip rather than emit `export type X = ;`).
fn emit_tagged_union_file(
    schema: &IrSchema,
    ir: &IrSpec,
    tu: &IrTaggedUnion,
    flags: EmitFlags,
    plan: &TypeScriptPlan<'_>,
    unknown_aliases: &HashSet<String>,
    ts: &TypeScript,
) -> Result<Option<FileSpec>, SigilStitchError> {
    if tu.variants.is_empty() {
        return Ok(None);
    }

    if matches!(tu.tagging, TaggingStyle::External)
        && tu
            .variants
            .iter()
            .any(|variant| variant.additional_properties.is_some())
    {
        return emit_open_external_union_file(schema, ir, tu, flags, plan, unknown_aliases, ts)
            .map(Some);
    }
    super::tagged::emit(schema, ir, tu, plan, ts).map(Some)
}

/// External tags are wire keys in both naming modes. Keep their outer maps
/// separate from the referenced content model so nested fields are converted
/// without treating the wrapper's extensions as part of that model.
fn emit_open_external_union_file(
    schema: &IrSchema,
    ir: &IrSpec,
    union: &IrTaggedUnion,
    flags: EmitFlags,
    plan: &TypeScriptPlan<'_>,
    unknown_aliases: &HashSet<String>,
    ts: &TypeScript,
) -> Result<FileSpec, SigilStitchError> {
    let name = schema.name.to_pascal_case();
    let wire_name = format!("{name}$Wire");
    let value_type = |expr: &IrTypeExpr, wire: bool| {
        if type_expr_collapses_to_unknown(expr, unknown_aliases) {
            TypeName::primitive("unknown")
        } else if wire {
            type_expr_to_typename_wire(expr, &schema.name, plan)
        } else {
            plan.value(expr, Scope::model(&schema.name)).public_type()
        }
    };
    let wrapper_type = |wire: bool| {
        union_typename(
            union
                .variants
                .iter()
                .map(|variant| {
                    super::tagged::open_external_variant_type(
                        plan,
                        &variant.discriminator_value,
                        value_type(&variant.content_type, wire),
                        variant
                            .additional_properties
                            .as_ref()
                            .map(|expr| value_type(expr, wire)),
                    )
                })
                .collect(),
        )
    };
    let mut file = FileSpec::builder_with(&format!("{name}.ts"), ts.clone());
    if flags.property_naming_camel_case {
        file = file.add_type(build_type_alias(
            &wire_name,
            wrapper_type(true),
            schema.description.as_deref(),
        )?);
    }
    file = file.add_type(build_type_alias(
        &name,
        wrapper_type(false),
        schema.description.as_deref(),
    )?);

    if flags.property_naming_camel_case {
        for direction in [JsonDirection::FromWire, JsonDirection::ToWire] {
            let source = direction.source();
            let wire = direction == JsonDirection::FromWire;
            let (input, output) = if wire {
                (&wire_name, &name)
            } else {
                (&name, &wire_name)
            };
            let branches = union.variants.iter().map(|variant| {
                let key = &variant.discriminator_value;
                // hasOwnProperty does not narrow a union containing a closed
                // alternative. Assert only the key just checked, not an index
                // signature on the entire wrapper.
                let content_record = type_application(
                    plan.utility_type("Record"),
                    vec![TypeName::string_literal(key.clone()), value_type(&variant.content_type, wire)],
                );
                let content = json_conversion_expr(
                    &variant.content_type,
                    sigil_quote!(TypeScript { contentValue })?,
                    &schema.name, plan, direction,
                )?;
                let additional = variant.additional_properties.as_ref().map(|expr| {
                    let ty = value_type(expr, wire);
                    let cast = !type_expr_collapses_to_unknown(expr, unknown_aliases);
                    let converted = json_conversion_expr(
                        expr,
                        sigil_quote!(TypeScript { additionalValue })?,
                        &schema.name, plan, direction,
                    )?;
                    let body = sigil_quote!(TypeScript {
                        const additionalValue = $if(cast) { value as $T(ty) } $else { value };
                        return [key, $C(converted)] as const;
                    })?;
                    let arrow = super::layout::block_arrow(sigil_quote!(TypeScript { ([key, value]) })?, body);
                    let mapping = sigil_quote!(TypeScript { entries.map($C(arrow)) })?;
                    let collected = super::layout::call(sigil_quote!(TypeScript { Object.fromEntries })?, &[mapping], 33)?;
                    sigil_quote!(TypeScript {
                        const entries = Object.entries($N(source)).filter(([key]) => key !== $S(key));
                        const additionalProperties = $C(collected);
                    })
                }).transpose()?;
                let mut fields = Vec::new();
                if additional.is_some() { fields.push(sigil_quote!(TypeScript { ...additionalProperties })?); }
                fields.push(sigil_quote!(TypeScript { [$S(key)]: $C(content) })?);
                let returned = super::layout::return_object(&fields, None, 4)?;
                sigil_quote!(TypeScript {
                    if (Object.prototype.hasOwnProperty.call($N(source), $S(key))) {
                        const contentValue = ($N(source) as $T(content_record))[$S(key)];
                        $if(let Some(additional) = additional) { $C(additional) }
                        $C(returned)
                    }
                })
            }).collect::<Result<Vec<_>, SigilStitchError>>()?;
            let body = sigil_quote!(TypeScript {
                $C_each(branches)
                throw new Error($S("Unknown variant"));
            })?;
            file = file.add_function(
                FunSpec::builder(&format!("{}{}", fn_base_name(&name), direction.suffix()))
                    .visibility(Visibility::Public)
                    .add_param(ParameterSpec::of(source, TypeName::primitive(input)))
                    .returns(vec![TypeName::primitive(output)])
                    .body(body)
                    .build()?,
            );
        }
    }
    if flags.emit_type_guards {
        for guard in super::tagged::guards(schema, ir, union, plan)? {
            file = file.add_code(guard);
        }
    }
    file.build()
}

// ---------------------------------------------------------------------------
// Union camelCase: structural member selection and semantic conversion
// ---------------------------------------------------------------------------

fn emit_union_file_camel_case(
    schema: &IrSchema,
    union: &IrUnion,
    plan: &TypeScriptPlan<'_>,
    unknown_aliases: &HashSet<String>,
    strategy: UntaggedUnionStrategy,
    ts: &TypeScript,
) -> Result<FileSpec, SigilStitchError> {
    let name = schema.name.to_pascal_case();
    let wire_name = format!("{name}$Wire");
    let collapses_to_unknown = union
        .members
        .iter()
        .any(|member| type_expr_collapses_to_unknown(member, unknown_aliases));
    let mut wire_members: Vec<_> = if collapses_to_unknown {
        vec![TypeName::primitive("unknown")]
    } else {
        union
            .members
            .iter()
            .map(|member| type_expr_to_typename_wire(member, &schema.name, plan))
            .collect()
    };
    if union.nullable && !collapses_to_unknown {
        wire_members.push(TypeName::primitive("null"));
    }
    let mut ergonomic_members: Vec<_> = if collapses_to_unknown {
        vec![TypeName::primitive("unknown")]
    } else {
        union
            .members
            .iter()
            .map(|member| plan.value(member, Scope::model(&schema.name)).public_type())
            .collect()
    };
    if union.nullable && !collapses_to_unknown {
        ergonomic_members.push(TypeName::primitive("null"));
    }
    let ergonomic_type = if strategy == UntaggedUnionStrategy::Raw {
        // Raw mode exposes the wire union honestly, rather than promising
        // ergonomic members while leaving their field names unchanged.
        TypeName::primitive(&wire_name)
    } else {
        union_typename(ergonomic_members)
    };
    let mut file = FileSpec::builder_with(&format!("{name}.ts"), ts.clone())
        .add_type(build_type_alias(
            &wire_name,
            union_typename(wire_members),
            schema.description.as_deref(),
        )?)
        .add_type(build_type_alias(
            &name,
            ergonomic_type,
            schema.description.as_deref(),
        )?);
    for direction in [JsonDirection::FromWire, JsonDirection::ToWire] {
        file = file.add_function(build_union_converter(schema, union, plan, direction)?);
    }
    file.build()
}

fn build_union_converter(
    schema: &IrSchema,
    union: &IrUnion,
    plan: &TypeScriptPlan<'_>,
    direction: JsonDirection,
) -> Result<FunSpec, SigilStitchError> {
    let name = schema.name.to_pascal_case();
    let wire_name = format!("{name}$Wire");
    let (input, output) = match direction {
        JsonDirection::FromWire => (&wire_name, &name),
        JsonDirection::ToWire => (&name, &wire_name),
    };
    let source = direction.source();
    let output_type = TypeName::primitive(output);
    let body = plan.union_body(
        union,
        Scope::model(&schema.name),
        output_type.clone(),
        direction,
        &name,
        true,
    )?;
    FunSpec::builder(&format!("{}{}", fn_base_name(&name), direction.suffix()))
        .visibility(Visibility::Public)
        .add_param(ParameterSpec::of(source, TypeName::primitive(input)))
        .returns(vec![output_type])
        .body(body)
        .build()
}

// ---------------------------------------------------------------------------
// Intersection camelCase: spread-based converters
// ---------------------------------------------------------------------------

fn emit_intersection_file_camel_case(
    schema: &IrSchema,
    ir: &IrSpec,
    intersection: &IrIntersection,
    name: &str,
    plan: &TypeScriptPlan<'_>,
    unknown_aliases: &HashSet<String>,
    ts: &TypeScript,
) -> Result<FileSpec, SigilStitchError> {
    if let [member] = intersection.members.as_slice()
        && open_intersection_object(intersection, ir, unknown_aliases).is_none()
    {
        // A single-member allOf is a delegation boundary, not an object spread.
        // Its converter can return any supported shape, including arrays/null.
        return emit_alias_file(schema, member, plan, ts);
    }
    let filename = format!("{}.ts", name);
    let mut fb = FileSpec::builder_with(&filename, ts.clone());

    if let Some(object) = open_intersection_object(intersection, ir, unknown_aliases) {
        let wire_name = format!("{name}$Wire");
        let wire_type = TypeName::intersection(
            intersection
                .members
                .iter()
                .map(|member| type_expr_to_typename_wire(member, &schema.name, plan))
                .collect(),
        );
        let ergonomic_type = TypeName::intersection(
            intersection
                .members
                .iter()
                .map(|member| plan.value(member, Scope::model(&schema.name)).public_type())
                .collect(),
        );
        let mut file = fb
            .add_type(build_type_alias(
                &wire_name,
                type_application(TypeName::primitive("Readonly"), vec![wire_type]),
                schema.description.as_deref(),
            )?)
            .add_type(build_type_alias(
                name,
                type_application(TypeName::primitive("Readonly"), vec![ergonomic_type]),
                schema.description.as_deref(),
            )?);
        for direction in [JsonDirection::FromWire, JsonDirection::ToWire] {
            file = file.add_function(build_json_converter(
                name,
                &wire_name,
                &schema.name,
                &object,
                plan,
                unknown_aliases,
                direction,
            )?);
        }
        return file.build();
    }
    // $Wire type alias: A$Wire & B$Wire & ... (wire symbols come from the plan).
    let wire_name = format!("{name}$Wire");
    let wire_members = intersection
        .members
        .iter()
        .map(|member| type_expr_to_typename_wire(member, &schema.name, plan))
        .collect();
    fb = fb.add_type(build_type_alias(
        &wire_name,
        TypeName::intersection(wire_members),
        schema.description.as_deref(),
    )?);

    // Ergonomic type alias: A & B & ...
    let ergo_members = intersection
        .members
        .iter()
        .map(|member| plan.value(member, Scope::model(&schema.name)).public_type())
        .collect();
    fb = fb.add_type(build_type_alias(
        name,
        TypeName::intersection(ergo_members),
        None,
    )?);

    // Keep the existing multi-member allOf spread behavior. Only its emission
    // and child delegation move to the value plan; intersection semantics are
    // not redesigned here.
    for direction in [JsonDirection::FromWire, JsonDirection::ToWire] {
        let source = direction.source();
        let (input, output): (&str, &str) = match direction {
            JsonDirection::FromWire => (&wire_name, name),
            JsonDirection::ToWire => (name, &wire_name),
        };
        let spreads = intersection
            .members
            .iter()
            .filter(|expr| matches!(expr, IrTypeExpr::Named(name) if plan.has_converter(name)))
            .map(|expr| {
                let value = plan.value(expr, Scope::model(&schema.name));
                let ty = match direction {
                    JsonDirection::FromWire => value.wire_type(),
                    JsonDirection::ToWire => value.public_type(),
                };
                let converted = value.convert(
                    sigil_quote!(TypeScript { $N(source) as $T(ty) })?,
                    direction,
                )?;
                sigil_quote!(TypeScript { ...$C(converted) })
            })
            .collect::<Result<Vec<_>, SigilStitchError>>()?;
        let output_type = TypeName::primitive(output);
        let body = super::layout::return_object(&spreads, Some(output_type.clone()), 2)?;
        fb = fb.add_function(
            FunSpec::builder(&format!("{}{}", fn_base_name(name), direction.suffix()))
                .visibility(Visibility::Public)
                .add_param(ParameterSpec::of(source, TypeName::primitive(input)))
                .returns(vec![output_type])
                .body(body)
                .build()?,
        );
    }
    fb.build()
}

/// Plan named fields and a dictionary together, so allOf map conversion never
/// interprets a declared field as an extension or leaves its wire spelling behind.
pub(super) fn open_intersection_object(
    intersection: &IrIntersection,
    ir: &IrSpec,
    unknown_aliases: &HashSet<String>,
) -> Option<IrObject> {
    fn merge(
        expr: &IrTypeExpr,
        ir: &IrSpec,
        unknown_aliases: &HashSet<String>,
        object: &mut IrObject,
        visiting: &mut HashSet<String>,
    ) -> Option<()> {
        match expr {
            IrTypeExpr::Map(value) => {
                object.additional_properties = Some(match &object.additional_properties {
                    Some(existing) => {
                        intersect_dictionary_values(existing, value, ir, unknown_aliases)?
                    }
                    None => value.as_ref().clone(),
                });
            }
            IrTypeExpr::Named(name) => {
                if !visiting.insert(name.clone()) {
                    return None;
                }
                match &ir.schemas.get(name)?.kind {
                    IrSchemaKind::Object(member) => {
                        for (name, property) in &member.properties {
                            if let Some(existing) = object.properties.get_mut(name) {
                                existing.type_expr = intersect_dictionary_values(
                                    &existing.type_expr,
                                    &property.type_expr,
                                    ir,
                                    unknown_aliases,
                                )?;
                                // allOf requires a property if any member does,
                                // and accepts null only if every member does.
                                existing.required |= property.required;
                                existing.nullable &= property.nullable;
                            } else {
                                object.properties.insert(name.clone(), property.clone());
                            }
                        }
                        if let Some(value) = &member.additional_properties {
                            merge(
                                &IrTypeExpr::Map(Box::new(value.clone())),
                                ir,
                                unknown_aliases,
                                object,
                                visiting,
                            )?;
                        }
                    }
                    IrSchemaKind::Alias(inner) => {
                        merge(inner, ir, unknown_aliases, object, visiting)?
                    }
                    IrSchemaKind::Intersection(inner) => {
                        for member in &inner.members {
                            merge(member, ir, unknown_aliases, object, visiting)?;
                        }
                    }
                    _ => return None,
                }
                visiting.remove(name);
            }
            _ => return None,
        }
        Some(())
    }
    let mut object = IrObject {
        properties: Default::default(),
        additional_properties: None,
    };
    let mut visiting = HashSet::new();
    for member in &intersection.members {
        merge(member, ir, unknown_aliases, &mut object, &mut visiting)?;
    }
    object.additional_properties.as_ref()?;
    Some(object)
}

/// Narrow compatible allOf value contracts instead of treating an unrestricted
/// dictionary and a typed dictionary as incompatible. Resolve aliases only for
/// planning; the public intersection aliases still retain their original members.
fn intersect_dictionary_values(
    left: &IrTypeExpr,
    right: &IrTypeExpr,
    ir: &IrSpec,
    unknown_aliases: &HashSet<String>,
) -> Option<IrTypeExpr> {
    fn intersect(
        left: &IrTypeExpr,
        right: &IrTypeExpr,
        ir: &IrSpec,
        unknown_aliases: &HashSet<String>,
        visiting: &mut HashSet<String>,
    ) -> Option<IrTypeExpr> {
        if left == right {
            return Some(left.clone());
        }
        // JSON unions and their aliases can render as unknown just like Any.
        // Narrow that representation here without changing ambiguous-union conversion.
        if type_expr_collapses_to_unknown(left, unknown_aliases) {
            return Some(right.clone());
        }
        if type_expr_collapses_to_unknown(right, unknown_aliases) {
            return Some(left.clone());
        }
        for (candidate, value) in [(left, right), (right, left)] {
            if let IrTypeExpr::Named(name) = candidate
                && let Some(IrSchemaKind::Alias(target)) =
                    ir.schemas.get(name).map(|schema| &schema.kind)
            {
                if !visiting.insert(name.clone()) {
                    return None;
                }
                let result = intersect(target, value, ir, unknown_aliases, visiting);
                visiting.remove(name);
                return result;
            }
        }
        match (left, right) {
            (IrTypeExpr::Array(a), IrTypeExpr::Array(b)) => Some(IrTypeExpr::Array(Box::new(
                intersect(a, b, ir, unknown_aliases, visiting)?,
            ))),
            (IrTypeExpr::Map(a), IrTypeExpr::Map(b)) => Some(IrTypeExpr::Map(Box::new(intersect(
                a,
                b,
                ir,
                unknown_aliases,
                visiting,
            )?))),
            (IrTypeExpr::Nullable(a), IrTypeExpr::Nullable(b)) => Some(IrTypeExpr::Nullable(
                Box::new(intersect(a, b, ir, unknown_aliases, visiting)?),
            )),
            (IrTypeExpr::Nullable(inner), value) | (value, IrTypeExpr::Nullable(inner)) => {
                intersect(inner, value, ir, unknown_aliases, visiting)
            }
            (IrTypeExpr::Primitive(a), IrTypeExpr::Primitive(b))
                if primitive_to_ts(a) == primitive_to_ts(b) =>
            {
                Some(left.clone())
            }
            (
                IrTypeExpr::Primitive(IrPrimitive::String),
                value @ (IrTypeExpr::StringLiteral(_) | IrTypeExpr::StringEnum(_)),
            )
            | (
                value @ (IrTypeExpr::StringLiteral(_) | IrTypeExpr::StringEnum(_)),
                IrTypeExpr::Primitive(IrPrimitive::String),
            ) => Some(value.clone()),
            _ => None,
        }
    }
    intersect(left, right, ir, unknown_aliases, &mut HashSet::new())
}

/// Derive `isVariantName` from a discriminator value.
pub(super) fn is_guard_name(disc_value: &str) -> String {
    format!("is{}", disc_value.to_pascal_case())
}

pub(super) fn variant_guard_name(variant: &IrTaggedVariant) -> String {
    variant.discriminator_values.first().map_or_else(
        || is_guard_name(&variant.discriminator_value),
        |value| is_guard_name(value),
    )
}

/// Variants that are placeholder sentinels with no meaningful content.
/// Skip emitting a type guard for:
/// - discriminator values containing "UNSPECIFIED" (explicit sentinel)
/// - discriminator values equal to the discriminator field name (e.g. `kind`
///   key in an External-tagged union where the `kind` variant is the empty case)
pub(super) fn is_unspecified_variant(
    disc_value: &str,
    disc_field: &str,
    content_type: &IrTypeExpr,
) -> bool {
    if disc_value.to_uppercase().contains("UNSPECIFIED") {
        return true;
    }
    // When the discriminator value equals the field name itself (common with
    // mixed-tagging External unions), this is the unspecified sentinel key.
    if disc_value == disc_field {
        return true;
    }
    // Bare string-literal content types are enum sentinels, not data variants.
    matches!(content_type, IrTypeExpr::StringLiteral(_))
}

/// Build the pipe-joined literal body for an enum: `'a' | 1 | true | null`.
/// Returns `None` if any value can't be represented as a TS literal.
fn enum_union_body(en: &IrEnum) -> Option<String> {
    let parts: Option<Vec<String>> = en
        .values
        .iter()
        .map(|v| match en.value_type {
            IrEnumValueType::String => v.value.as_str().map(|s| format!("'{s}'")),
            IrEnumValueType::Integer | IrEnumValueType::Number => {
                v.value.as_number().map(|n| n.to_string())
            }
            IrEnumValueType::Mixed => json_value_to_ts_literal(&v.value),
        })
        .collect();
    let parts = parts?;
    Some(simplify_union_strings(parts))
}

/// Render a JSON value as a TS literal. Used for mixed-type enums.
fn json_value_to_ts_literal(v: &serde_json::Value) -> Option<String> {
    use serde_json::Value;
    match v {
        Value::Null => Some("null".to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Number(n) => Some(n.to_string()),
        Value::String(s) => Some(format!("'{s}'")),
        Value::Array(_) | Value::Object(_) => None,
    }
}

fn build_field(
    prop: &IrProperty,
    current_schema: &str,
    plan: &TypeScriptPlan<'_>,
    unknown_aliases: &HashSet<String>,
) -> Result<FieldSpec, SigilStitchError> {
    let field_name = if is_valid_ts_identifier(&prop.name) {
        prop.name.clone()
    } else {
        format!("'{}'", prop.name)
    };
    let inner_ty = plan
        .value(&prop.type_expr, Scope::model(current_schema))
        .public_type();
    let field_ty = if prop.nullable {
        nullable_field_type_name(&prop.type_expr, inner_ty, unknown_aliases)
    } else {
        inner_ty
    };

    let mut fb = FieldSpec::builder(&field_name, field_ty).is_readonly();
    if !prop.required {
        fb = fb.is_optional();
    }
    if let Some(desc) = &prop.description {
        fb = fb.doc(desc);
    }
    fb.build()
}

#[cfg(test)]
fn build_unknown_alias_set(ir: &IrSpec) -> HashSet<String> {
    TypeScriptPlan::new(ir, EmitFlags::default())
        .unknown_aliases()
        .clone()
}

/// Lower every `IrSchema` in the spec into a sigil-rendered `FileInfo`.
pub fn generate_model_files(
    ir: &IrSpec,
    flags: EmitFlags,
    ts: &TypeScript,
) -> Result<Vec<FileInfo>, GeneratorError> {
    let plan = TypeScriptPlan::new(ir, flags);
    generate_model_files_with_plan(&plan, ts)
}

pub(super) fn generate_model_files_with_plan(
    plan: &TypeScriptPlan<'_>,
    ts: &TypeScript,
) -> Result<Vec<FileInfo>, GeneratorError> {
    let ir = plan.ir();
    let flags = plan.flags();
    let header = super::project_files::render_file_header(&ir.info);
    let unknown_aliases = plan.unknown_aliases();
    let mut files = Vec::with_capacity(ir.schemas.len());

    for (name, schema) in &ir.schemas {
        let file_spec = emit_model_file(schema, ir, flags, plan, unknown_aliases, ts)
            .map_err(|source| GeneratorError::emission(ArtifactKind::Model, name, source))?
            .ok_or_else(|| GeneratorError::UnsupportedSchema {
                schema: name.clone(),
                kind: format!("{:?}", schema.kind),
            })?;
        let body = file_spec
            .with_lang(super::layout::TypeScript::from(ts.clone()))
            .render(100)
            .map_err(|source| GeneratorError::emission(ArtifactKind::Model, name, source))?;
        let content = format!("{header}{body}");
        let filename = format!("{}.ts", schema.name.to_pascal_case());
        files.push(FileInfo::model(filename, content));
    }

    let mut runtime_files = Vec::new();
    if let Some(file) = plan
        .matching_file(ts)
        .map_err(|source| GeneratorError::emission(ArtifactKind::Model, "union matching", source))?
    {
        runtime_files.push(file);
    }
    runtime_files.extend(plan.conversion_files(ts).map_err(|source| {
        GeneratorError::emission(ArtifactKind::Model, "compound value conversion", source)
    })?);
    for file in runtime_files {
        let filename = file.filename().to_owned();
        let body = file
            .with_lang(super::layout::TypeScript::from(ts.clone()))
            .render(100)
            .map_err(|source| GeneratorError::emission(ArtifactKind::Model, &filename, source))?;
        files.push(FileInfo::runtime(
            filename.to_string(),
            format!("{header}{body}"),
        ));
    }

    Ok(files)
}

fn primitive_to_ts(p: &IrPrimitive) -> &'static str {
    match p {
        IrPrimitive::Binary => "Blob | File",
        IrPrimitive::String
        | IrPrimitive::Date
        | IrPrimitive::DateTime
        | IrPrimitive::Uuid
        | IrPrimitive::StringWithFormat(_) => "string",
        IrPrimitive::Integer
        | IrPrimitive::Number
        | IrPrimitive::IntegerWithFormat(_)
        | IrPrimitive::NumberWithFormat(_) => "number",
        IrPrimitive::Boolean => "boolean",
    }
}

#[cfg(test)]
mod render_tests {
    use super::*;
    use sigil_stitch::assert_rendered;

    #[test]
    fn inline_union_conversion_has_a_typed_lexical_boundary() {
        let expr = IrTypeExpr::Union(vec![IrTypeExpr::Primitive(IrPrimitive::Boolean)]);
        let mut ir = additional_properties_ir();
        ir.schemas.clear();
        ir.schemas.insert(
            "Inline".into(),
            IrSchema {
                name: "Inline".into(),
                description: None,
                deprecated: false,
                kind: IrSchemaKind::Alias(expr.clone()),
                is_component: true,
            },
        );
        let plan = TypeScriptPlan::new(
            &ir,
            EmitFlags {
                property_naming_camel_case: true,
                ..Default::default()
            },
        );
        let conversion = json_conversion_expr(
            &expr,
            sigil_quote!(TypeScript { input }).unwrap(),
            "Inline",
            &plan,
            JsonDirection::FromWire,
        )
        .unwrap();
        assert_rendered!(
            TypeScript::new(),
            conversion,
            "import { convert$0FromJSON } from '../runtime/valueConversion$0';\n\nconvert$0FromJSON(input)\n"
        );
        let file = plan.conversion_files(&TypeScript::new()).unwrap().remove(0);
        assert_eq!(
            file.with_lang(super::super::layout::TypeScript::from(TypeScript::new()))
                .render(100)
                .unwrap(),
            "import { matches$0FromJSON, matches$0ToJSON } from '../runtime/unionMatch';\n\nexport type Value$0Wire = boolean;\n\nexport type Value$0 = boolean;\n\nexport function convert$0FromJSON(json: boolean): Value$0 {\n  const candidate: unknown = json;\n  if (matches$0FromJSON(candidate)) {\n    return json as unknown as boolean;\n  }\n  return json as unknown as Value$0;\n}\n\nexport function convert$0ToJSON(value: boolean): Value$0Wire {\n  const candidate: unknown = value;\n  if (matches$0ToJSON(candidate)) {\n    return value as unknown as boolean;\n  }\n  return value as unknown as Value$0Wire;\n}\n"
        );
    }

    #[test]
    fn unknown_union_aliases_resolve_without_schema_order_dependence() {
        let parsed = crate::parser::parse_content_yaml(include_str!(
            "../../../../tests/fixtures/valid/typescript-untagged-union.yaml"
        ))
        .unwrap();
        let mut ir = crate::ir::lower::lower(parsed).unwrap();
        for _ in 0..2 {
            let unknown = build_unknown_alias_set(&ir);
            for name in [
                "OpenInner",
                "OpenAlias",
                "AliasOuter",
                "Outer",
                "AmbiguousInner",
                "AmbiguousOuter",
            ] {
                assert!(
                    unknown.contains(name),
                    "{name} must retain the raw JSON type contract"
                );
            }
            assert!(!unknown.contains("Detail"));
            assert!(!unknown.contains("NullableMapFallback"));
            ir.schemas.reverse();
        }
    }

    #[test]
    fn extracted_enum_null_permission_has_a_typed_conversion_boundary() {
        let parsed = crate::parser::parse_content_yaml(
            r#"openapi: 3.1.0
info: {title: Enum null permission, version: '1'}
paths: {}
components:
  schemas:
    ScalarChoice:
      anyOf: [{type: integer}, {type: boolean}]
    Parent:
      type: object
      required: [scalar_value]
      properties:
        scalar_value:
          anyOf: [{$ref: '#/components/schemas/ScalarChoice'}, {type: 'null'}]
          enum: [1, null]
"#,
        )
        .unwrap();
        let ir = crate::ir::lower::lower(parsed).unwrap();
        let IrSchemaKind::Object(parent) = &ir.schemas["Parent"].kind else {
            panic!("parent is an object");
        };
        let plan = TypeScriptPlan::new(
            &ir,
            EmitFlags {
                property_naming_camel_case: true,
                untagged_union_strategy: UntaggedUnionStrategy::Strict,
                ..Default::default()
            },
        );
        let conversion = json_conversion_expr(
            &parent.properties["scalar_value"].type_expr,
            sigil_quote!(TypeScript { input }).unwrap(),
            "Parent",
            &plan,
            JsonDirection::FromWire,
        )
        .unwrap();
        assert_rendered!(
            TypeScript::new(),
            conversion,
            "import { convert$0FromJSON } from '../runtime/valueConversion$0';\n\nconvert$0FromJSON(input)\n"
        );
        let file = plan.conversion_files(&TypeScript::new()).unwrap().remove(0);
        assert_eq!(
            file.with_lang(super::super::layout::TypeScript::from(TypeScript::new()))
                .render(100)
                .unwrap(),
            "import type { ScalarChoice, ScalarChoice$Wire } from '../models/ScalarChoice';\nimport { scalarChoiceFromJSON, scalarChoiceToJSON } from '../models/ScalarChoice';\n\nexport type Value$0Wire = ScalarChoice$Wire | null;\n\nexport type Value$0 = ScalarChoice | null;\n\nexport function convert$0FromJSON(json: ScalarChoice$Wire | null): Value$0 {\n  if (json == null) {\n    return null;\n  }\n  return scalarChoiceFromJSON(json);\n}\n\nexport function convert$0ToJSON(value: ScalarChoice | null): Value$0Wire {\n  if (value == null) {\n    return null;\n  }\n  return scalarChoiceToJSON(value);\n}\n"
        );
    }

    fn additional_properties_ir() -> IrSpec {
        let parsed = crate::parser::parse_content_yaml(include_str!(
            "../../../../tests/fixtures/valid/typescript-additional-properties.yaml"
        ))
        .unwrap();
        crate::ir::lower::lower(parsed).unwrap()
    }

    #[test]
    fn structured_readonly_intersection_keeps_imports() {
        let target = type_application(
            TypeName::primitive("Readonly"),
            vec![TypeName::intersection(vec![
                TypeName::importable_type("./Detail", "Detail$Wire"),
                type_application(
                    TypeName::primitive("Record"),
                    vec![
                        TypeName::primitive("string"),
                        TypeName::primitive("unknown"),
                    ],
                ),
            ])],
        );
        let alias = build_type_alias("Envelope$Wire", target, None).unwrap();
        let fragments = alias.emit(&TypeScript::new()).unwrap();
        let block = sigil_quote!(TypeScript { $C_each(fragments) }).unwrap();
        assert_rendered!(
            TypeScript::new(),
            block,
            "import type { Detail$Wire } from './Detail';\n\nexport type Envelope$Wire = Readonly<Detail$Wire & Record<string, unknown>>;\n"
        );
    }

    #[test]
    fn convertible_aliases_resolve_independently_of_schema_order() {
        let mut ir = additional_properties_ir();
        let flags = EmitFlags {
            property_naming_camel_case: true,
            ..Default::default()
        };
        for _ in 0..2 {
            let plan = TypeScriptPlan::new(&ir, flags);
            for name in [
                "DetailAlias",
                "DetailAliasChain",
                "DetailListAlias",
                "NullableDetailAlias",
            ] {
                assert!(
                    plan.has_converter(name),
                    "{name} must retain model conversion"
                );
            }
            // A representation that is unchanged may still have a union
            // selection action. Symbol and effect planning are independent.
            assert!(plan.has_converter("AnyJsonValue"));
            assert!(plan.has_wire_type("AnyJsonValue"));
            let preserved = TypeScriptPlan::new(&ir, EmitFlags::default());
            assert!(
                ir.schemas
                    .keys()
                    .all(|name| !preserved.has_converter(name) && !preserved.has_wire_type(name))
            );
            ir.schemas.reverse();
        }
    }

    #[test]
    fn model_alias_file_tracks_wire_types_and_converter_imports() {
        let ir = additional_properties_ir();
        let flags = EmitFlags {
            property_naming_camel_case: true,
            ..Default::default()
        };
        let plan = TypeScriptPlan::new(&ir, flags);
        let file = emit_model_file(
            &ir.schemas["DetailAliasChain"],
            &ir,
            flags,
            &plan,
            &HashSet::new(),
            &TypeScript::new(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            file.render(100).unwrap(),
            r#"import type { DetailAlias, DetailAlias$Wire } from './DetailAlias';
import { detailAliasFromJSON, detailAliasToJSON } from './DetailAlias';

export type DetailAliasChain$Wire = DetailAlias$Wire;

export type DetailAliasChain = DetailAlias;

export function detailAliasChainFromJSON(json: DetailAliasChain$Wire): DetailAliasChain {
  return detailAliasFromJSON(json);
}

export function detailAliasChainToJSON(value: DetailAliasChain): DetailAliasChain$Wire {
  return detailAliasToJSON(value);
}
"#
        );
    }

    #[test]
    fn singleton_nullable_union_converts_its_only_non_null_member() {
        let ir = additional_properties_ir();
        let flags = EmitFlags {
            property_naming_camel_case: true,
            ..Default::default()
        };
        let plan = TypeScriptPlan::new(&ir, flags);
        let file = emit_model_file(
            &ir.schemas["NullableDetailAlias"],
            &ir,
            flags,
            &plan,
            &HashSet::new(),
            &TypeScript::new(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            file.render(100).unwrap(),
            r#"import { matches$5FromJSON, matches$5ToJSON } from '../runtime/unionMatch';
import type { DetailAliasChain, DetailAliasChain$Wire } from './DetailAliasChain';
import { detailAliasChainFromJSON, detailAliasChainToJSON } from './DetailAliasChain';

export type NullableDetailAlias$Wire = DetailAliasChain$Wire | null;

export type NullableDetailAlias = DetailAliasChain | null;

export function nullableDetailAliasFromJSON(json: NullableDetailAlias$Wire): NullableDetailAlias {
  const candidate: unknown = json;
  if (matches$5FromJSON(candidate)) {
    return detailAliasChainFromJSON(json as unknown as DetailAliasChain$Wire);
  }
  if (candidate === null) {
    return null;
  }
  return json as unknown as NullableDetailAlias;
}

export function nullableDetailAliasToJSON(value: NullableDetailAlias): NullableDetailAlias$Wire {
  const candidate: unknown = value;
  if (matches$5ToJSON(candidate)) {
    return detailAliasChainToJSON(value as unknown as DetailAliasChain);
  }
  if (candidate === null) {
    return null;
  }
  return value as unknown as NullableDetailAlias$Wire;
}
"#
        );
    }

    #[test]
    fn planned_allof_type_keeps_readonly_dictionary_and_imports() {
        let ir = additional_properties_ir();
        let plan = TypeScriptPlan::new(&ir, EmitFlags::default());
        let file = emit_model_file(
            &ir.schemas["AllOfMixedStringMap"],
            &ir,
            EmitFlags::default(),
            &plan,
            &build_unknown_alias_set(&ir),
            &TypeScript::new(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            file.render(100).unwrap(),
            "import type { AllOfMixedStringMapAllOf1 } from './AllOfMixedStringMapAllOf1';\n\nexport type AllOfMixedStringMap = Readonly<AllOfMixedStringMapAllOf1 & Record<string, string>>;\n"
        );
    }

    #[test]
    fn fallback_allof_type_tracks_nested_dictionary_imports() {
        let ir = additional_properties_ir();
        // A non-object member deliberately keeps this on the fallback path.
        let intersection = IrIntersection {
            members: vec![
                IrTypeExpr::Named("Detail".to_string()),
                IrTypeExpr::Map(Box::new(IrTypeExpr::Named("AnyJsonValue".to_string()))),
                IrTypeExpr::Primitive(IrPrimitive::String),
            ],
        };
        let flags = EmitFlags {
            property_naming_camel_case: true,
            ..Default::default()
        };
        let plan = TypeScriptPlan::new(&ir, flags);
        let mut schema = ir.schemas["AllOfJsonDictionary"].clone();
        schema.kind = IrSchemaKind::Intersection(intersection);
        let file = emit_model_file(
            &schema,
            &ir,
            flags,
            &plan,
            &build_unknown_alias_set(&ir),
            &TypeScript::new(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            file.render(100).unwrap(),
            r#"import type { AnyJsonValue, AnyJsonValue$Wire } from './AnyJsonValue';
import type { Detail, Detail$Wire } from './Detail';
import { detailFromJSON, detailToJSON } from './Detail';

export type AllOfJsonDictionary$Wire = Detail$Wire & Record<string, AnyJsonValue$Wire> & string;

export type AllOfJsonDictionary = Detail & Record<string, AnyJsonValue> & string;

export function allOfJsonDictionaryFromJSON(json: AllOfJsonDictionary$Wire): AllOfJsonDictionary {
  return { ...detailFromJSON(json as Detail$Wire) } as AllOfJsonDictionary;
}

export function allOfJsonDictionaryToJSON(value: AllOfJsonDictionary): AllOfJsonDictionary$Wire {
  return { ...detailToJSON(value as Detail) } as AllOfJsonDictionary$Wire;
}
"#
        );
    }

    #[test]
    fn compatible_allof_dictionaries_plan_all_named_fields_in_both_orders() {
        let ir = additional_properties_ir();
        let unknown_aliases = build_unknown_alias_set(&ir);
        let IrSchemaKind::Intersection(intersection) = &ir.schemas["HeterogeneousAllOf"].kind
        else {
            panic!("intersection fixture")
        };
        let mut intersection = intersection.clone();
        for _ in 0..2 {
            let object = open_intersection_object(&intersection, &ir, &unknown_aliases).unwrap();
            assert_eq!(
                object.additional_properties,
                Some(IrTypeExpr::Primitive(IrPrimitive::String))
            );
            assert_eq!(object.properties.len(), 2);
            for key in ["known_field", "other_field"] {
                let property = &object.properties[key];
                assert_eq!(
                    property.type_expr,
                    IrTypeExpr::Primitive(IrPrimitive::String)
                );
                assert!(property.required);
                assert!(!property.nullable);
            }
            intersection.members.reverse();
        }
    }

    #[test]
    fn json_model_allof_dictionaries_plan_fields_in_both_orders() {
        let ir = additional_properties_ir();
        let unknown_aliases = build_unknown_alias_set(&ir);
        let IrSchemaKind::Intersection(intersection) = &ir.schemas["JsonModelAllOf"].kind else {
            panic!("intersection fixture")
        };
        let mut intersection = intersection.clone();
        for _ in 0..2 {
            let object = open_intersection_object(&intersection, &ir, &unknown_aliases)
                .expect("JSON and model dictionaries have a compatible value contract");
            assert_eq!(
                object.additional_properties,
                Some(IrTypeExpr::Named("Detail".to_string()))
            );
            assert_eq!(object.properties.len(), 2);
            for key in ["known_first", "known_second"] {
                assert!(object.properties[key].required);
            }
            intersection.members.reverse();
        }
    }

    #[test]
    fn compatible_allof_values_narrow_aliases_and_nested_containers() {
        let mut ir = additional_properties_ir();
        let mut json_alias = ir.schemas["DetailAlias"].clone();
        json_alias.name = "JsonAlias".to_string();
        json_alias.kind = IrSchemaKind::Alias(IrTypeExpr::Named("AnyJsonValue".to_string()));
        ir.schemas.insert(json_alias.name.clone(), json_alias);
        let unknown_aliases = build_unknown_alias_set(&ir);
        let string = IrTypeExpr::Primitive(IrPrimitive::String);
        let detail = IrTypeExpr::Named("Detail".to_string());
        let alias = IrTypeExpr::Named("DetailAliasChain".to_string());
        let literal = IrTypeExpr::StringLiteral("fixed".to_string());
        for (left, right, expected) in [
            (IrTypeExpr::Any, string.clone(), string.clone()),
            (
                IrTypeExpr::Named("AnyJsonValue".to_string()),
                detail.clone(),
                detail.clone(),
            ),
            (
                IrTypeExpr::Named("JsonAlias".to_string()),
                detail.clone(),
                detail.clone(),
            ),
            (alias.clone(), detail.clone(), detail.clone()),
            (
                IrTypeExpr::Array(Box::new(alias)),
                IrTypeExpr::Array(Box::new(detail.clone())),
                IrTypeExpr::Array(Box::new(detail)),
            ),
            (
                IrTypeExpr::Map(Box::new(IrTypeExpr::Any)),
                IrTypeExpr::Map(Box::new(string.clone())),
                IrTypeExpr::Map(Box::new(string.clone())),
            ),
            (
                IrTypeExpr::Nullable(Box::new(string.clone())),
                string.clone(),
                string.clone(),
            ),
            (string, literal.clone(), literal),
        ] {
            assert_eq!(
                intersect_dictionary_values(&left, &right, &ir, &unknown_aliases),
                Some(expected.clone())
            );
            assert_eq!(
                intersect_dictionary_values(&right, &left, &ir, &unknown_aliases),
                Some(expected)
            );
        }
    }

    #[test]
    fn ambiguous_allof_value_unions_keep_the_existing_fallback() {
        let ir = additional_properties_ir();
        let unknown_aliases = build_unknown_alias_set(&ir);
        let union = IrTypeExpr::Union(vec![
            IrTypeExpr::Primitive(IrPrimitive::String),
            IrTypeExpr::Primitive(IrPrimitive::Number),
        ]);
        let detail = IrTypeExpr::Named("Detail".to_string());
        for (left, right) in [(&union, &detail), (&detail, &union)] {
            assert_eq!(
                intersect_dictionary_values(left, right, &ir, &unknown_aliases),
                None
            );
        }
    }

    #[test]
    fn allof_duplicate_fields_merge_required_and_nullable_contracts_in_both_orders() {
        let parsed = crate::parser::parse_content_yaml(
            r#"
openapi: 3.1.0
info: {title: Compatible maps, version: 1.0.0}
paths: {}
components:
  schemas:
    Composed:
      allOf:
        - type: object
          properties:
            known_field: {type: [string, 'null']}
          additionalProperties: true
        - type: object
          required: [known_field]
          properties:
            known_field: {type: string}
          additionalProperties: {type: string}
"#,
        )
        .unwrap();
        let ir = crate::ir::lower::lower(parsed).unwrap();
        let unknown_aliases = build_unknown_alias_set(&ir);
        let IrSchemaKind::Intersection(intersection) = &ir.schemas["Composed"].kind else {
            panic!("intersection fixture")
        };
        let mut intersection = intersection.clone();
        for _ in 0..2 {
            let object = open_intersection_object(&intersection, &ir, &unknown_aliases).unwrap();
            let property = &object.properties["known_field"];
            assert!(property.required);
            assert!(!property.nullable);
            assert_eq!(
                property.type_expr,
                IrTypeExpr::Primitive(IrPrimitive::String)
            );
            intersection.members.reverse();
        }
    }

    #[test]
    fn nullable_and_union_array_types_keep_precedence_and_imports() {
        let string = IrTypeExpr::Primitive(IrPrimitive::String);
        let cases = [
            (
                IrTypeExpr::Nullable(Box::new(IrTypeExpr::Array(Box::new(IrTypeExpr::Array(
                    Box::new(string.clone()),
                ))))),
                "export type Values = readonly string[][] | null;\n",
            ),
            (
                IrTypeExpr::Array(Box::new(IrTypeExpr::Nullable(Box::new(IrTypeExpr::Array(
                    Box::new(string.clone()),
                ))))),
                "export type Values = readonly (string[] | null)[];\n",
            ),
            (
                IrTypeExpr::Array(Box::new(IrTypeExpr::Union(vec![
                    string,
                    IrTypeExpr::Primitive(IrPrimitive::Number),
                ]))),
                "export type Values = readonly (string | number)[];\n",
            ),
            (
                IrTypeExpr::Array(Box::new(IrTypeExpr::Nullable(Box::new(IrTypeExpr::Named(
                    "Detail".to_string(),
                ))))),
                "import type { Detail } from './Detail';\n\nexport type Values = readonly (Detail | null)[];\n",
            ),
            (
                IrTypeExpr::Array(Box::new(IrTypeExpr::Union(vec![
                    IrTypeExpr::Primitive(IrPrimitive::Integer),
                    IrTypeExpr::Primitive(IrPrimitive::Number),
                ]))),
                "export type Values = readonly number[];\n",
            ),
            (
                IrTypeExpr::Nullable(Box::new(IrTypeExpr::Nullable(Box::new(IrTypeExpr::Any)))),
                "export type Values = unknown;\n",
            ),
        ];
        for (expr, expected) in cases {
            let ir = additional_properties_ir();
            let plan = TypeScriptPlan::new(&ir, EmitFlags::default());
            let ty = plan.value(&expr, Scope::model("Values")).public_type();
            let block = sigil_quote!(TypeScript {
                export type Values = $T(ty);
            })
            .expect("array type render test quote builds");
            assert_rendered!(TypeScript::new(), width = 100, block, expected);
        }
    }

    #[test]
    fn additional_property_conversions_keep_nested_nullability_and_value_imports() {
        let expr = IrTypeExpr::Map(Box::new(IrTypeExpr::Nullable(Box::new(IrTypeExpr::Named(
            "Detail".to_string(),
        )))));
        let mut ir = additional_properties_ir();
        ir.schemas.retain(|name, _| name == "Detail");
        ir.schemas.insert(
            "Values".into(),
            IrSchema {
                name: "Values".into(),
                description: None,
                deprecated: false,
                kind: IrSchemaKind::Alias(expr.clone()),
                is_component: true,
            },
        );
        let plan = TypeScriptPlan::new(
            &ir,
            EmitFlags {
                property_naming_camel_case: true,
                ..Default::default()
            },
        );
        for (direction, expected) in [
            (
                JsonDirection::FromWire,
                "import { convert$0FromJSON } from '../runtime/valueConversion$0';\n\nconvert$0FromJSON(values)\n",
            ),
            (
                JsonDirection::ToWire,
                "import { convert$0ToJSON } from '../runtime/valueConversion$0';\n\nconvert$0ToJSON(values)\n",
            ),
        ] {
            let access = sigil_quote!(TypeScript { values }).unwrap();
            let block = json_conversion_expr(&expr, access, "Envelope", &plan, direction).unwrap();
            assert_rendered!(TypeScript::new(), width = 200, block, expected);
        }
        let mut helper_files = plan.conversion_files(&TypeScript::new()).unwrap();
        assert_eq!(helper_files.len(), 2);
        let file = helper_files.remove(0);
        assert_eq!(
            file.with_lang(super::super::layout::TypeScript::from(TypeScript::new()))
                .render(100)
                .unwrap(),
            "import type { Detail, Detail$Wire } from '../models/Detail';\nimport { convert$1FromJSON, convert$1ToJSON } from './valueConversion$1';\n\nexport type Value$0Wire = Record<string, Detail$Wire | null>;\n\nexport type Value$0 = Record<string, Detail | null>;\n\nexport function convert$0FromJSON(json: Record<string, Detail$Wire | null>): Value$0 {\n  return Object.fromEntries(\n    Object.entries(json ?? {}).map(([key, item]) => [key, convert$1FromJSON(item)]),\n  );\n}\n\nexport function convert$0ToJSON(value: Record<string, Detail | null>): Value$0Wire {\n  return Object.fromEntries(\n    Object.entries(value ?? {}).map(([key, item]) => [key, convert$1ToJSON(item)]),\n  );\n}\n"
        );
        assert_eq!(
            helper_files
                .remove(0)
                .with_lang(super::super::layout::TypeScript::from(TypeScript::new()))
                .render(100)
                .unwrap(),
            "import type { Detail, Detail$Wire } from '../models/Detail';\nimport { detailFromJSON, detailToJSON } from '../models/Detail';\n\nexport type Value$1Wire = Detail$Wire | null;\n\nexport type Value$1 = Detail | null;\n\nexport function convert$1FromJSON(json: Detail$Wire | null): Value$1 {\n  if (json == null) {\n    return null;\n  }\n  return detailFromJSON(json);\n}\n\nexport function convert$1ToJSON(value: Detail | null): Value$1Wire {\n  if (value == null) {\n    return null;\n  }\n  return detailToJSON(value);\n}\n"
        );
    }

    #[test]
    fn open_external_types_qualify_utilities_and_track_payload_imports() {
        let parsed = crate::parser::parse_content_yaml(include_str!(
            "../../../../tests/fixtures/valid/typescript-untagged-union.yaml"
        ))
        .unwrap();
        let ir = crate::ir::lower::lower(parsed).unwrap();
        let plan = TypeScriptPlan::new(
            &ir,
            EmitFlags {
                property_naming_camel_case: true,
                ..Default::default()
            },
        );
        let detail = IrTypeExpr::Named("Detail".to_string());
        let items = IrTypeExpr::Array(Box::new(detail.clone()));
        let ty = super::super::tagged::open_external_variant_type(
            &plan,
            "extension_key",
            plan.value(&detail, Scope::model("Readonly")).wire_type(),
            Some(plan.value(&items, Scope::model("Readonly")).wire_type()),
        );
        let block = sigil_quote!(TypeScript { export type Readonly = $T(ty); })
            .expect("qualified open external type quote builds");
        assert_rendered!(
            TypeScript::new(),
            width = 200,
            block,
            "import type { Detail$Wire } from './Detail';\n\nexport type Readonly = globalThis.Readonly<globalThis.Record<'extension_key', Detail$Wire>> & globalThis.Readonly<globalThis.Record<string, readonly Detail$Wire[] | Detail$Wire>>;\n"
        );
    }

    #[test]
    fn raw_tagged_guard_keeps_semantic_payload_types_and_imports() {
        let parsed = crate::parser::parse_content_yaml(include_str!(
            "../../../../tests/fixtures/valid/typescript-untagged-union.yaml"
        ))
        .unwrap();
        let ir = crate::ir::lower::lower(parsed).unwrap();
        let schema = &ir.schemas["AdjacentContainer"];
        let IrSchemaKind::TaggedUnion(union) = &schema.kind else {
            panic!("adjacent fixture")
        };
        let plan = TypeScriptPlan::new(
            &ir,
            EmitFlags {
                property_naming_camel_case: true,
                untagged_union_strategy: UntaggedUnionStrategy::Raw,
                ..Default::default()
            },
        );
        let guards = super::super::tagged::guards(schema, &ir, union, &plan).unwrap();
        assert_eq!(guards.len(), 1);
        assert_rendered!(
            TypeScript::new(),
            width = 100,
            guards[0].clone(),
            "import type { Detail$Wire } from './Detail';\n\nexport function isData(\n  value: AdjacentContainer,\n): value is globalThis.Record<'eventType', 'data'> &\n  globalThis.Record<'eventData', readonly (globalThis.Record<string, Detail$Wire> | string)[]> {\n  return value['eventType'] === 'data';\n}\n"
        );
    }

    #[test]
    fn additional_property_types_keep_dictionary_value_imports_and_known_types() {
        let parsed = crate::parser::parse_content_yaml(include_str!(
            "../../../../tests/fixtures/valid/typescript-additional-properties.yaml"
        ))
        .unwrap();
        let ir = crate::ir::lower::lower(parsed).unwrap();
        let IrSchemaKind::Object(obj) = &ir.schemas["MixedModelMap"].kind else {
            panic!("object fixture")
        };
        let plan = TypeScriptPlan::new(
            &ir,
            EmitFlags {
                property_naming_camel_case: true,
                ..Default::default()
            },
        );
        for (wire, expected) in [
            (
                false,
                "import type { Detail } from './Detail';\n\nexport type Values = Readonly<Record<string, Detail | number>>;\n",
            ),
            (
                true,
                "import type { Detail$Wire } from './Detail';\n\nexport type Values = Readonly<Record<string, Detail$Wire | number>>;\n",
            ),
        ] {
            let ty = additional_properties_super_type(
                obj,
                "MixedModelMap",
                wire,
                &plan,
                &HashSet::new(),
            )
            .unwrap();
            let block = sigil_quote!(TypeScript { export type Values = $T(ty); }).unwrap();
            assert_rendered!(TypeScript::new(), width = 100, block, expected);
        }
    }

    #[test]
    fn json_property_access_does_not_escape_wire_keywords_or_quoted_keys() {
        for (key, expected) in [
            ("known_field", "json.known_field\n"),
            ("type", "json['type']\n"),
            ("__proto__", "json['__proto__']\n"),
            ("quoted'field", "json['quoted\\'field']\n"),
        ] {
            let block = json_property_access("json", key).unwrap();
            assert_rendered!(TypeScript::new(), block, expected);
        }
    }
}
