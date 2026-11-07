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
use crate::generators::type_application;
use crate::generators::union_match::{JsonShape, plan_union_members};
use crate::generators::{ArtifactKind, GeneratorError};
use crate::ir::types::{
    IrEnum, IrEnumValueType, IrIntersection, IrObject, IrPrimitive, IrProperty, IrSchema,
    IrSchemaKind, IrSpec, IrTaggedUnion, IrTypeExpr, IrUnion, TaggingStyle,
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
use sigil_stitch::spec::import_spec::ImportSpec;
use sigil_stitch::spec::modifiers::{TypeKind, Visibility};
use sigil_stitch::spec::parameter_spec::ParameterSpec;
use sigil_stitch::spec::type_spec::TypeSpec;
use sigil_stitch::type_name::TypeName;
use std::collections::HashSet;

use super::config::UntaggedUnionStrategy;

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
pub fn value_exports_for_schema(
    schema: &IrSchema,
    flags: EmitFlags,
    convertible: &HashSet<String>,
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
                        .map(|v| is_guard_name(&v.discriminator_value)),
                );
            }
            exports
        }
        IrSchemaKind::Alias(_) | IrSchemaKind::Intersection(_)
            if convertible.contains(&schema.name) =>
        {
            let pascal = schema.name.to_pascal_case();
            let base = fn_base_name(&pascal);
            vec![format!("{base}FromJSON"), format!("{base}ToJSON")]
        }
        IrSchemaKind::Union(_) if convertible.contains(&schema.name) => {
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
            .map(|v| is_guard_name(&v.discriminator_value))
            .collect(),
        _ => vec![],
    }
}

/// Return extra type-export names for the barrel (beyond the primary type name).
///
/// Objects in camelCase mode also export `Name$Wire`.
pub fn extra_type_exports_for_schema(
    schema: &IrSchema,
    flags: EmitFlags,
    convertible: &HashSet<String>,
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
            if convertible.contains(&schema.name) =>
        {
            vec![format!("{}$Wire", schema.name.to_pascal_case())]
        }
        IrSchemaKind::Union(_) if convertible.contains(&schema.name) => {
            vec![format!("{}$Wire", schema.name.to_pascal_case())]
        }
        _ => vec![],
    }
}

/// Emit a TypeScript model file for an IR schema. Dispatches on `schema.kind`.
pub fn emit_model_file(
    schema: &IrSchema,
    ir: &IrSpec,
    flags: EmitFlags,
    convertible: &HashSet<String>,
    unknown_aliases: &HashSet<String>,
    ts: &TypeScript,
) -> Result<Option<FileSpec>, SigilStitchError> {
    if let IrSchemaKind::Union(union) = &schema.kind
        && flags.property_naming_camel_case
        && convertible.contains(&schema.name)
    {
        if let [member] = union.members.as_slice()
            && flags.untagged_union_strategy == UntaggedUnionStrategy::FirstMatch
        {
            let expr = if union.nullable {
                IrTypeExpr::Nullable(Box::new(member.clone()))
            } else {
                member.clone()
            };
            return Ok(emit_alias_file(schema, &expr, convertible, ts));
        }
        return emit_union_file_camel_case(
            schema,
            ir,
            union,
            &schema.name.to_pascal_case(),
            convertible,
            flags.untagged_union_strategy,
            ts,
        )
        .map(Some);
    }
    Ok(match &schema.kind {
        IrSchemaKind::Object(obj) => {
            emit_object_file(schema, obj, flags, convertible, unknown_aliases, ts)
        }
        IrSchemaKind::Enum(en) => emit_enum_file_from(schema, en, flags, ts),
        IrSchemaKind::Alias(expr) => emit_alias_file(schema, expr, convertible, ts),
        IrSchemaKind::Union(u) => emit_union_file(schema, u, ts),
        IrSchemaKind::Intersection(i) => {
            emit_intersection_file(schema, ir, i, flags, convertible, unknown_aliases, ts)
        }
        IrSchemaKind::TaggedUnion(tu) => {
            emit_tagged_union_file(schema, ir, tu, flags, convertible, unknown_aliases, ts)
        }
    })
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
    convertible: &HashSet<String>,
    unknown_aliases: &HashSet<String>,
    ts: &TypeScript,
) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();

    if flags.property_naming_camel_case {
        return emit_object_file_camel_case(schema, obj, &name, convertible, unknown_aliases, ts);
    }

    let mut tb = TypeSpec::builder(&name, TypeKind::Interface).visibility(Visibility::Public);
    if let Some(doc) = &schema.description {
        tb = tb.doc(doc);
    }

    for (_json_name, prop) in &obj.properties {
        tb = tb.add_field(build_field(prop, &schema.name, unknown_aliases)?);
    }
    if let Some(dictionary) =
        additional_properties_super_type(obj, &schema.name, false, convertible, unknown_aliases)
    {
        tb = tb.extends(dictionary);
    }

    let filename = format!("{}.ts", name);
    FileSpec::builder_with(&filename, ts.clone())
        .add_type(tb.build().ok()?)
        .build()
        .ok()
}

/// Emit an object file with dual types: `Name$Wire` (wire format) + `Name`
/// (camelCase ergonomic) plus `nameFromJSON` / `nameToJSON` converters.
fn emit_object_file_camel_case(
    schema: &IrSchema,
    obj: &IrObject,
    name: &str,
    convertible: &HashSet<String>,
    unknown_aliases: &HashSet<String>,
    ts: &TypeScript,
) -> Option<FileSpec> {
    let wire_name = format!("{}$Wire", name);
    let filename = format!("{}.ts", name);

    // --- $Wire interface (original wire-format property names) ---
    let mut wire_tb =
        TypeSpec::builder(&wire_name, TypeKind::Interface).visibility(Visibility::Public);
    if let Some(doc) = &schema.description {
        wire_tb = wire_tb.doc(doc);
    }
    for (_json_name, prop) in &obj.properties {
        wire_tb = wire_tb.add_field(build_field_wire(
            prop,
            &schema.name,
            convertible,
            unknown_aliases,
        )?);
    }
    if let Some(dictionary) =
        additional_properties_super_type(obj, &schema.name, true, convertible, unknown_aliases)
    {
        wire_tb = wire_tb.extends(dictionary);
    }

    // --- Ergonomic interface (camelCase property names) ---
    let mut ergo_tb = TypeSpec::builder(name, TypeKind::Interface).visibility(Visibility::Public);
    if let Some(doc) = &schema.description {
        ergo_tb = ergo_tb.doc(doc);
    }
    for (_json_name, prop) in &obj.properties {
        ergo_tb = ergo_tb.add_field(build_field_camel(prop, &schema.name, unknown_aliases)?);
    }
    if let Some(dictionary) =
        additional_properties_super_type(obj, &schema.name, false, convertible, unknown_aliases)
    {
        ergo_tb = ergo_tb.extends(dictionary);
    }

    // --- fromJSON / toJSON functions ---
    let from_json = build_json_converter(
        name,
        &wire_name,
        &schema.name,
        obj,
        convertible,
        unknown_aliases,
        JsonDirection::FromWire,
    )
    .expect("TypeScript object FromJSON converter builds");
    let to_json = build_json_converter(
        name,
        &wire_name,
        &schema.name,
        obj,
        convertible,
        unknown_aliases,
        JsonDirection::ToWire,
    )
    .expect("TypeScript object ToJSON converter builds");

    let mut fb = FileSpec::builder_with(&filename, ts.clone());
    fb = fb.add_type(wire_tb.build().ok()?);
    fb = fb.add_type(ergo_tb.build().ok()?);

    fb = fb.add_function(from_json);
    fb = fb.add_function(to_json);
    fb.build().ok()
}

/// A string index also covers the declared properties in TypeScript. Include
/// their types (and optional undefined) instead of making valid mixed schemas
/// uncompilable. Readonly<Record<...>> expresses the index structurally without
/// pretending an index signature is an ordinary named field.
fn additional_properties_super_type(
    obj: &IrObject,
    current_schema: &str,
    wire: bool,
    convertible: &HashSet<String>,
    unknown_aliases: &HashSet<String>,
) -> Option<TypeName> {
    let additional = obj.additional_properties.as_ref()?;
    let value_type = |expr: &IrTypeExpr| {
        if type_expr_collapses_to_unknown(expr, unknown_aliases) {
            TypeName::primitive("unknown")
        } else if wire {
            type_expr_to_typename_wire(expr, current_schema, convertible)
        } else {
            type_expr_to_typename_for_schema(expr, current_schema)
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
    convertible: &HashSet<String>,
    unknown_aliases: &HashSet<String>,
) -> Option<FieldSpec> {
    let field_name = if is_valid_ts_identifier(&prop.name) {
        prop.name.clone()
    } else {
        format!("'{}'", prop.name)
    };
    let inner_ty = type_expr_to_typename_wire(&prop.type_expr, current_schema, convertible);
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
    fb.build().ok()
}

/// Build a field for the ergonomic interface (camelCase property name).
fn build_field_camel(
    prop: &IrProperty,
    current_schema: &str,
    unknown_aliases: &HashSet<String>,
) -> Option<FieldSpec> {
    let camel = prop.name.to_lower_camel_case();
    let field_name = if is_valid_ts_identifier(&camel) {
        camel
    } else {
        format!("'{}'", camel)
    };
    let inner_ty = type_expr_to_typename_for_schema(&prop.type_expr, current_schema);
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
    fb.build().ok()
}

/// Like `type_expr_to_typename` but Named refs resolve to `Name$Wire` (only if convertible).
fn type_expr_to_typename_wire(
    expr: &IrTypeExpr,
    current_schema: &str,
    convertible: &HashSet<String>,
) -> TypeName {
    match expr {
        IrTypeExpr::Named(name) => {
            let ts_name = name.to_pascal_case();
            let rendered_name = if convertible.contains(name) {
                let wire_name = format!("{}$Wire", ts_name);
                wire_name
            } else {
                ts_name
            };
            named_ref_to_typename(name, &rendered_name, Some(current_schema))
        }
        IrTypeExpr::Array(inner) => TypeName::readonly_array(type_expr_to_typename_wire_nested(
            inner,
            current_schema,
            convertible,
        )),
        IrTypeExpr::Nullable(inner) => union_typename(vec![
            type_expr_to_typename_wire(inner, current_schema, convertible),
            TypeName::primitive("null"),
        ]),
        IrTypeExpr::Map(inner) => type_application(
            TypeName::primitive("Record"),
            vec![
                TypeName::primitive("string"),
                type_expr_to_typename_wire(inner, current_schema, convertible),
            ],
        ),
        IrTypeExpr::StringLiteral(s) => TypeName::string_literal(s.clone()),
        IrTypeExpr::StringEnum(values) => union_typename(
            values
                .iter()
                .map(|v| TypeName::string_literal(v.clone()))
                .collect(),
        ),
        IrTypeExpr::Union(members) => union_typename(
            members
                .iter()
                .map(|member| type_expr_to_typename_wire(member, current_schema, convertible))
                .collect(),
        ),
        IrTypeExpr::Primitive(p) => TypeName::primitive(primitive_to_ts(p)),
        IrTypeExpr::Any => TypeName::primitive("unknown"),
    }
}

fn type_expr_to_typename_wire_nested(
    expr: &IrTypeExpr,
    current_schema: &str,
    convertible: &HashSet<String>,
) -> TypeName {
    match expr {
        IrTypeExpr::Array(inner) => TypeName::array(type_expr_to_typename_wire_nested(
            inner,
            current_schema,
            convertible,
        )),
        other => type_expr_to_typename_wire(other, current_schema, convertible),
    }
}

/// Render a type expression as a plain TS string (ergonomic / PascalCase names).
fn type_expr_str(expr: &IrTypeExpr) -> String {
    match expr {
        IrTypeExpr::Named(name) => name.to_pascal_case(),
        IrTypeExpr::Primitive(p) => primitive_to_ts(p).to_string(),
        IrTypeExpr::StringLiteral(s) => format!("'{s}'"),
        IrTypeExpr::StringEnum(values) => {
            simplify_union_strings(values.iter().map(|v| format!("'{v}'")).collect())
        }
        IrTypeExpr::Array(inner) => {
            let inner_str = type_expr_str(inner);
            if is_compound_type(inner) {
                format!("readonly ({inner_str})[]")
            } else {
                format!("readonly {inner_str}[]")
            }
        }
        IrTypeExpr::Nullable(inner) => {
            simplify_union_strings(vec![type_expr_str(inner), "null".to_string()])
        }
        IrTypeExpr::Map(inner) => format!("Record<string, {}>", type_expr_str(inner)),
        IrTypeExpr::Union(members) => {
            simplify_union_strings(members.iter().map(type_expr_str).collect())
        }
        IrTypeExpr::Any => "unknown".to_string(),
    }
}

/// Render a type expression as a plain TS string ($Wire variant — Named → Name$Wire only if convertible).
fn type_expr_str_wire(expr: &IrTypeExpr, convertible: &HashSet<String>) -> String {
    match expr {
        IrTypeExpr::Named(name) => {
            let pascal = name.to_pascal_case();
            if convertible.contains(name) {
                format!("{pascal}$Wire")
            } else {
                pascal
            }
        }
        IrTypeExpr::Primitive(p) => primitive_to_ts(p).to_string(),
        IrTypeExpr::StringLiteral(s) => format!("'{s}'"),
        IrTypeExpr::StringEnum(values) => {
            simplify_union_strings(values.iter().map(|v| format!("'{v}'")).collect())
        }
        IrTypeExpr::Array(inner) => {
            let inner_str = type_expr_str_wire(inner, convertible);
            if is_compound_type(inner) {
                format!("readonly ({inner_str})[]")
            } else {
                format!("readonly {inner_str}[]")
            }
        }
        IrTypeExpr::Nullable(inner) => simplify_union_strings(vec![
            type_expr_str_wire(inner, convertible),
            "null".to_string(),
        ]),
        IrTypeExpr::Map(inner) => {
            format!("Record<string, {}>", type_expr_str_wire(inner, convertible))
        }
        IrTypeExpr::Union(members) => simplify_union_strings(
            members
                .iter()
                .map(|m| type_expr_str_wire(m, convertible))
                .collect(),
        ),
        IrTypeExpr::Any => "unknown".to_string(),
    }
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

fn union_typename(members: Vec<TypeName>) -> TypeName {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for member in members {
        if type_name_is_unknown(&member) {
            return TypeName::primitive("unknown");
        }
        let key = format!("{member:?}");
        if seen.insert(key) {
            out.push(member);
        }
    }
    if out.len() == 1 {
        out.pop().unwrap()
    } else {
        TypeName::union(out)
    }
}

fn type_name_is_unknown(type_name: &TypeName) -> bool {
    match type_name {
        TypeName::Primitive(value) | TypeName::Raw(value) => value == "unknown",
        TypeName::Union(members) => members.iter().any(type_name_is_unknown),
        _ => false,
    }
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

fn is_compound_type(expr: &IrTypeExpr) -> bool {
    matches!(
        expr,
        IrTypeExpr::Nullable(_) | IrTypeExpr::Union(_) | IrTypeExpr::StringEnum(_)
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum JsonDirection {
    FromWire,
    ToWire,
}

impl JsonDirection {
    fn source(self) -> &'static str {
        match self {
            Self::FromWire => "json",
            Self::ToWire => "value",
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            Self::FromWire => "FromJSON",
            Self::ToWire => "ToJSON",
        }
    }

    fn source_key(self, prop: &IrProperty) -> String {
        match self {
            Self::FromWire => prop.name.clone(),
            Self::ToWire => prop.name.to_lower_camel_case(),
        }
    }

    fn target_key(self, prop: &IrProperty) -> String {
        match self {
            Self::FromWire => prop.name.to_lower_camel_case(),
            Self::ToWire => prop.name.clone(),
        }
    }
}

fn build_json_converter(
    name: &str,
    wire_name: &str,
    current_schema: &str,
    obj: &IrObject,
    convertible: &HashSet<String>,
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
            convertible,
            direction,
        )?;
        if prop.nullable && needs_conversion(&prop.type_expr, convertible) {
            value = sigil_quote!(TypeScript {
                $C(access.clone()) === null ? null : $C(value)
            })?;
        }
        if !prop.required && needs_conversion(&prop.type_expr, convertible) {
            value = sigil_quote!(TypeScript {
                $C(access) !== undefined ? $C(value) : undefined
            })?;
        }
        let field = if target_key == "__proto__" {
            // A literal __proto__ property changes an object literal's prototype.
            sigil_quote!(TypeScript { [$S(target_key)]: $C(value), })?
        } else if is_unescaped_ts_property(&target_key) {
            sigil_quote!(TypeScript { $N(target_key): $C(value), })?
        } else {
            sigil_quote!(TypeScript { $S(target_key): $C(value), })?
        };
        fields.push(if !prop.required && obj.additional_properties.is_some() {
            sigil_quote!(TypeScript {
                ...(Object.prototype.hasOwnProperty.call($N(source), $S(source_key)) ? {
                    $C(field)
                } : {}),
            })?
        } else {
            field
        });
    }
    let body = if obj.properties.is_empty()
        && !obj
            .additional_properties
            .as_ref()
            .is_some_and(|ty| needs_conversion(ty, convertible))
    {
        sigil_quote!(TypeScript { return $N(source) as unknown as $T(TypeName::primitive(output)); })?
    } else if let Some(additional) = &obj.additional_properties {
        build_open_object_conversion(
            obj,
            additional,
            fields,
            current_schema,
            convertible,
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
    convertible: &HashSet<String>,
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
        JsonDirection::FromWire => {
            type_expr_to_typename_wire(additional, current_schema, convertible)
        }
        JsonDirection::ToWire => type_expr_to_typename_for_schema(additional, current_schema),
    };
    let retained_type = union_typename(vec![
        type_application(
            TypeName::primitive("Record"),
            vec![TypeName::primitive("string"), additional_type.clone()],
        ),
        TypeName::primitive("undefined"),
    ]);
    let entries = sigil_quote!(TypeScript {
        Object.entries($N(source))
        $if(!source_keys.is_empty()) {
            .filter(([key]) => ![$for(key in &source_keys; separator = ", ") { $S(key) }].includes(key))
        }
    })?;
    let entries = if restore {
        sigil_quote!(TypeScript {
            Object.entries({ ...Object.fromEntries($C(entries)), ...retainedProperties })
        })?
    } else {
        entries
    };
    let cast_additional =
        !obj.properties.is_empty() && !type_expr_collapses_to_unknown(additional, unknown_aliases);
    let converted = json_conversion_expr(
        additional,
        sigil_quote!(TypeScript { additionalValue })?,
        current_schema,
        convertible,
        direction,
    )?;
    let visible = if retain {
        sigil_quote!(TypeScript {
            Object.fromEntries(Object.entries(additionalProperties).filter(([key]) => ![$for(key in &target_keys; separator = ", ") { $S(key) }].includes(key)))
        })?
    } else {
        sigil_quote!(TypeScript {
            additionalProperties
        })?
    };
    // An extension may have the same spelling as a renamed declared field.
    // Keep the declared property usable and retain the shadowed extension in
    // enumerable symbol metadata: object spread preserves it, JSON does not.
    sigil_quote!(TypeScript {
        $if(restore) {
            const retainedProperties = Object.getOwnPropertyDescriptor($N(source), Symbol.for($S(metadata_key)))?.value as $T(retained_type);
        }
        const additionalProperties = Object.fromEntries($C(entries).map(([key, value]) => {
            const additionalValue = $if(cast_additional) { value as $T(additional_type) } $else { value };
            return [key, $C(converted)] as const;
        }));
        const result = {
            ...$C(visible),
            $C_each(fields)
        };
        $if(retain) {
            const shadowedProperties = Object.fromEntries(Object.entries(additionalProperties).filter(([key]) => [$for(key in &target_keys; separator = ", ") { $S(key) }].includes(key)));
            if (Object.keys(shadowedProperties).length > 0) {
                Object.defineProperty(result, Symbol.for($S(metadata_key)), {
                    value: shadowedProperties,
                    enumerable: true,
                });
            }
        }
        return result;
    })
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
    convertible: &HashSet<String>,
    direction: JsonDirection,
) -> Result<CodeBlock, SigilStitchError> {
    match expr {
        IrTypeExpr::Named(name) if convertible.contains(name) => {
            let pascal = name.to_pascal_case();
            let function_name = format!("{}{}", fn_base_name(&pascal), direction.suffix());
            let function = if name == current_schema {
                TypeName::primitive(&function_name)
            } else {
                TypeName::importable(&format!("./{pascal}"), &function_name)
            };
            sigil_quote!(TypeScript { $T(function)($C(access)) })
        }
        IrTypeExpr::Array(inner) if has_named_ref(inner, convertible) => {
            let item = json_conversion_expr(
                inner,
                sigil_quote!(TypeScript { item })?,
                current_schema,
                convertible,
                direction,
            )?;
            sigil_quote!(TypeScript { $C(access).map((item) => $C(item)) })
        }
        IrTypeExpr::Nullable(inner) if has_named_ref(inner, convertible) => {
            let value = json_conversion_expr(
                inner,
                access.clone(),
                current_schema,
                convertible,
                direction,
            )?;
            sigil_quote!(TypeScript { $C(access) != null ? $C(value) : null })
        }
        IrTypeExpr::Map(inner) if has_named_ref(inner, convertible) => {
            let value = json_conversion_expr(
                inner,
                sigil_quote!(TypeScript { v })?,
                current_schema,
                convertible,
                direction,
            )?;
            sigil_quote!(TypeScript {
                Object.fromEntries(Object.entries($C(access) ?? {}).map(([k, v]) => [k, $C(value)]))
            })
        }
        _ => Ok(access),
    }
}

fn from_json_expr_inner(expr: &IrTypeExpr, access: &str, convertible: &HashSet<String>) -> String {
    match expr {
        IrTypeExpr::Named(ref_name) if convertible.contains(ref_name) => {
            let pascal = ref_name.to_pascal_case();
            let converter = format!("{}FromJSON", fn_base_name(&pascal));
            format!("{converter}({access})")
        }
        IrTypeExpr::Array(inner) if has_named_ref(inner, convertible) => {
            let item_expr = from_json_expr_inner(inner, "item", convertible);
            format!("{access}.map((item) => {item_expr})")
        }
        IrTypeExpr::Nullable(inner) if has_named_ref(inner, convertible) => {
            let inner_expr = from_json_expr_inner(inner, access, convertible);
            format!("{access} != null ? {inner_expr} : null")
        }
        IrTypeExpr::Map(inner) if has_named_ref(inner, convertible) => {
            let val_expr = from_json_expr_inner(inner, "v", convertible);
            format!(
                "Object.fromEntries(Object.entries({access} ?? {{}}).map(([k, v]) => [k, {val_expr}]))"
            )
        }
        _ => access.to_string(),
    }
}

fn to_json_expr_inner(expr: &IrTypeExpr, access: &str, convertible: &HashSet<String>) -> String {
    match expr {
        IrTypeExpr::Named(ref_name) if convertible.contains(ref_name) => {
            let pascal = ref_name.to_pascal_case();
            let converter = format!("{}ToJSON", fn_base_name(&pascal));
            format!("{converter}({access})")
        }
        IrTypeExpr::Array(inner) if has_named_ref(inner, convertible) => {
            let item_expr = to_json_expr_inner(inner, "item", convertible);
            format!("{access}.map((item) => {item_expr})")
        }
        IrTypeExpr::Nullable(inner) if has_named_ref(inner, convertible) => {
            let inner_expr = to_json_expr_inner(inner, access, convertible);
            format!("{access} != null ? {inner_expr} : null")
        }
        IrTypeExpr::Map(inner) if has_named_ref(inner, convertible) => {
            let val_expr = to_json_expr_inner(inner, "v", convertible);
            format!(
                "Object.fromEntries(Object.entries({access} ?? {{}}).map(([k, v]) => [k, {val_expr}]))"
            )
        }
        _ => access.to_string(),
    }
}

/// Returns true if the type expression contains a Named reference that needs conversion.
fn needs_conversion(expr: &IrTypeExpr, convertible: &HashSet<String>) -> bool {
    match expr {
        IrTypeExpr::Named(name) => convertible.contains(name),
        IrTypeExpr::Array(inner) => has_named_ref(inner, convertible),
        IrTypeExpr::Nullable(inner) => has_named_ref(inner, convertible),
        IrTypeExpr::Map(inner) => has_named_ref(inner, convertible),
        _ => false,
    }
}

/// Returns true if a type expression (recursively) contains a convertible Named ref.
fn has_named_ref(expr: &IrTypeExpr, convertible: &HashSet<String>) -> bool {
    match expr {
        IrTypeExpr::Named(name) => convertible.contains(name),
        IrTypeExpr::Array(inner) | IrTypeExpr::Nullable(inner) | IrTypeExpr::Map(inner) => {
            has_named_ref(inner, convertible)
        }
        _ => false,
    }
}

fn contains_any_named_ref(expr: &IrTypeExpr) -> bool {
    match expr {
        IrTypeExpr::Named(_) => true,
        IrTypeExpr::Array(inner) | IrTypeExpr::Nullable(inner) | IrTypeExpr::Map(inner) => {
            contains_any_named_ref(inner)
        }
        IrTypeExpr::Union(members) => members.iter().any(contains_any_named_ref),
        IrTypeExpr::Primitive(_)
        | IrTypeExpr::StringLiteral(_)
        | IrTypeExpr::StringEnum(_)
        | IrTypeExpr::Any => false,
    }
}

fn type_expr_collapses_to_unknown(expr: &IrTypeExpr, unknown_aliases: &HashSet<String>) -> bool {
    match expr {
        IrTypeExpr::Any => true,
        IrTypeExpr::Named(name) => unknown_aliases.contains(name),
        IrTypeExpr::Nullable(inner) => type_expr_collapses_to_unknown(inner, unknown_aliases),
        IrTypeExpr::Union(members) => members
            .iter()
            .any(|member| type_expr_collapses_to_unknown(member, unknown_aliases)),
        IrTypeExpr::Array(_)
        | IrTypeExpr::Map(_)
        | IrTypeExpr::Primitive(_)
        | IrTypeExpr::StringLiteral(_)
        | IrTypeExpr::StringEnum(_) => false,
    }
}

fn build_unknown_alias_set(ir: &IrSpec) -> HashSet<String> {
    ir.schemas
        .iter()
        .filter_map(|(name, schema)| {
            let renders_unknown = match &schema.kind {
                IrSchemaKind::Alias(expr) => type_expr_str(expr) == "unknown",
                IrSchemaKind::Union(union) => {
                    let mut members: Vec<String> =
                        union.members.iter().map(type_expr_str).collect();
                    if union.nullable {
                        members.push("null".to_string());
                    }
                    simplify_union_strings(members) == "unknown"
                }
                _ => false,
            };
            renders_unknown.then(|| name.clone())
        })
        .collect()
}

fn collect_named_refs_from_expr(
    expr: &IrTypeExpr,
    refs: &mut HashSet<String>,
    convertible: &HashSet<String>,
) {
    match expr {
        IrTypeExpr::Named(name) => {
            if convertible.contains(name) {
                refs.insert(name.clone());
            }
        }
        IrTypeExpr::Array(inner) | IrTypeExpr::Nullable(inner) | IrTypeExpr::Map(inner) => {
            collect_named_refs_from_expr(inner, refs, convertible);
        }
        IrTypeExpr::Union(members) => {
            for m in members {
                collect_named_refs_from_expr(m, refs, convertible);
            }
        }
        _ => {}
    }
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
    convertible: &HashSet<String>,
    ts: &TypeScript,
) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();
    if convertible.contains(&schema.name) {
        let wire_name = format!("{name}$Wire");
        let mut file = FileSpec::builder_with(&format!("{name}.ts"), ts.clone())
            .add_type(
                build_type_alias(
                    &wire_name,
                    type_expr_to_typename_wire(expr, &schema.name, convertible),
                    schema.description.as_deref(),
                )
                .expect("convertible wire type alias builds"),
            )
            .add_type(
                build_type_alias(
                    &name,
                    type_expr_to_typename_for_schema(expr, &schema.name),
                    schema.description.as_deref(),
                )
                .expect("convertible ergonomic type alias builds"),
            );
        for direction in [JsonDirection::FromWire, JsonDirection::ToWire] {
            let source = direction.source();
            let (input, output) = match direction {
                JsonDirection::FromWire => (&wire_name, &name),
                JsonDirection::ToWire => (&name, &wire_name),
            };
            let value = json_conversion_expr(
                expr,
                sigil_quote!(TypeScript { $N(source) }).expect("alias input builds"),
                &schema.name,
                convertible,
                direction,
            )
            .expect("alias JSON conversion expression builds");
            file = file.add_function(
                FunSpec::builder(&format!("{}{}", fn_base_name(&name), direction.suffix()))
                    .visibility(Visibility::Public)
                    .add_param(ParameterSpec::of(source, TypeName::primitive(input)))
                    .returns(vec![TypeName::primitive(output)])
                    .body(
                        sigil_quote!(TypeScript { return $C(value); })
                            .expect("alias JSON converter body builds"),
                    )
                    .build()
                    .expect("alias JSON converter builds"),
            );
        }
        return file.build().ok();
    }
    let rhs = type_expr_to_typename(expr);

    let type_alias = sigil_quote!(TypeScript {
        export type $N(name.as_str()) = $T(rhs);
    })
    .ok()?;

    let filename = format!("{}.ts", name);
    let mut fb = FileSpec::builder_with(&filename, ts.clone());
    if let Some(doc) = &schema.description {
        fb = fb.add_raw(&format!("/** {doc} */\n"));
    }
    fb.add_code(type_alias).build().ok()
}

fn build_type_alias(
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
fn emit_union_file(schema: &IrSchema, union: &IrUnion, ts: &TypeScript) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();

    let mut members: Vec<TypeName> = union.members.iter().map(type_expr_to_typename).collect();
    if union.nullable {
        members.push(TypeName::primitive("null"));
    }
    let union_ty = union_typename(members);

    let type_alias = sigil_quote!(TypeScript {
        export type $N(name.as_str()) = $T(union_ty);
    })
    .ok()?;

    let filename = format!("{}.ts", name);
    let mut fb = FileSpec::builder_with(&filename, ts.clone());
    if let Some(doc) = &schema.description {
        fb = fb.add_raw(&format!("/** {doc} */\n"));
    }
    fb.add_code(type_alias).build().ok()
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
    convertible: &HashSet<String>,
    unknown_aliases: &HashSet<String>,
    ts: &TypeScript,
) -> Option<FileSpec> {
    if intersection.members.is_empty() {
        return None;
    }
    let name = schema.name.to_pascal_case();

    if flags.property_naming_camel_case && convertible.contains(&schema.name) {
        return emit_intersection_file_camel_case(
            schema,
            ir,
            intersection,
            &name,
            convertible,
            unknown_aliases,
            ts,
        );
    }

    let members: Vec<TypeName> = intersection
        .members
        .iter()
        .map(type_expr_to_typename)
        .collect();
    let inter_ty = TypeName::intersection(members);
    let inter_ty = if open_intersection_object(intersection, ir, unknown_aliases).is_some() {
        type_application(TypeName::primitive("Readonly"), vec![inter_ty])
    } else {
        inter_ty
    };

    let type_alias = sigil_quote!(TypeScript {
        export type $N(name.as_str()) = $T(inter_ty);
    })
    .expect("intersection type alias builds");

    let filename = format!("{}.ts", name);
    let mut fb = FileSpec::builder_with(&filename, ts.clone());
    if let Some(doc) = &schema.description {
        // FileSpec has no structural doc slot for a code member.
        fb = fb.add_raw(&format!("/** {doc} */\n"));
    }
    fb.add_code(type_alias).build().ok()
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
    convertible: &HashSet<String>,
    unknown_aliases: &HashSet<String>,
    ts: &TypeScript,
) -> Option<FileSpec> {
    if tu.variants.is_empty() {
        return None;
    }

    let name = schema.name.to_pascal_case();

    if matches!(tu.tagging, TaggingStyle::External)
        && tu
            .variants
            .iter()
            .any(|variant| variant.additional_properties.is_some())
    {
        return emit_open_external_union_file(
            schema,
            ir,
            tu,
            flags,
            convertible,
            unknown_aliases,
            ts,
        );
    }

    if flags.property_naming_camel_case {
        return emit_tagged_union_file_camel_case(schema, ir, tu, &name, flags, convertible, ts);
    }

    let code =
        tagged_union_type_alias_code(&name, ir, tu, &tu.discriminator_field, None, |expr| {
            type_expr_str(expr)
        })?;

    let filename = format!("{}.ts", name);
    let mut fb = FileSpec::builder_with(&filename, ts.clone());
    if let Some(doc) = &schema.description {
        fb = fb.add_raw(&format!("/** {doc} */\n"));
    }
    fb = fb.add_code(code);
    for import in tagged_union_type_imports(ir, tu, convertible, false) {
        fb = fb.add_import(import);
    }

    if flags.emit_type_guards {
        let guards = build_tagged_union_type_guards(&name, ir, tu, false);
        for guard in guards {
            fb = fb.add_code(guard);
        }
    }

    fb.build().ok()
}

/// External tags are wire keys in both naming modes. Keep their outer maps
/// separate from the referenced content model so nested fields are converted
/// without treating the wrapper's extensions as part of that model.
fn emit_open_external_union_file(
    schema: &IrSchema,
    ir: &IrSpec,
    union: &IrTaggedUnion,
    flags: EmitFlags,
    convertible: &HashSet<String>,
    unknown_aliases: &HashSet<String>,
    ts: &TypeScript,
) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();
    let wire_name = format!("{name}$Wire");
    let value_type = |expr: &IrTypeExpr, wire: bool| {
        if type_expr_collapses_to_unknown(expr, unknown_aliases) {
            TypeName::primitive("unknown")
        } else if wire {
            type_expr_to_typename_wire(expr, &schema.name, convertible)
        } else {
            type_expr_to_typename_for_schema(expr, &schema.name)
        }
    };
    let wrapper_type = |wire: bool| {
        union_typename(
            union
                .variants
                .iter()
                .map(|variant| {
                    let content = value_type(&variant.content_type, wire);
                    let required = type_application(
                        TypeName::primitive("Readonly"),
                        vec![type_application(
                            TypeName::primitive("Record"),
                            vec![
                                TypeName::string_literal(variant.discriminator_value.clone()),
                                content.clone(),
                            ],
                        )],
                    );
                    if let Some(additional) = &variant.additional_properties {
                        TypeName::intersection(vec![
                            required,
                            type_application(
                                TypeName::primitive("Readonly"),
                                vec![type_application(
                                    TypeName::primitive("Record"),
                                    vec![
                                        TypeName::primitive("string"),
                                        union_typename(vec![value_type(additional, wire), content]),
                                    ],
                                )],
                            ),
                        ])
                    } else {
                        required
                    }
                })
                .collect(),
        )
    };
    let mut file = FileSpec::builder_with(&format!("{name}.ts"), ts.clone());
    if flags.property_naming_camel_case {
        file = file.add_type(
            build_type_alias(
                &wire_name,
                wrapper_type(true),
                schema.description.as_deref(),
            )
            .expect("external wrapper wire alias builds"),
        );
    }
    file = file.add_type(
        build_type_alias(&name, wrapper_type(false), schema.description.as_deref())
            .expect("external wrapper ergonomic alias builds"),
    );

    if flags.property_naming_camel_case {
        for direction in [JsonDirection::FromWire, JsonDirection::ToWire] {
            let source = direction.source();
            let wire = direction == JsonDirection::FromWire;
            let (input, output) = if wire {
                (&wire_name, &name)
            } else {
                (&name, &wire_name)
            };
            let branches: Vec<_> = union.variants.iter().map(|variant| {
                let key = &variant.discriminator_value;
                // hasOwnProperty does not narrow a union containing a closed
                // alternative. Assert only the key just checked, not an index
                // signature on the entire wrapper.
                let content_record = type_application(
                    TypeName::primitive("Record"),
                    vec![TypeName::string_literal(key.clone()), value_type(&variant.content_type, wire)],
                );
                let content = json_conversion_expr(
                    &variant.content_type,
                    sigil_quote!(TypeScript { contentValue }).expect("external content input builds"),
                    &schema.name, convertible, direction,
                ).expect("external content conversion builds");
                let additional = variant.additional_properties.as_ref().map(|expr| {
                    let ty = value_type(expr, wire);
                    let cast = !type_expr_collapses_to_unknown(expr, unknown_aliases);
                    let converted = json_conversion_expr(
                        expr,
                        sigil_quote!(TypeScript { additionalValue }).expect("external extension input builds"),
                        &schema.name, convertible, direction,
                    ).expect("external extension conversion builds");
                    sigil_quote!(TypeScript {
                        const additionalProperties = Object.fromEntries(Object.entries($N(source))
                            .filter(([key]) => key !== $S(key))
                            .map(([key, value]) => {
                                const additionalValue = $if(cast) { value as $T(ty) } $else { value };
                                return [key, $C(converted)] as const;
                            }));
                    }).expect("external extension collection builds")
                });
                sigil_quote!(TypeScript {
                    if (Object.prototype.hasOwnProperty.call($N(source), $S(key))) {
                        const contentValue = ($N(source) as $T(content_record))[$S(key)];
                        $if(let Some(additional) = additional) { $C(additional) }
                        return {
                            $if(variant.additional_properties.is_some()) { ...additionalProperties, }
                            [$S(key)]: $C(content),
                        };
                    }
                }).expect("external wrapper conversion branch builds")
            }).collect();
            let body = sigil_quote!(TypeScript {
                $C_each(branches)
                throw new Error($S("Unknown variant"));
            })
            .expect("external wrapper converter body builds");
            file = file.add_function(
                FunSpec::builder(&format!("{}{}", fn_base_name(&name), direction.suffix()))
                    .visibility(Visibility::Public)
                    .add_param(ParameterSpec::of(source, TypeName::primitive(input)))
                    .returns(vec![TypeName::primitive(output)])
                    .body(body)
                    .build()
                    .expect("external wrapper JSON converter builds"),
            );
        }
    }
    if flags.emit_type_guards {
        for guard in
            build_tagged_union_type_guards(&name, ir, union, flags.property_naming_camel_case)
        {
            file = file.add_code(guard);
        }
    }
    file.build().ok()
}

/// Emit a tagged union file with dual types + fromJSON/toJSON when camelCase.
fn emit_tagged_union_file_camel_case(
    schema: &IrSchema,
    ir: &IrSpec,
    tu: &IrTaggedUnion,
    name: &str,
    flags: EmitFlags,
    convertible: &HashSet<String>,
    ts: &TypeScript,
) -> Option<FileSpec> {
    let wire_name = format!("{}$Wire", name);
    let filename = format!("{}.ts", name);
    let disc_field = &tu.discriminator_field;
    let disc_field_camel = disc_field.to_lower_camel_case();

    // --- $Wire type (wire discriminator field name + $Wire content refs) ---
    let wire_code = tagged_union_type_alias_code(&wire_name, ir, tu, disc_field, None, |expr| {
        type_expr_str_wire(expr, convertible)
    })?;

    // --- Ergonomic type (camelCase discriminator field name + ergonomic content refs) ---
    let ergo_code = tagged_union_type_alias_code(
        name,
        ir,
        tu,
        &disc_field_camel,
        Some(PropertyNamingMode::CamelCase),
        type_expr_str,
    )?;

    // --- fromJSON / toJSON ---
    let from_json = build_tagged_union_from_json(name, &wire_name, ir, tu, convertible)?;
    let to_json = build_tagged_union_to_json(name, &wire_name, ir, tu, convertible)?;

    let mut fb = FileSpec::builder_with(&filename, ts.clone());
    if let Some(doc) = &schema.description {
        fb = fb.add_raw(&format!("/** {doc} */\n"));
    }
    fb = fb.add_code(wire_code);
    fb = fb.add_code(ergo_code);
    for import in tagged_union_type_imports(ir, tu, convertible, true) {
        fb = fb.add_import(import);
    }

    // Add converter imports for Named content types (only if convertible)
    let mut converter_refs = HashSet::new();
    for variant in &tu.variants {
        if is_tag_only_variant(
            ir,
            &tu.discriminator_field,
            &variant.discriminator_value,
            &variant.content_type,
        ) {
            continue;
        }
        collect_named_refs_from_expr(&variant.content_type, &mut converter_refs, convertible);
    }
    let mut converter_refs: Vec<String> = converter_refs.into_iter().collect();
    converter_refs.sort();
    for ref_name in converter_refs {
        let pascal = ref_name.to_pascal_case();
        let module = format!("./{pascal}");
        let base = fn_base_name(&pascal);
        let from_fn = format!("{base}FromJSON");
        let to_fn = format!("{base}ToJSON");
        fb = fb.add_import(ImportSpec::named(&module, &from_fn));
        fb = fb.add_import(ImportSpec::named(&module, &to_fn));
    }

    fb = fb.add_code(from_json);
    fb = fb.add_code(to_json);

    if flags.emit_type_guards {
        let guards = build_tagged_union_type_guards(name, ir, tu, true);
        for guard in guards {
            fb = fb.add_code(guard);
        }
    }

    fb.build().ok()
}

/// Build fromJSON for a tagged union: switch on the wire discriminator field.
fn build_tagged_union_from_json(
    name: &str,
    wire_name: &str,
    ir: &IrSpec,
    tu: &IrTaggedUnion,
    convertible: &HashSet<String>,
) -> Option<CodeBlock> {
    let fn_name = format!("{}FromJSON", fn_base_name(name));

    if matches!(tu.tagging, TaggingStyle::External) {
        return build_external_tagged_from_json(&fn_name, name, wire_name, tu, convertible);
    }

    let disc_field = &tu.discriminator_field;
    let disc_field_camel = disc_field.to_lower_camel_case();
    let disc_access = if is_valid_ts_identifier(disc_field) {
        format!("json.{disc_field}")
    } else {
        format!("json['{disc_field}']")
    };

    let mut case_lines: Vec<String> = Vec::new();
    for variant in &tu.variants {
        let val = &variant.discriminator_value;
        let case_body = internal_or_adjacent_from_json_case(
            &tu.tagging,
            val,
            &disc_field_camel,
            &variant.content_type,
            ir,
            &tu.discriminator_field,
            convertible,
        );
        case_lines.push(format!("case '{val}': {case_body}"));
    }
    sigil_quote!(TypeScript {
        export function $N(fn_name)(json: $N(wire_name)): $N(name) {
          switch ($L(disc_access)) {
        $for(case_line in &case_lines) {
            $L(case_line.as_str())
        }
          }
        }
    })
    .ok()
}

fn internal_or_adjacent_from_json_case(
    tagging: &TaggingStyle,
    val: &str,
    disc_field_camel: &str,
    content_type: &IrTypeExpr,
    ir: &IrSpec,
    disc_field: &str,
    convertible: &HashSet<String>,
) -> String {
    match tagging {
        TaggingStyle::Internal => {
            if is_tag_only_variant(ir, disc_field, val, content_type) {
                return format!("return {{ {disc_field_camel}: '{val}' }};");
            }
            if let IrTypeExpr::Named(ref_name) = content_type {
                if convertible.contains(ref_name) {
                    let pascal = ref_name.to_pascal_case();
                    let converter = format!("{}FromJSON", fn_base_name(&pascal));
                    format!("return {{ ...{converter}(json), {disc_field_camel}: '{val}' }};")
                } else {
                    format!("return {{ ...json, {disc_field_camel}: '{val}' }};")
                }
            } else {
                format!("return {{ ...json, {disc_field_camel}: '{val}' }};")
            }
        }
        TaggingStyle::Adjacent { content_field } => {
            if is_tag_only_variant(ir, disc_field, val, content_type) {
                return format!("return {{ {disc_field_camel}: '{val}' }};");
            }
            let content_camel = content_field.to_lower_camel_case();
            let content_access = if is_valid_ts_identifier(content_field) {
                format!("json.{content_field}")
            } else {
                format!("json['{content_field}']")
            };
            let content_expr = from_json_expr_inner(content_type, &content_access, convertible);
            format!("return {{ {disc_field_camel}: '{val}', {content_camel}: {content_expr} }};")
        }
        TaggingStyle::External => unreachable!(),
    }
}

fn build_external_tagged_from_json(
    fn_name: &str,
    name: &str,
    wire_name: &str,
    tu: &IrTaggedUnion,
    convertible: &HashSet<String>,
) -> Option<CodeBlock> {
    let mut body_lines: Vec<String> = Vec::new();
    for variant in &tu.variants {
        let val = &variant.discriminator_value;
        let ret_expr = external_variant_from_json_expr(val, &variant.content_type, convertible);
        body_lines.push(format!("if ('{val}' in json) return {ret_expr};"));
    }
    body_lines.push("throw new Error('Unknown variant');".to_string());
    sigil_quote!(TypeScript {
        export function $N(fn_name)(json: $N(wire_name)): $N(name) {
        $for(body_line in &body_lines) {
            $L(body_line.as_str())
        }
        }
    })
    .ok()
}

fn external_variant_from_json_expr(
    val: &str,
    content_type: &IrTypeExpr,
    convertible: &HashSet<String>,
) -> String {
    let val_access = format!("json['{val}']");
    let content_expr = from_json_expr_inner(content_type, &val_access, convertible);
    format!("{{ '{val}': {content_expr} }}")
}

/// Build toJSON for a tagged union: switch on the camelCase discriminator field.
fn build_tagged_union_to_json(
    name: &str,
    wire_name: &str,
    ir: &IrSpec,
    tu: &IrTaggedUnion,
    convertible: &HashSet<String>,
) -> Option<CodeBlock> {
    let fn_name = format!("{}ToJSON", fn_base_name(name));

    if matches!(tu.tagging, TaggingStyle::External) {
        return build_external_tagged_to_json(&fn_name, name, wire_name, tu, convertible);
    }

    let disc_field = &tu.discriminator_field;
    let disc_field_camel = disc_field.to_lower_camel_case();
    let disc_access = if is_valid_ts_identifier(&disc_field_camel) {
        format!("value.{disc_field_camel}")
    } else {
        format!("value['{disc_field_camel}']")
    };
    let disc_wire_key = if is_valid_ts_identifier(disc_field) {
        disc_field.clone()
    } else {
        format!("'{disc_field}'")
    };

    let mut case_lines: Vec<String> = Vec::new();
    for variant in &tu.variants {
        let val = &variant.discriminator_value;
        let tag_only = is_tag_only_variant(ir, &tu.discriminator_field, val, &variant.content_type);
        let case_body = internal_or_adjacent_to_json_case(
            &tu.tagging,
            val,
            &disc_wire_key,
            wire_name,
            &variant.content_type,
            tag_only,
            convertible,
        );
        case_lines.push(format!("case '{val}': {case_body}"));
    }
    sigil_quote!(TypeScript {
        export function $N(fn_name)(value: $N(name)): $N(wire_name) {
          switch ($L(disc_access)) {
        $for(case_line in &case_lines) {
            $L(case_line.as_str())
        }
          }
        }
    })
    .ok()
}

fn internal_or_adjacent_to_json_case(
    tagging: &TaggingStyle,
    val: &str,
    disc_wire_key: &str,
    wire_name: &str,
    content_type: &IrTypeExpr,
    tag_only: bool,
    convertible: &HashSet<String>,
) -> String {
    match tagging {
        TaggingStyle::Internal => {
            if tag_only {
                return format!("return {{ {disc_wire_key}: '{val}' }} as {wire_name};");
            }
            if let IrTypeExpr::Named(ref_name) = content_type {
                if convertible.contains(ref_name) {
                    let pascal = ref_name.to_pascal_case();
                    let converter = format!("{}ToJSON", fn_base_name(&pascal));
                    format!(
                        "return {{ ...{converter}(value), {disc_wire_key}: '{val}' }} as {wire_name};"
                    )
                } else {
                    format!("return {{ ...value, {disc_wire_key}: '{val}' }} as {wire_name};")
                }
            } else {
                format!("return {{ ...value, {disc_wire_key}: '{val}' }} as {wire_name};")
            }
        }
        TaggingStyle::Adjacent { content_field } => {
            if tag_only {
                return format!("return {{ {disc_wire_key}: '{val}' }};");
            }
            let content_camel = content_field.to_lower_camel_case();
            let content_wire_key = if is_valid_ts_identifier(content_field) {
                content_field.clone()
            } else {
                format!("'{content_field}'")
            };
            let content_access = if is_valid_ts_identifier(&content_camel) {
                format!("value.{content_camel}")
            } else {
                format!("value['{content_camel}']")
            };
            let content_expr = to_json_expr_inner(content_type, &content_access, convertible);
            format!("return {{ {disc_wire_key}: '{val}', {content_wire_key}: {content_expr} }};")
        }
        TaggingStyle::External => unreachable!(),
    }
}

fn build_external_tagged_to_json(
    fn_name: &str,
    name: &str,
    wire_name: &str,
    tu: &IrTaggedUnion,
    convertible: &HashSet<String>,
) -> Option<CodeBlock> {
    let mut body_lines: Vec<String> = Vec::new();
    for variant in &tu.variants {
        let val = &variant.discriminator_value;
        let ret_expr = external_variant_to_json_expr(val, &variant.content_type, convertible);
        body_lines.push(format!("if ('{val}' in value) return {ret_expr};"));
    }
    body_lines.push("throw new Error('Unknown variant');".to_string());
    sigil_quote!(TypeScript {
        export function $N(fn_name)(value: $N(name)): $N(wire_name) {
        $for(body_line in &body_lines) {
            $L(body_line.as_str())
        }
        }
    })
    .ok()
}

fn external_variant_to_json_expr(
    val: &str,
    content_type: &IrTypeExpr,
    convertible: &HashSet<String>,
) -> String {
    let val_access = format!("value['{val}']");
    let content_expr = to_json_expr_inner(content_type, &val_access, convertible);
    format!("{{ '{val}': {content_expr} }}")
}

// ---------------------------------------------------------------------------
// Union camelCase: structural member selection and semantic conversion
// ---------------------------------------------------------------------------

fn emit_union_file_camel_case(
    schema: &IrSchema,
    ir: &IrSpec,
    union: &IrUnion,
    name: &str,
    convertible: &HashSet<String>,
    strategy: UntaggedUnionStrategy,
    ts: &TypeScript,
) -> Result<FileSpec, SigilStitchError> {
    let wire_name = format!("{name}$Wire");
    let mut wire_members: Vec<_> = union
        .members
        .iter()
        .map(|member| type_expr_to_typename_wire(member, &schema.name, convertible))
        .collect();
    if union.nullable {
        wire_members.push(TypeName::primitive("null"));
    }
    let mut ergonomic_members: Vec<_> = union
        .members
        .iter()
        .map(|member| type_expr_to_typename_for_schema(member, &schema.name))
        .collect();
    if union.nullable {
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
            name,
            ergonomic_type,
            schema.description.as_deref(),
        )?);
    for direction in [JsonDirection::FromWire, JsonDirection::ToWire] {
        file = file.add_function(build_union_converter(
            schema,
            ir,
            union,
            convertible,
            strategy,
            direction,
        )?);
    }
    file.build()
}

fn build_union_converter(
    schema: &IrSchema,
    ir: &IrSpec,
    union: &IrUnion,
    convertible: &HashSet<String>,
    strategy: UntaggedUnionStrategy,
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
    let body = if strategy == UntaggedUnionStrategy::Raw {
        sigil_quote!(TypeScript { return $N(source); })?
    } else {
        let strict = strategy == UntaggedUnionStrategy::Strict;
        let members = plan_union_members(union, ir);
        let raw_fallback = members.iter().any(|member| member.shape == JsonShape::Any);
        let mut checks = Vec::new();
        let mut branches = Vec::new();
        for member in &members {
            if member.shape == JsonShape::Any {
                continue;
            }
            // A self-reference without a container cannot make progress. Leave
            // it to the raw fallback instead of recursively calling ourselves.
            if matches!(&member.type_expr, IrTypeExpr::Named(target) if target == &schema.name) {
                continue;
            }
            let condition = union_shape_condition(
                &member.shape,
                sigil_quote!(TypeScript { candidate })?,
                direction,
            )?;
            let condition = if strict && !member.fallback {
                let index = checks.len();
                checks.push(condition);
                // sigil-stitch literal arguments accept text, not usize. Only
                // this numeric index crosses that boundary.
                sigil_quote!(TypeScript { matches[$L(index.to_string())] })?
            } else {
                condition
            };
            let member_type = match direction {
                JsonDirection::FromWire => {
                    type_expr_to_typename_wire(&member.type_expr, &schema.name, convertible)
                }
                JsonDirection::ToWire => {
                    type_expr_to_typename_for_schema(&member.type_expr, &schema.name)
                }
            };
            let value = json_conversion_expr(
                &member.type_expr,
                sigil_quote!(TypeScript { ($N(source) as unknown as $T(member_type)) })?,
                &schema.name,
                convertible,
                direction,
            )?;
            branches.push(sigil_quote!(TypeScript {
                if ($C(condition)) {
                    return $C(value);
                }
            })?);
        }
        let ambiguity = format!("Ambiguous {name} untagged union value");
        let unmatched = format!("No matching {name} untagged union member");
        sigil_quote!(TypeScript {
            const candidate: $T(TypeName::primitive("unknown")) = $N(source);
            $if(union.nullable) {
                if (candidate === null) {
                    return null;
                }
            }
            $if(strict && !checks.is_empty()) {
                const matches = [$for(check in &checks; separator = ", ") { $C(check.clone()) }];
                if (matches.filter(Boolean).length > 1) {
                    throw new TypeError($S(ambiguity));
                }
            }
            $C_each(branches)
            $if(strict && !raw_fallback) {
                throw new TypeError($S(unmatched));
            } $else {
                return $N(source) as unknown as $T(output_type.clone());
            }
        })?
    };
    FunSpec::builder(&format!("{}{}", fn_base_name(&name), direction.suffix()))
        .visibility(Visibility::Public)
        .add_param(ParameterSpec::of(source, TypeName::primitive(input)))
        .returns(vec![output_type])
        .body(body)
        .build()
}

fn join_union_conditions(
    conditions: Vec<CodeBlock>,
    disjunction: bool,
) -> Result<CodeBlock, SigilStitchError> {
    let mut conditions = conditions.into_iter();
    let Some(mut result) = conditions.next() else {
        return if disjunction {
            sigil_quote!(TypeScript { false })
        } else {
            sigil_quote!(TypeScript { true })
        };
    };
    for condition in conditions {
        result = if disjunction {
            sigil_quote!(TypeScript { ($C(result) || $C(condition)) })?
        } else {
            sigil_quote!(TypeScript { ($C(result) && $C(condition)) })?
        };
    }
    Ok(result)
}

fn union_shape_condition(
    shape: &JsonShape,
    value: CodeBlock,
    direction: JsonDirection,
) -> Result<CodeBlock, SigilStitchError> {
    let record_type = type_application(
        TypeName::primitive("Record"),
        vec![
            TypeName::primitive("string"),
            TypeName::primitive("unknown"),
        ],
    );
    match shape {
        JsonShape::Any => sigil_quote!(TypeScript { true }),
        JsonShape::Never => sigil_quote!(TypeScript { false }),
        JsonShape::Null => sigil_quote!(TypeScript { $C(value) === null }),
        JsonShape::Boolean => sigil_quote!(TypeScript { typeof $C(value) === "boolean" }),
        JsonShape::String => sigil_quote!(TypeScript { typeof $C(value) === "string" }),
        JsonShape::Number { integer } => {
            let predicate = if *integer { "isInteger" } else { "isFinite" };
            sigil_quote!(TypeScript { typeof $C(value.clone()) === "number" && Number.$N(predicate)($C(value)) })
        }
        JsonShape::Literal(literal) => match literal {
            serde_json::Value::String(text) => sigil_quote!(TypeScript { $C(value) === $S(text) }),
            serde_json::Value::Number(number) => {
                sigil_quote!(TypeScript { $C(value) === $L(number.to_string()) })
            }
            serde_json::Value::Bool(true) => sigil_quote!(TypeScript { $C(value) === true }),
            serde_json::Value::Bool(false) => sigil_quote!(TypeScript { $C(value) === false }),
            serde_json::Value::Null => sigil_quote!(TypeScript { $C(value) === null }),
            _ => sigil_quote!(TypeScript { false }),
        },
        JsonShape::Array(inner) => {
            if **inner == JsonShape::Any {
                return sigil_quote!(TypeScript { Array.isArray($C(value)) });
            }
            let item = union_shape_condition(inner, sigil_quote!(TypeScript { item })?, direction)?;
            sigil_quote!(TypeScript { Array.isArray($C(value.clone())) && $C(value).every((item: $T(TypeName::primitive("unknown"))) => $C(item)) })
        }
        JsonShape::Map(inner) => {
            if **inner == JsonShape::Any {
                return sigil_quote!(TypeScript { ($C(value.clone()) !== null && typeof $C(value.clone()) === "object" && !Array.isArray($C(value))) });
            }
            let item = union_shape_condition(inner, sigil_quote!(TypeScript { item })?, direction)?;
            sigil_quote!(TypeScript {
                ($C(value.clone()) !== null && typeof $C(value.clone()) === "object" && !Array.isArray($C(value.clone())) && Object.values($C(value) as $T(record_type)).every((item) => $C(item)))
            })
        }
        JsonShape::Object(properties) => {
            let mut conditions = vec![sigil_quote!(TypeScript {
                $C(value.clone()) !== null && typeof $C(value.clone()) === "object" && !Array.isArray($C(value.clone()))
            })?];
            for property in properties {
                let key = match direction {
                    JsonDirection::FromWire => property.wire_name.clone(),
                    JsonDirection::ToWire if property.rename => {
                        property.wire_name.to_lower_camel_case()
                    }
                    JsonDirection::ToWire => property.wire_name.clone(),
                };
                let access = sigil_quote!(TypeScript { ($C(value.clone()) as $T(record_type.clone()))[$S(key.as_str())] })?;
                let shape_check =
                    union_shape_condition(&property.shape, access.clone(), direction)?;
                conditions.push(if property.required {
                    sigil_quote!(TypeScript { (Object.prototype.hasOwnProperty.call($C(value.clone()), $S(key.as_str())) && $C(shape_check)) })?
                } else {
                    sigil_quote!(TypeScript { (!Object.prototype.hasOwnProperty.call($C(value.clone()), $S(key.as_str())) || $C(access) === undefined || $C(shape_check)) })?
                });
            }
            join_union_conditions(conditions, false)
        }
        JsonShape::AnyOf(shapes) | JsonShape::AllOf(shapes) => {
            let conditions = shapes
                .iter()
                .map(|shape| union_shape_condition(shape, value.clone(), direction))
                .collect::<Result<Vec<_>, _>>()?;
            join_union_conditions(conditions, matches!(shape, JsonShape::AnyOf(_)))
        }
    }
}

// ---------------------------------------------------------------------------
// Intersection camelCase: spread-based converters
// ---------------------------------------------------------------------------

fn emit_intersection_file_camel_case(
    schema: &IrSchema,
    ir: &IrSpec,
    intersection: &IrIntersection,
    name: &str,
    convertible: &HashSet<String>,
    unknown_aliases: &HashSet<String>,
    ts: &TypeScript,
) -> Option<FileSpec> {
    let filename = format!("{}.ts", name);
    let mut fb = FileSpec::builder_with(&filename, ts.clone());

    if let Some(object) = open_intersection_object(intersection, ir, unknown_aliases) {
        let wire_name = format!("{name}$Wire");
        let wire_type = TypeName::intersection(
            intersection
                .members
                .iter()
                .map(|member| type_expr_to_typename_wire(member, &schema.name, convertible))
                .collect(),
        );
        let ergonomic_type = TypeName::intersection(
            intersection
                .members
                .iter()
                .map(|member| type_expr_to_typename_for_schema(member, &schema.name))
                .collect(),
        );
        let mut file = fb
            .add_type(
                build_type_alias(
                    &wire_name,
                    type_application(TypeName::primitive("Readonly"), vec![wire_type]),
                    schema.description.as_deref(),
                )
                .expect("open intersection wire type alias builds"),
            )
            .add_type(
                build_type_alias(
                    name,
                    type_application(TypeName::primitive("Readonly"), vec![ergonomic_type]),
                    schema.description.as_deref(),
                )
                .expect("open intersection ergonomic type alias builds"),
            );
        for direction in [JsonDirection::FromWire, JsonDirection::ToWire] {
            file = file.add_function(
                build_json_converter(
                    name,
                    &wire_name,
                    &schema.name,
                    &object,
                    convertible,
                    unknown_aliases,
                    direction,
                )
                .expect("open intersection JSON converter builds"),
            );
        }
        return file.build().ok();
    }
    // $Wire type alias: A$Wire & B$Wire & ... (only $Wire for convertible refs)
    let wire_name = format!("{name}$Wire");
    let wire_members = intersection
        .members
        .iter()
        .map(|member| type_expr_to_typename_wire(member, &schema.name, convertible))
        .collect();
    fb = fb.add_type(
        build_type_alias(
            &wire_name,
            TypeName::intersection(wire_members),
            schema.description.as_deref(),
        )
        .expect("intersection wire type alias builds"),
    );

    // Ergonomic type alias: A & B & ...
    let ergo_members = intersection
        .members
        .iter()
        .map(|member| type_expr_to_typename_for_schema(member, &schema.name))
        .collect();
    fb = fb.add_type(
        build_type_alias(name, TypeName::intersection(ergo_members), None)
            .expect("intersection ergonomic type alias builds"),
    );

    // fromJSON: spread each convertible Named member's converter
    let base = fn_base_name(name);
    let from_fn = format!("{base}FromJSON");
    let to_fn = format!("{base}ToJSON");
    let from_spreads: Vec<String> = intersection
        .members
        .iter()
        .filter_map(|m| {
            if let IrTypeExpr::Named(ref_name) = m {
                if convertible.contains(ref_name) {
                    let member_pascal = ref_name.to_pascal_case();
                    let member_base = fn_base_name(&member_pascal);
                    Some(format!(
                        "...{member_base}FromJSON(json as {member_pascal}$Wire)"
                    ))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    let from_json = sigil_quote!(TypeScript {
        export function $N(from_fn)(json: $N(wire_name.as_str())): $N(name) {
            return $L("{ ")$for(spread in &from_spreads; separator = ", ") { $L(spread.as_str()) }$L(format!(" }} as {name};"))
        }
    })
    .ok()?;
    fb = fb.add_code(from_json);

    // toJSON: spread each convertible Named member's converter
    let to_spreads: Vec<String> = intersection
        .members
        .iter()
        .filter_map(|m| {
            if let IrTypeExpr::Named(ref_name) = m {
                if convertible.contains(ref_name) {
                    let member_pascal = ref_name.to_pascal_case();
                    let member_base = fn_base_name(&member_pascal);
                    Some(format!("...{member_base}ToJSON(value as {member_pascal})"))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    let to_json = sigil_quote!(TypeScript {
        export function $N(to_fn)(value: $N(name)): $N(wire_name.as_str()) {
            return $L("{ ")$for(spread in &to_spreads; separator = ", ") { $L(spread.as_str()) }$L(format!(" }} as {name}$Wire;"))
        }
    })
    .ok()?;
    fb = fb.add_code(to_json);

    // Add imports for Named members
    for member in &intersection.members {
        if let IrTypeExpr::Named(ref_name) = member {
            let member_pascal = ref_name.to_pascal_case();
            fb = fb.add_import(ImportSpec::named_type(
                &format!("./{member_pascal}"),
                &member_pascal,
            ));
            if convertible.contains(ref_name) {
                let member_base = fn_base_name(&member_pascal);
                fb = fb.add_import(ImportSpec::named_type(
                    &format!("./{member_pascal}"),
                    &format!("{member_pascal}$Wire"),
                ));
                fb = fb.add_import(ImportSpec::named(
                    &format!("./{member_pascal}"),
                    &format!("{member_base}FromJSON"),
                ));
                fb = fb.add_import(ImportSpec::named(
                    &format!("./{member_pascal}"),
                    &format!("{member_base}ToJSON"),
                ));
            }
        }
    }

    fb.build().ok()
}

/// Plan named fields and a dictionary together, so allOf map conversion never
/// interprets a declared field as an extension or leaves its wire spelling behind.
fn open_intersection_object(
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

fn tagged_union_type_imports(
    ir: &IrSpec,
    tu: &IrTaggedUnion,
    convertible: &HashSet<String>,
    include_wire: bool,
) -> Vec<ImportSpec> {
    let mut seen = HashSet::new();
    let mut imports = Vec::new();
    for variant in &tu.variants {
        if is_tag_only_variant(
            ir,
            &tu.discriminator_field,
            &variant.discriminator_value,
            &variant.content_type,
        ) {
            continue;
        }
        collect_type_imports_from_expr(
            &variant.content_type,
            &mut seen,
            &mut imports,
            convertible,
            include_wire,
        );
    }
    imports
}

fn collect_type_imports_from_expr(
    expr: &IrTypeExpr,
    seen: &mut HashSet<String>,
    imports: &mut Vec<ImportSpec>,
    convertible: &HashSet<String>,
    include_wire: bool,
) {
    match expr {
        IrTypeExpr::Named(ref_name) => {
            let pascal = ref_name.to_pascal_case();
            if seen.insert(pascal.clone()) {
                imports.push(ImportSpec::named_type(&format!("./{pascal}"), &pascal));
            }
            if include_wire && convertible.contains(ref_name) {
                let wire = format!("{pascal}$Wire");
                if seen.insert(wire.clone()) {
                    imports.push(ImportSpec::named_type(&format!("./{pascal}"), &wire));
                }
            }
        }
        IrTypeExpr::Array(inner) | IrTypeExpr::Nullable(inner) | IrTypeExpr::Map(inner) => {
            collect_type_imports_from_expr(inner, seen, imports, convertible, include_wire);
        }
        IrTypeExpr::Union(members) => {
            for member in members {
                collect_type_imports_from_expr(member, seen, imports, convertible, include_wire);
            }
        }
        IrTypeExpr::Primitive(_)
        | IrTypeExpr::StringLiteral(_)
        | IrTypeExpr::StringEnum(_)
        | IrTypeExpr::Any => {}
    }
}

/// Build `is*` type guard functions for a tagged union.
///
/// Returns one CodeBlock per contentful (non-empty) variant.
fn build_tagged_union_type_guards(
    name: &str,
    ir: &IrSpec,
    tu: &IrTaggedUnion,
    camel_case: bool,
) -> Vec<CodeBlock> {
    tu.variants
        .iter()
        .filter(|variant| {
            !is_tag_only_variant(
                ir,
                &tu.discriminator_field,
                &variant.discriminator_value,
                &variant.content_type,
            ) && !is_unspecified_variant(
                &variant.discriminator_value,
                &tu.discriminator_field,
                &variant.content_type,
            )
        })
        .filter_map(|variant| {
            let guard_name = is_guard_name(&variant.discriminator_value);
            let (check_body, guard_type) = guard_check_and_type(
                &tu.tagging,
                &tu.discriminator_field,
                &variant.discriminator_value,
                &variant.content_type,
                camel_case,
            );

            let predicate = format!("value is {guard_type}");
            sigil_quote!(TypeScript {
                export function $N(guard_name)(value: $N(name)): $L(predicate) {
                return $L(check_body);
                }
            })
            .ok()
        })
        .collect()
}

/// Derive `isVariantName` from a discriminator value.
fn is_guard_name(disc_value: &str) -> String {
    format!("is{}", disc_value.to_pascal_case())
}

/// Variants that are placeholder sentinels with no meaningful content.
/// Skip emitting a type guard for:
/// - discriminator values containing "UNSPECIFIED" (explicit sentinel)
/// - discriminator values equal to the discriminator field name (e.g. `kind`
///   key in an External-tagged union where the `kind` variant is the empty case)
fn is_unspecified_variant(disc_value: &str, disc_field: &str, content_type: &IrTypeExpr) -> bool {
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

fn is_tag_only_variant(
    ir: &IrSpec,
    disc_field: &str,
    disc_value: &str,
    content_type: &IrTypeExpr,
) -> bool {
    let IrTypeExpr::Named(name) = content_type else {
        return false;
    };
    let Some(schema) = ir.schemas.get(name) else {
        return false;
    };
    let IrSchemaKind::Object(obj) = &schema.kind else {
        return false;
    };
    if obj.additional_properties.is_some() {
        return false;
    }
    if obj.properties.len() != 1 {
        return false;
    }
    let Some(prop) = obj.properties.get(disc_field) else {
        return false;
    };
    matches!(&prop.type_expr, IrTypeExpr::StringLiteral(value) if value == disc_value)
}

// ---------------------------------------------------------------------------
// Variant helpers — shared between tagged-union type emission and guard emission
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
enum PropertyNamingMode {
    CamelCase,
}

/// Build the TS type expression for one variant, with `content` slotted in.
fn variant_type_format(
    tagging: &TaggingStyle,
    disc_field: &str,
    disc_value: &str,
    content_field_mode: Option<PropertyNamingMode>,
    content: &str,
) -> String {
    match tagging {
        TaggingStyle::Internal => {
            format!("({{ {disc_field}: '{disc_value}' }} & {content})")
        }
        TaggingStyle::Adjacent { content_field } => {
            let emitted_content_field = match content_field_mode {
                Some(PropertyNamingMode::CamelCase) => content_field.to_lower_camel_case(),
                None => content_field.clone(),
            };
            format!("{{ {disc_field}: '{disc_value}'; {emitted_content_field}: {content} }}")
        }
        TaggingStyle::External => {
            format!("{{ '{disc_value}': {content} }}")
        }
    }
}

fn tagged_variant_type_string(
    tagging: &TaggingStyle,
    disc_field: &str,
    disc_value: &str,
    content_field_mode: Option<PropertyNamingMode>,
    content: &str,
    tag_only: bool,
) -> String {
    if tag_only
        && matches!(
            tagging,
            TaggingStyle::Internal | TaggingStyle::Adjacent { .. }
        )
    {
        return format!("{{ {disc_field}: '{disc_value}' }}");
    }
    variant_type_format(tagging, disc_field, disc_value, content_field_mode, content)
}

/// Build the runtime narrowing expression for one variant.
fn variant_check_body(tagging: &TaggingStyle, disc_field: &str, disc_value: &str) -> String {
    match tagging {
        TaggingStyle::Internal | TaggingStyle::Adjacent { .. } => {
            format!("value.{disc_field} === '{disc_value}'")
        }
        TaggingStyle::External => {
            format!("'{disc_value}' in value")
        }
    }
}

// ---------------------------------------------------------------------------
// TsTypeDisplay — Display impl for IrTypeExpr as a raw TS type string
// ---------------------------------------------------------------------------

struct TsTypeDisplay<'a>(&'a IrTypeExpr);

impl std::fmt::Display for TsTypeDisplay<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            IrTypeExpr::Named(name) => write!(f, "{}", name.to_pascal_case()),
            IrTypeExpr::Primitive(p) => write!(f, "{}", primitive_to_ts(p)),
            IrTypeExpr::Nullable(inner) => write!(
                f,
                "{}",
                simplify_union_strings(vec![TsTypeDisplay(inner).to_string(), "null".to_string()])
            ),
            IrTypeExpr::Array(inner) => write!(f, "readonly {}[]", TsTypeDisplay(inner)),
            IrTypeExpr::Map(inner) => {
                write!(f, "Record<string, {}>", TsTypeDisplay(inner))
            }
            IrTypeExpr::StringLiteral(s) => write!(f, "'{s}'"),
            IrTypeExpr::StringEnum(values) => write!(
                f,
                "{}",
                simplify_union_strings(values.iter().map(|v| format!("'{v}'")).collect())
            ),
            IrTypeExpr::Union(members) => {
                let parts: Vec<String> = members
                    .iter()
                    .map(|m| TsTypeDisplay(m).to_string())
                    .collect();
                write!(f, "{}", simplify_union_strings(parts))
            }
            IrTypeExpr::Any => write!(f, "unknown"),
        }
    }
}

// ---------------------------------------------------------------------------
// Guard emission helpers
// ---------------------------------------------------------------------------

/// Produce `(check_body, guard_type_text)` for one variant.
fn guard_check_and_type(
    tagging: &TaggingStyle,
    disc_field: &str,
    disc_value: &str,
    content_type: &IrTypeExpr,
    camel_case: bool,
) -> (String, String) {
    let emitted_disc_field = if camel_case {
        disc_field.to_lower_camel_case()
    } else {
        disc_field.to_string()
    };
    let content_field_mode = if camel_case {
        Some(PropertyNamingMode::CamelCase)
    } else {
        None
    };
    let check = variant_check_body(tagging, &emitted_disc_field, disc_value);
    let ty = variant_type_format(
        tagging,
        &emitted_disc_field,
        disc_value,
        content_field_mode,
        &TsTypeDisplay(content_type).to_string(),
    );
    (check, ty)
}

fn tagged_union_type_alias_code<F>(
    name: &str,
    ir: &IrSpec,
    tu: &IrTaggedUnion,
    discriminator_field: &str,
    content_field_mode: Option<PropertyNamingMode>,
    type_name_for: F,
) -> Option<CodeBlock>
where
    F: Fn(&IrTypeExpr) -> String,
{
    let members: Vec<String> = tu
        .variants
        .iter()
        .map(|variant| {
            let tag_only = is_tag_only_variant(
                ir,
                &tu.discriminator_field,
                &variant.discriminator_value,
                &variant.content_type,
            );
            tagged_variant_type_string(
                &tu.tagging,
                discriminator_field,
                &variant.discriminator_value,
                content_field_mode,
                &type_name_for(&variant.content_type),
                tag_only,
            )
        })
        .collect();

    sigil_quote!(TypeScript {
        export type $N(name) = $for(member in &members; separator = " | ") { $L(member.as_str()) };
    })
    .ok()
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
    unknown_aliases: &HashSet<String>,
) -> Option<FieldSpec> {
    let field_name = if is_valid_ts_identifier(&prop.name) {
        prop.name.clone()
    } else {
        format!("'{}'", prop.name)
    };
    let inner_ty = type_expr_to_typename_for_schema(&prop.type_expr, current_schema);
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
    fb.build().ok()
}

fn type_expr_to_typename(expr: &IrTypeExpr) -> TypeName {
    type_expr_to_typename_with_current(expr, None)
}

fn type_expr_to_typename_for_schema(expr: &IrTypeExpr, current_schema: &str) -> TypeName {
    type_expr_to_typename_with_current(expr, Some(current_schema))
}

fn type_expr_to_typename_with_current(expr: &IrTypeExpr, current_schema: Option<&str>) -> TypeName {
    match expr {
        IrTypeExpr::Named(name) => {
            let ts_name = name.to_pascal_case();
            named_ref_to_typename(name, &ts_name, current_schema)
        }
        IrTypeExpr::Primitive(p) => TypeName::primitive(primitive_to_ts(p)),
        // Outer array is `readonly X[]`; nested arrays stay plain `X[]` to
        // avoid sigil's invalid-TS `readonly readonly X[][]` when both layers
        // are readonly. Field-level readonly on the property is still applied
        // by `FieldSpec::is_readonly()` elsewhere.
        IrTypeExpr::Array(inner) => {
            TypeName::readonly_array(type_expr_to_typename_nested(inner, current_schema))
        }
        IrTypeExpr::Nullable(inner) => union_typename(vec![
            type_expr_to_typename_with_current(inner, current_schema),
            TypeName::primitive("null"),
        ]),
        IrTypeExpr::StringLiteral(s) => TypeName::string_literal(s.clone()),
        IrTypeExpr::StringEnum(values) => union_typename(
            values
                .iter()
                .map(|v| TypeName::string_literal(v.clone()))
                .collect(),
        ),
        IrTypeExpr::Map(inner) => type_application(
            TypeName::primitive("Record"),
            vec![
                TypeName::primitive("string"),
                type_expr_to_typename_with_current(inner, current_schema),
            ],
        ),
        IrTypeExpr::Union(_) if !contains_any_named_ref(expr) => {
            TypeName::raw(&type_expr_str(expr))
        }
        IrTypeExpr::Union(members) => union_typename(
            members
                .iter()
                .map(|member| type_expr_to_typename_with_current(member, current_schema))
                .collect(),
        ),
        IrTypeExpr::Any => TypeName::primitive("unknown"),
    }
}

fn named_ref_to_typename(
    schema_name: &str,
    rendered_name: &str,
    current_schema: Option<&str>,
) -> TypeName {
    if current_schema == Some(schema_name) {
        TypeName::primitive(rendered_name)
    } else {
        let module = format!("./{}", schema_name.to_pascal_case());
        TypeName::importable_type(&module, rendered_name)
    }
}

/// Same as [`type_expr_to_typename`] but nested arrays render as plain `X[]`.
fn type_expr_to_typename_nested(expr: &IrTypeExpr, current_schema: Option<&str>) -> TypeName {
    match expr {
        IrTypeExpr::Array(inner) => {
            TypeName::array(type_expr_to_typename_nested(inner, current_schema))
        }
        // Array element unions must stay structured so the renderer can add
        // parentheses; a raw `T | null` would render as `T | null[]`.
        IrTypeExpr::Nullable(inner) => union_typename(vec![
            type_expr_to_typename_nested(inner, current_schema),
            TypeName::primitive("null"),
        ]),
        IrTypeExpr::Union(members) => union_typename(
            members
                .iter()
                .map(|member| type_expr_to_typename_nested(member, current_schema))
                .collect(),
        ),
        other => type_expr_to_typename_with_current(other, current_schema),
    }
}

pub fn build_convertible_set(ir: &IrSpec, flags: EmitFlags) -> HashSet<String> {
    if !flags.property_naming_camel_case {
        return HashSet::new();
    }
    // Pass 1: leaf convertibles (Object, TaggedUnion) — always convertible.
    let leaf: HashSet<String> = ir
        .schemas
        .iter()
        .filter_map(|(name, schema)| {
            let dominated = match &schema.kind {
                IrSchemaKind::Object(_) => true,
                IrSchemaKind::TaggedUnion(tu) => !tu.variants.is_empty(),
                _ => false,
            };
            dominated.then(|| name.clone())
        })
        .collect();

    // Resolve aliases and composites to a fixed point so a chain of aliases
    // cannot hide a model's wire type or required JSON conversion.
    let mut set = leaf;
    loop {
        let previous_len = set.len();
        for (name, schema) in &ir.schemas {
            let dominated = match &schema.kind {
                IrSchemaKind::Alias(expr) => has_named_ref(expr, &set),
                IrSchemaKind::Intersection(i) => i.members.iter().any(|m| has_named_ref(m, &set)),
                IrSchemaKind::Union(u) => u.members.iter().any(|m| has_named_ref(m, &set)),
                _ => false,
            };
            if dominated {
                set.insert(name.clone());
            }
        }
        if set.len() == previous_len {
            break;
        }
    }
    set
}

/// Lower every `IrSchema` in the spec into a sigil-rendered `FileInfo`.
pub fn generate_model_files(
    ir: &IrSpec,
    flags: EmitFlags,
    ts: &TypeScript,
) -> Result<Vec<FileInfo>, GeneratorError> {
    let header = super::project_files::render_file_header(&ir.info);
    let convertible = build_convertible_set(ir, flags);
    let unknown_aliases = build_unknown_alias_set(ir);
    let mut files = Vec::with_capacity(ir.schemas.len());

    for (name, schema) in &ir.schemas {
        let file_spec = emit_model_file(schema, ir, flags, &convertible, &unknown_aliases, ts)
            .map_err(|source| GeneratorError::emission(ArtifactKind::Model, name, source))?
            .ok_or_else(|| GeneratorError::UnsupportedSchema {
                schema: name.clone(),
                kind: format!("{:?}", schema.kind),
            })?;
        let body = file_spec
            .render(100)
            .map_err(|source| GeneratorError::emission(ArtifactKind::Model, name, source))?;
        let content = format!("{header}{body}");
        let filename = format!("{}.ts", schema.name.to_pascal_case());
        files.push(FileInfo::model(filename, content));
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
    fn union_literal_and_numeric_guards_render_exactly() {
        let value = sigil_quote!(TypeScript { candidate }).unwrap();
        let integer = union_shape_condition(
            &JsonShape::Number { integer: true },
            value.clone(),
            JsonDirection::FromWire,
        )
        .unwrap();
        assert_rendered!(
            TypeScript::new(),
            integer,
            "typeof candidate === \"number\" && Number.isInteger(candidate)\n"
        );
        let boolean = union_shape_condition(
            &JsonShape::Literal(false.into()),
            value,
            JsonDirection::FromWire,
        )
        .unwrap();
        assert_rendered!(TypeScript::new(), boolean, "candidate === false\n");
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
            let convertible = build_convertible_set(&ir, flags);
            for name in [
                "DetailAlias",
                "DetailAliasChain",
                "DetailListAlias",
                "NullableDetailAlias",
            ] {
                assert!(
                    convertible.contains(name),
                    "{name} must retain model conversion"
                );
            }
            assert!(!convertible.contains("AnyJsonValue"));
            assert!(build_convertible_set(&ir, EmitFlags::default()).is_empty());
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
        let convertible = build_convertible_set(&ir, flags);
        let file = emit_model_file(
            &ir.schemas["DetailAliasChain"],
            &ir,
            flags,
            &convertible,
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
        let convertible = build_convertible_set(&ir, flags);
        let file = emit_model_file(
            &ir.schemas["NullableDetailAlias"],
            &ir,
            flags,
            &convertible,
            &HashSet::new(),
            &TypeScript::new(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            file.render(100).unwrap(),
            r#"import type { DetailAliasChain, DetailAliasChain$Wire } from './DetailAliasChain';
import { detailAliasChainFromJSON, detailAliasChainToJSON } from './DetailAliasChain';

export type NullableDetailAlias$Wire = DetailAliasChain$Wire | null;

export type NullableDetailAlias = DetailAliasChain | null;

export function nullableDetailAliasFromJSON(json: NullableDetailAlias$Wire): NullableDetailAlias {
  return json != null ? detailAliasChainFromJSON(json) : null;
}

export function nullableDetailAliasToJSON(value: NullableDetailAlias): NullableDetailAlias$Wire {
  return value != null ? detailAliasChainToJSON(value) : null;
}
"#
        );
    }

    #[test]
    fn planned_allof_type_keeps_readonly_dictionary_and_imports() {
        let ir = additional_properties_ir();
        let file = emit_model_file(
            &ir.schemas["AllOfMixedStringMap"],
            &ir,
            EmitFlags::default(),
            &HashSet::new(),
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
        let convertible = build_convertible_set(&ir, flags);
        let mut schema = ir.schemas["AllOfJsonDictionary"].clone();
        schema.kind = IrSchemaKind::Intersection(intersection);
        let file = emit_model_file(
            &schema,
            &ir,
            flags,
            &convertible,
            &build_unknown_alias_set(&ir),
            &TypeScript::new(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            file.render(100).unwrap(),
            r#"import type { AnyJsonValue } from './AnyJsonValue';
import type { Detail, Detail$Wire } from './Detail';
import { detailFromJSON, detailToJSON } from './Detail';

export type AllOfJsonDictionary$Wire = Detail$Wire & Record<string, AnyJsonValue> & string;

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
        ];
        for (expr, expected) in cases {
            let ty = type_expr_to_typename(&expr);
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
        let convertible = HashSet::from(["Detail".to_string()]);
        for (direction, expected) in [
            (
                JsonDirection::FromWire,
                "import { detailFromJSON } from './Detail';\n\nObject.fromEntries(Object.entries(values ?? {}).map(([k, v]) => [k, v != null ? detailFromJSON(v) : null]))\n",
            ),
            (
                JsonDirection::ToWire,
                "import { detailToJSON } from './Detail';\n\nObject.fromEntries(Object.entries(values ?? {}).map(([k, v]) => [k, v != null ? detailToJSON(v) : null]))\n",
            ),
        ] {
            let access = sigil_quote!(TypeScript { values }).unwrap();
            let block =
                json_conversion_expr(&expr, access, "Envelope", &convertible, direction).unwrap();
            assert_rendered!(TypeScript::new(), width = 200, block, expected);
        }
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
        let convertible = HashSet::from(["Detail".to_string()]);
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
                &convertible,
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
