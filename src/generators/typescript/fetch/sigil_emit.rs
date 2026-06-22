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
use crate::ir::types::{
    IrEnum, IrEnumValueType, IrIntersection, IrObject, IrPrimitive, IrProperty, IrSchema,
    IrSchemaKind, IrSpec, IrTaggedUnion, IrTypeExpr, IrUnion, TaggingStyle,
};
use heck::{ToLowerCamelCase, ToPascalCase};
use sigil_stitch::code_block::CodeBlock;
use sigil_stitch::lang::typescript::TypeScript;
use sigil_stitch::prelude::sigil_quote;
use sigil_stitch::spec::field_spec::FieldSpec;
use sigil_stitch::spec::file_spec::FileSpec;
use sigil_stitch::spec::import_spec::ImportSpec;
use sigil_stitch::spec::modifiers::{TypeKind, Visibility};
use sigil_stitch::spec::type_spec::TypeSpec;
use sigil_stitch::type_name::TypeName;
use std::collections::HashSet;

/// Flags controlling optional TS emissions (const objects, type guards).
#[derive(Debug, Clone, Copy, Default)]
pub struct EmitFlags {
    pub emit_enum_constants: bool,
    pub emit_type_guards: bool,
    pub property_naming_camel_case: bool,
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
        IrSchemaKind::Intersection(_) if convertible.contains(&schema.name) => {
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
        IrSchemaKind::Intersection(_) if convertible.contains(&schema.name) => {
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
) -> Option<FileSpec> {
    match &schema.kind {
        IrSchemaKind::Object(obj) => {
            emit_object_file(schema, obj, flags, convertible, unknown_aliases, ts)
        }
        IrSchemaKind::Enum(en) => emit_enum_file_from(schema, en, flags, ts),
        IrSchemaKind::Alias(expr) => emit_alias_file(schema, expr, ts),
        IrSchemaKind::Union(u) => emit_union_file(schema, u, flags, convertible, ts),
        IrSchemaKind::Intersection(i) => emit_intersection_file(schema, i, flags, convertible, ts),
        IrSchemaKind::TaggedUnion(tu) => {
            emit_tagged_union_file(schema, ir, tu, flags, convertible, ts)
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
        tb = tb.add_field(build_field(prop, unknown_aliases)?);
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
        wire_tb = wire_tb.add_field(build_field_wire(prop, convertible, unknown_aliases)?);
    }

    // --- Ergonomic interface (camelCase property names) ---
    let mut ergo_tb = TypeSpec::builder(name, TypeKind::Interface).visibility(Visibility::Public);
    if let Some(doc) = &schema.description {
        ergo_tb = ergo_tb.doc(doc);
    }
    for (_json_name, prop) in &obj.properties {
        ergo_tb = ergo_tb.add_field(build_field_camel(prop, unknown_aliases)?);
    }

    // Collect Named refs that need converter imports (only convertible ones)
    let named_refs = collect_named_refs(obj, convertible);

    // --- fromJSON / toJSON functions ---
    let from_json = build_from_json_fn(name, &wire_name, obj, convertible)?;
    let to_json = build_to_json_fn(name, &wire_name, obj, convertible)?;

    let mut fb = FileSpec::builder_with(&filename, ts.clone());
    fb = fb.add_type(wire_tb.build().ok()?);
    fb = fb.add_type(ergo_tb.build().ok()?);

    // Add explicit imports for referenced converter functions
    for ref_name in &named_refs {
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
    fb.build().ok()
}

/// Build a field for the $Wire interface (preserves original property name).
fn build_field_wire(
    prop: &IrProperty,
    convertible: &HashSet<String>,
    unknown_aliases: &HashSet<String>,
) -> Option<FieldSpec> {
    let field_name = if is_valid_ts_identifier(&prop.name) {
        prop.name.clone()
    } else {
        format!("'{}'", prop.name)
    };
    let inner_ty = type_expr_to_typename_wire(&prop.type_expr, convertible);
    let field_ty = if prop.nullable {
        nullable_field_type_name(
            &prop.type_expr,
            inner_ty,
            type_expr_str_wire(&prop.type_expr, convertible),
            unknown_aliases,
        )
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
fn build_field_camel(prop: &IrProperty, unknown_aliases: &HashSet<String>) -> Option<FieldSpec> {
    let camel = prop.name.to_lower_camel_case();
    let field_name = if is_valid_ts_identifier(&camel) {
        camel
    } else {
        format!("'{}'", camel)
    };
    let inner_ty = type_expr_to_typename(&prop.type_expr);
    let field_ty = if prop.nullable {
        nullable_field_type_name(
            &prop.type_expr,
            inner_ty,
            type_expr_str(&prop.type_expr),
            unknown_aliases,
        )
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
fn type_expr_to_typename_wire(expr: &IrTypeExpr, convertible: &HashSet<String>) -> TypeName {
    match expr {
        IrTypeExpr::Named(name) => {
            let ts_name = name.to_pascal_case();
            let module = format!("./{ts_name}");
            if convertible.contains(name) {
                let wire_name = format!("{}$Wire", ts_name);
                TypeName::importable_type(&module, &wire_name)
            } else {
                TypeName::importable_type(&module, &ts_name)
            }
        }
        IrTypeExpr::Array(inner) => {
            TypeName::readonly_array(type_expr_to_typename_wire_nested(inner, convertible))
        }
        IrTypeExpr::Nullable(inner) => union_typename(vec![
            type_expr_to_typename_wire(inner, convertible),
            TypeName::primitive("null"),
        ]),
        IrTypeExpr::Map(inner) => TypeName::generic(
            TypeName::primitive("Record"),
            vec![
                TypeName::primitive("string"),
                type_expr_to_typename_wire(inner, convertible),
            ],
        ),
        IrTypeExpr::StringLiteral(s) => TypeName::raw(&format!("'{s}'")),
        IrTypeExpr::StringEnum(values) => union_typename(
            values
                .iter()
                .map(|v| TypeName::raw(&format!("'{v}'")))
                .collect(),
        ),
        IrTypeExpr::Union(members) => union_typename(
            members
                .iter()
                .map(|member| type_expr_to_typename_wire(member, convertible))
                .collect(),
        ),
        IrTypeExpr::Primitive(p) => TypeName::primitive(primitive_to_ts(p)),
        IrTypeExpr::Any => TypeName::primitive("unknown"),
    }
}

fn type_expr_to_typename_wire_nested(expr: &IrTypeExpr, convertible: &HashSet<String>) -> TypeName {
    match expr {
        IrTypeExpr::Array(inner) => {
            TypeName::array(type_expr_to_typename_wire_nested(inner, convertible))
        }
        other => type_expr_to_typename_wire(other, convertible),
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
    rendered_inner: String,
    unknown_aliases: &HashSet<String>,
) -> TypeName {
    if type_expr_collapses_to_unknown(expr, unknown_aliases) {
        inner_ty
    } else if contains_any_named_ref(expr) {
        union_typename(vec![inner_ty, TypeName::primitive("null")])
    } else {
        TypeName::raw(&simplify_union_strings(vec![
            rendered_inner,
            "null".to_string(),
        ]))
    }
}

fn is_compound_type(expr: &IrTypeExpr) -> bool {
    matches!(
        expr,
        IrTypeExpr::Nullable(_) | IrTypeExpr::Union(_) | IrTypeExpr::StringEnum(_)
    )
}

/// Build the `export function nameFromJSON(json: Name$Wire): Name` function body.
fn build_from_json_fn(
    name: &str,
    wire_name: &str,
    obj: &IrObject,
    convertible: &HashSet<String>,
) -> Option<CodeBlock> {
    let fn_name = format!("{}FromJSON", fn_base_name(name));
    let fields: Vec<CodeBlock> = obj
        .properties
        .iter()
        .map(|(_json_name, prop)| {
            let camel = prop.name.to_lower_camel_case();
            let ergo_key = obj_literal_key(&camel);
            let wire_access = wire_field_access(&prop.name);
            let conv = from_json_expr(
                &prop.type_expr,
                &wire_access,
                !prop.required,
                prop.nullable,
                convertible,
            );
            CodeBlock::of(&format!("{ergo_key}: {conv},"), ())
        })
        .collect::<Result<_, _>>()
        .ok()?;

    if obj.properties.is_empty() {
        return sigil_quote!(TypeScript {
            export function $N(fn_name)(json: $N(wire_name)): $N(name) {
                return json as unknown as $N(name);
            }
        })
        .ok();
    }

    sigil_quote!(TypeScript {
        export function $N(fn_name)(json: $N(wire_name)): $N(name) {
        return {
        $C_each(fields);
        };
        }
    })
    .ok()
}

/// Build the `export function nameToJSON(value: Name): Name$Wire` function body.
fn build_to_json_fn(
    name: &str,
    wire_name: &str,
    obj: &IrObject,
    convertible: &HashSet<String>,
) -> Option<CodeBlock> {
    let fn_name = format!("{}ToJSON", fn_base_name(name));
    let fields: Vec<CodeBlock> = obj
        .properties
        .iter()
        .map(|(_json_name, prop)| {
            let camel = prop.name.to_lower_camel_case();
            let wire_key = obj_literal_key(&prop.name);
            let ergo_access = ergo_field_access(&camel);
            let conv = to_json_expr(
                &prop.type_expr,
                &ergo_access,
                !prop.required,
                prop.nullable,
                convertible,
            );
            CodeBlock::of(&format!("{wire_key}: {conv},"), ())
        })
        .collect::<Result<_, _>>()
        .ok()?;

    if obj.properties.is_empty() {
        return sigil_quote!(TypeScript {
            export function $N(fn_name)(value: $N(name)): $N(wire_name) {
                return value as unknown as $N(wire_name);
            }
        })
        .ok();
    }

    sigil_quote!(TypeScript {
        export function $N(fn_name)(value: $N(name)): $N(wire_name) {
        return {
        $C_each(fields);
        };
        }
    })
    .ok()
}

/// Produce `json.field` or `json['kebab-field']` depending on identifier validity.
fn wire_field_access(wire_name: &str) -> String {
    if is_valid_ts_identifier(wire_name) {
        format!("json.{wire_name}")
    } else {
        format!("json['{wire_name}']")
    }
}

/// Produce `value.field` or `value['field']` for accessing the ergonomic object.
fn ergo_field_access(camel: &str) -> String {
    if is_valid_ts_identifier(camel) {
        format!("value.{camel}")
    } else {
        format!("value['{camel}']")
    }
}

/// Produce a valid object literal key: quote if not a valid identifier.
fn obj_literal_key(name: &str) -> String {
    if is_valid_ts_identifier(name) {
        name.to_string()
    } else {
        format!("'{name}'")
    }
}

/// Generate the fromJSON expression for a type.
fn from_json_expr(
    expr: &IrTypeExpr,
    access: &str,
    optional: bool,
    nullable: bool,
    convertible: &HashSet<String>,
) -> String {
    let inner = from_json_expr_inner(expr, access, convertible);
    let with_null = if nullable && needs_conversion(expr, convertible) {
        format!("{access} === null ? null : {inner}")
    } else {
        inner
    };
    if optional {
        if needs_conversion(expr, convertible) {
            format!("{access} !== undefined ? {with_null} : undefined")
        } else {
            with_null
        }
    } else {
        with_null
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

/// Generate the toJSON expression for a type.
fn to_json_expr(
    expr: &IrTypeExpr,
    access: &str,
    optional: bool,
    nullable: bool,
    convertible: &HashSet<String>,
) -> String {
    let inner = to_json_expr_inner(expr, access, convertible);
    let with_null = if nullable && needs_conversion(expr, convertible) {
        format!("{access} === null ? null : {inner}")
    } else {
        inner
    };
    if optional {
        if needs_conversion(expr, convertible) {
            format!("{access} !== undefined ? {with_null} : undefined")
        } else {
            with_null
        }
    } else {
        with_null
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

/// Collect unique Named type references from an object's properties (only convertible ones).
fn collect_named_refs(obj: &IrObject, convertible: &HashSet<String>) -> Vec<String> {
    let mut refs = HashSet::new();
    for (_name, prop) in &obj.properties {
        collect_named_refs_from_expr(&prop.type_expr, &mut refs, convertible);
    }
    let mut sorted: Vec<String> = refs.into_iter().collect();
    sorted.sort();
    sorted
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
            let key_lit = if is_valid_ts_identifier(&key) {
                key.clone()
            } else {
                format!("'{key}'")
            };
            CodeBlock::of(&format!("  {key_lit}: {val_literal},"), ()).ok()
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
fn emit_alias_file(schema: &IrSchema, expr: &IrTypeExpr, ts: &TypeScript) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();
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

/// Union file: `export type Name = A | B | C [| null];`
///
/// `nullable` appends a `null` member. Imports for each `IrTypeExpr::Named`
/// member are tracked via `TypeName::Importable` inside `TypeName::union`.
fn emit_union_file(
    schema: &IrSchema,
    union: &IrUnion,
    flags: EmitFlags,
    convertible: &HashSet<String>,
    ts: &TypeScript,
) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();

    if flags.property_naming_camel_case && convertible.contains(&schema.name) {
        return emit_union_file_camel_case(schema, union, &name, convertible, ts);
    }

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
    intersection: &IrIntersection,
    flags: EmitFlags,
    convertible: &HashSet<String>,
    ts: &TypeScript,
) -> Option<FileSpec> {
    if intersection.members.is_empty() {
        return None;
    }
    let name = schema.name.to_pascal_case();

    if flags.property_naming_camel_case && convertible.contains(&schema.name) {
        return emit_intersection_file_camel_case(schema, intersection, &name, convertible, ts);
    }

    let members: Vec<TypeName> = intersection
        .members
        .iter()
        .map(type_expr_to_typename)
        .collect();
    let inter_ty = TypeName::intersection(members);

    let type_alias = sigil_quote!(TypeScript {
        export type $N(name.as_str()) = $T(inter_ty);
    })
    .ok()?;

    let filename = format!("{}.ts", name);
    let mut fb = FileSpec::builder_with(&filename, ts.clone());
    if let Some(doc) = &schema.description {
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
    ts: &TypeScript,
) -> Option<FileSpec> {
    if tu.variants.is_empty() {
        return None;
    }

    let name = schema.name.to_pascal_case();

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
// Union camelCase: cast-through pattern
// ---------------------------------------------------------------------------

fn emit_union_file_camel_case(
    schema: &IrSchema,
    union: &IrUnion,
    name: &str,
    convertible: &HashSet<String>,
    ts: &TypeScript,
) -> Option<FileSpec> {
    let filename = format!("{}.ts", name);
    let mut fb = FileSpec::builder_with(&filename, ts.clone());

    if let Some(doc) = &schema.description {
        fb = fb.add_raw(&format!("/** {doc} */\n"));
    }

    // $Wire type alias
    let wire_name = format!("{name}$Wire");
    let mut wire_members: Vec<String> = union
        .members
        .iter()
        .map(|m| type_expr_str_wire(m, convertible))
        .collect();
    if union.nullable {
        wire_members.push("null".to_string());
    }
    let wire_rhs = simplify_union_strings(wire_members);
    let wire_type = sigil_quote!(TypeScript {
        export type $N(wire_name.as_str()) = $L(wire_rhs.as_str());
    })
    .ok()?;
    fb = fb.add_code(wire_type);

    // Ergonomic type alias
    let mut ergo_members: Vec<String> = union.members.iter().map(type_expr_str).collect();
    if union.nullable {
        ergo_members.push("null".to_string());
    }
    let ergo_rhs = simplify_union_strings(ergo_members);
    let ergo_type = sigil_quote!(TypeScript {
        export type $N(name) = $L(ergo_rhs.as_str());
    })
    .ok()?;
    fb = fb.add_code(ergo_type);

    // fromJSON: cast-through
    let base = fn_base_name(name);
    let from_fn = format!("{base}FromJSON");
    let to_fn = format!("{base}ToJSON");
    let from_json = sigil_quote!(TypeScript {
        export function $N(from_fn)(json: $N(wire_name.as_str())): $N(name) {
            return json as unknown as $N(name);
        }
    })
    .ok()?;
    fb = fb.add_code(from_json);

    // toJSON: cast-through
    let to_json = sigil_quote!(TypeScript {
        export function $N(to_fn)(value: $N(name)): $N(wire_name.as_str()) {
            return value as unknown as $N(wire_name.as_str());
        }
    })
    .ok()?;
    fb = fb.add_code(to_json);

    // Add imports for Named members (wire + ergonomic types only if convertible)
    let mut seen_imports = HashSet::new();
    let mut imports = Vec::new();
    for member in &union.members {
        collect_type_imports_from_expr(member, &mut seen_imports, &mut imports, convertible, true);
    }
    if ergo_rhs != "unknown" || wire_rhs != "unknown" {
        for import in imports {
            fb = fb.add_import(import);
        }
    }

    fb.build().ok()
}

// ---------------------------------------------------------------------------
// Intersection camelCase: spread-based converters
// ---------------------------------------------------------------------------

fn emit_intersection_file_camel_case(
    schema: &IrSchema,
    intersection: &IrIntersection,
    name: &str,
    convertible: &HashSet<String>,
    ts: &TypeScript,
) -> Option<FileSpec> {
    let filename = format!("{}.ts", name);
    let mut fb = FileSpec::builder_with(&filename, ts.clone());

    if let Some(doc) = &schema.description {
        fb = fb.add_raw(&format!("/** {doc} */\n"));
    }

    // $Wire type alias: A$Wire & B$Wire & ... (only $Wire for convertible refs)
    let wire_name = format!("{name}$Wire");
    let wire_members: Vec<String> = intersection
        .members
        .iter()
        .map(|m| type_expr_str_wire(m, convertible))
        .collect();
    let wire_type = sigil_quote!(TypeScript {
        export type $N(wire_name.as_str()) = $for(member in &wire_members; separator = " & ") { $L(member.as_str()) };
    })
    .ok()?;
    fb = fb.add_code(wire_type);

    // Ergonomic type alias: A & B & ...
    let ergo_members: Vec<String> = intersection.members.iter().map(type_expr_str).collect();
    let ergo_type = sigil_quote!(TypeScript {
        export type $N(name) = $for(member in &ergo_members; separator = " & ") { $L(member.as_str()) };
    })
    .ok()?;
    fb = fb.add_code(ergo_type);

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

fn build_field(prop: &IrProperty, unknown_aliases: &HashSet<String>) -> Option<FieldSpec> {
    let field_name = if is_valid_ts_identifier(&prop.name) {
        prop.name.clone()
    } else {
        format!("'{}'", prop.name)
    };
    let inner_ty = type_expr_to_typename(&prop.type_expr);
    let field_ty = if prop.nullable {
        nullable_field_type_name(
            &prop.type_expr,
            inner_ty,
            type_expr_str(&prop.type_expr),
            unknown_aliases,
        )
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
    match expr {
        IrTypeExpr::Named(name) => {
            let ts_name = name.to_pascal_case();
            let module = format!("./{ts_name}");
            TypeName::importable_type(&module, &ts_name)
        }
        IrTypeExpr::Primitive(p) => TypeName::primitive(primitive_to_ts(p)),
        // Outer array is `readonly X[]`; nested arrays stay plain `X[]` to
        // avoid sigil's invalid-TS `readonly readonly X[][]` when both layers
        // are readonly. Field-level readonly on the property is still applied
        // by `FieldSpec::is_readonly()` elsewhere.
        IrTypeExpr::Array(inner) => TypeName::readonly_array(type_expr_to_typename_nested(inner)),
        IrTypeExpr::Nullable(inner) if !contains_any_named_ref(inner) => {
            TypeName::raw(&type_expr_str(expr))
        }
        IrTypeExpr::Nullable(inner) => union_typename(vec![
            type_expr_to_typename(inner),
            TypeName::primitive("null"),
        ]),
        IrTypeExpr::StringLiteral(s) => TypeName::raw(&format!("'{s}'")),
        IrTypeExpr::StringEnum(values) => union_typename(
            values
                .iter()
                .map(|v| TypeName::raw(&format!("'{v}'")))
                .collect(),
        ),
        IrTypeExpr::Map(inner) => TypeName::generic(
            TypeName::primitive("Record"),
            vec![TypeName::primitive("string"), type_expr_to_typename(inner)],
        ),
        IrTypeExpr::Union(_) if !contains_any_named_ref(expr) => {
            TypeName::raw(&type_expr_str(expr))
        }
        IrTypeExpr::Union(members) => {
            union_typename(members.iter().map(type_expr_to_typename).collect())
        }
        IrTypeExpr::Any => TypeName::primitive("unknown"),
    }
}

/// Same as [`type_expr_to_typename`] but nested arrays render as plain `X[]`.
fn type_expr_to_typename_nested(expr: &IrTypeExpr) -> TypeName {
    match expr {
        IrTypeExpr::Array(inner) => TypeName::array(type_expr_to_typename_nested(inner)),
        other => type_expr_to_typename(other),
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

    // Pass 2: composite convertibles (Intersection, Union) — only if they
    // reference at least one leaf-convertible member.
    let mut set = leaf.clone();
    for (name, schema) in &ir.schemas {
        let dominated = match &schema.kind {
            IrSchemaKind::Intersection(i) => i.members.iter().any(|m| has_named_ref(m, &leaf)),
            IrSchemaKind::Union(u) => u.members.iter().any(|m| has_named_ref(m, &leaf)),
            _ => false,
        };
        if dominated {
            set.insert(name.clone());
        }
    }
    set
}

/// Lower every `IrSchema` in the spec into a sigil-rendered `FileInfo`.
pub fn generate_model_files(
    ir: &IrSpec,
    flags: EmitFlags,
    ts: &TypeScript,
) -> Result<Vec<FileInfo>, String> {
    let header = super::project_files::render_file_header(&ir.info);
    let convertible = build_convertible_set(ir, flags);
    let unknown_aliases = build_unknown_alias_set(ir);
    let mut files = Vec::with_capacity(ir.schemas.len());

    for (name, schema) in &ir.schemas {
        let file_spec = emit_model_file(schema, ir, flags, &convertible, &unknown_aliases, ts)
            .ok_or_else(|| {
                format!(
                    "sigil_emit: unsupported schema kind for {name}: {:?}",
                    schema.kind
                )
            })?;
        let body = file_spec
            .render(100)
            .map_err(|e| format!("sigil_emit: render {name}: {e}"))?;
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
