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
use sigil_stitch::code_block::{Arg, CodeBlock};
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
                            !is_unspecified_variant(
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
                !is_unspecified_variant(
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
    flags: EmitFlags,
    convertible: &HashSet<String>,
) -> Option<FileSpec> {
    match &schema.kind {
        IrSchemaKind::Object(obj) => Some(emit_object_file(schema, obj, flags, convertible)),
        IrSchemaKind::Enum(en) => emit_enum_file_from(schema, en, flags),
        IrSchemaKind::Alias(expr) => emit_alias_file(schema, expr),
        IrSchemaKind::Union(u) => emit_union_file(schema, u, flags, convertible),
        IrSchemaKind::Intersection(i) => emit_intersection_file(schema, i, flags, convertible),
        IrSchemaKind::TaggedUnion(tu) => emit_tagged_union_file(schema, tu, flags, convertible),
    }
}

/// Back-compat alias used by the enum prototype test. Prefer `emit_model_file`.
pub fn emit_enum_file(schema: &IrSchema) -> Option<FileSpec> {
    let IrSchemaKind::Enum(en) = &schema.kind else {
        return None;
    };
    emit_enum_file_from(schema, en, EmitFlags::default())
}

fn emit_object_file(
    schema: &IrSchema,
    obj: &IrObject,
    flags: EmitFlags,
    convertible: &HashSet<String>,
) -> FileSpec {
    let name = schema.name.to_pascal_case();

    if flags.property_naming_camel_case {
        return emit_object_file_camel_case(schema, obj, &name, convertible);
    }

    let mut tb = TypeSpec::builder(&name, TypeKind::Interface).visibility(Visibility::Public);
    if let Some(doc) = &schema.description {
        tb = tb.doc(doc);
    }

    for (_json_name, prop) in &obj.properties {
        tb = tb.add_field(build_field(prop));
    }

    let filename = format!("{}.ts", name);
    FileSpec::builder(&filename)
        .add_type(tb.build().expect("TypeSpec builds"))
        .build()
        .expect("FileSpec builds")
}

/// Emit an object file with dual types: `Name$Wire` (wire format) + `Name`
/// (camelCase ergonomic) plus `nameFromJSON` / `nameToJSON` converters.
fn emit_object_file_camel_case(
    schema: &IrSchema,
    obj: &IrObject,
    name: &str,
    convertible: &HashSet<String>,
) -> FileSpec {
    let wire_name = format!("{}$Wire", name);
    let filename = format!("{}.ts", name);

    // --- $Wire interface (original wire-format property names) ---
    let mut wire_tb =
        TypeSpec::builder(&wire_name, TypeKind::Interface).visibility(Visibility::Public);
    if let Some(doc) = &schema.description {
        wire_tb = wire_tb.doc(doc);
    }
    for (_json_name, prop) in &obj.properties {
        wire_tb = wire_tb.add_field(build_field_wire(prop, convertible));
    }

    // --- Ergonomic interface (camelCase property names) ---
    let mut ergo_tb = TypeSpec::builder(name, TypeKind::Interface).visibility(Visibility::Public);
    if let Some(doc) = &schema.description {
        ergo_tb = ergo_tb.doc(doc);
    }
    for (_json_name, prop) in &obj.properties {
        ergo_tb = ergo_tb.add_field(build_field_camel(prop));
    }

    // Collect Named refs that need converter imports (only convertible ones)
    let named_refs = collect_named_refs(obj, convertible);

    // --- fromJSON / toJSON functions ---
    let from_json = build_from_json_fn(name, &wire_name, obj, convertible);
    let to_json = build_to_json_fn(name, &wire_name, obj, convertible);

    let mut fb = FileSpec::builder(&filename);
    fb = fb.add_type(wire_tb.build().expect("TypeSpec builds"));
    fb = fb.add_type(ergo_tb.build().expect("TypeSpec builds"));

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
    fb.build().expect("FileSpec builds")
}

/// Build a field for the $Wire interface (preserves original property name).
fn build_field_wire(prop: &IrProperty, convertible: &HashSet<String>) -> FieldSpec {
    let field_name = if is_valid_ts_identifier(&prop.name) {
        prop.name.clone()
    } else {
        format!("'{}'", prop.name)
    };
    let inner_ty = type_expr_to_typename_wire(&prop.type_expr, convertible);
    let field_ty = if prop.nullable && prop.required {
        TypeName::optional(inner_ty)
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
    fb.build().expect("FieldSpec builds")
}

/// Build a field for the ergonomic interface (camelCase property name).
fn build_field_camel(prop: &IrProperty) -> FieldSpec {
    let camel = prop.name.to_lower_camel_case();
    let field_name = if is_valid_ts_identifier(&camel) {
        camel
    } else {
        format!("'{}'", camel)
    };
    let inner_ty = type_expr_to_typename(&prop.type_expr);
    let field_ty = if prop.nullable && prop.required {
        TypeName::optional(inner_ty)
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
    fb.build().expect("FieldSpec builds")
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
        IrTypeExpr::Nullable(inner) => {
            TypeName::optional(type_expr_to_typename_wire(inner, convertible))
        }
        IrTypeExpr::Map(inner) => TypeName::generic(
            TypeName::primitive("Record"),
            vec![
                TypeName::primitive("string"),
                type_expr_to_typename_wire(inner, convertible),
            ],
        ),
        other => type_expr_to_typename(other),
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
        IrTypeExpr::StringEnum(values) => values
            .iter()
            .map(|v| format!("'{v}'"))
            .collect::<Vec<_>>()
            .join(" | "),
        IrTypeExpr::Array(inner) => {
            let inner_str = type_expr_str(inner);
            if is_compound_type(inner) {
                format!("readonly ({inner_str})[]")
            } else {
                format!("readonly {inner_str}[]")
            }
        }
        IrTypeExpr::Nullable(inner) => format!("{} | null", type_expr_str(inner)),
        IrTypeExpr::Map(inner) => format!("Record<string, {}>", type_expr_str(inner)),
        IrTypeExpr::Union(members) => members
            .iter()
            .map(type_expr_str)
            .collect::<Vec<_>>()
            .join(" | "),
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
        IrTypeExpr::StringEnum(values) => values
            .iter()
            .map(|v| format!("'{v}'"))
            .collect::<Vec<_>>()
            .join(" | "),
        IrTypeExpr::Array(inner) => {
            let inner_str = type_expr_str_wire(inner, convertible);
            if is_compound_type(inner) {
                format!("readonly ({inner_str})[]")
            } else {
                format!("readonly {inner_str}[]")
            }
        }
        IrTypeExpr::Nullable(inner) => format!("{} | null", type_expr_str_wire(inner, convertible)),
        IrTypeExpr::Map(inner) => {
            format!("Record<string, {}>", type_expr_str_wire(inner, convertible))
        }
        IrTypeExpr::Union(members) => members
            .iter()
            .map(|m| type_expr_str_wire(m, convertible))
            .collect::<Vec<_>>()
            .join(" | "),
        IrTypeExpr::Any => "unknown".to_string(),
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
) -> CodeBlock {
    let fn_name = format!("{}FromJSON", fn_base_name(name));
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!(
        "export function {fn_name}(json: {wire_name}): {name} {{"
    ));
    lines.push("  return {".to_string());

    for (_json_name, prop) in &obj.properties {
        let camel = prop.name.to_lower_camel_case();
        let ergo_key = obj_literal_key(&camel);
        let wire_access = wire_field_access(&prop.name);
        let conversion = from_json_expr(&prop.type_expr, &wire_access, !prop.required, convertible);
        lines.push(format!("    {ergo_key}: {conversion},"));
    }

    lines.push("  };".to_string());
    lines.push("}".to_string());

    CodeBlock::of(&lines.join("\n"), ()).expect("CodeBlock builds")
}

/// Build the `export function nameToJSON(value: Name): Name$Wire` function body.
fn build_to_json_fn(
    name: &str,
    wire_name: &str,
    obj: &IrObject,
    convertible: &HashSet<String>,
) -> CodeBlock {
    let fn_name = format!("{}ToJSON", fn_base_name(name));
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!(
        "export function {fn_name}(value: {name}): {wire_name} {{"
    ));
    lines.push("  return {".to_string());

    for (_json_name, prop) in &obj.properties {
        let camel = prop.name.to_lower_camel_case();
        let wire_key = obj_literal_key(&prop.name);
        let ergo_access = ergo_field_access(&camel);
        let conversion = to_json_expr(&prop.type_expr, &ergo_access, !prop.required, convertible);
        lines.push(format!("    {wire_key}: {conversion},"));
    }

    lines.push("  };".to_string());
    lines.push("}".to_string());

    CodeBlock::of(&lines.join("\n"), ()).expect("CodeBlock builds")
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
    convertible: &HashSet<String>,
) -> String {
    if optional {
        let inner = from_json_expr_inner(expr, access, convertible);
        if needs_conversion(expr, convertible) {
            format!("{access} !== undefined ? {inner} : undefined")
        } else {
            inner
        }
    } else {
        from_json_expr_inner(expr, access, convertible)
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
    convertible: &HashSet<String>,
) -> String {
    if optional {
        let inner = to_json_expr_inner(expr, access, convertible);
        if needs_conversion(expr, convertible) {
            format!("{access} !== undefined ? {inner} : undefined")
        } else {
            inner
        }
    } else {
        to_json_expr_inner(expr, access, convertible)
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
fn emit_enum_file_from(schema: &IrSchema, en: &IrEnum, flags: EmitFlags) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();
    let union_body = enum_union_body(en)?;

    let type_alias = sigil_quote!(TypeScript {
        export type $N(name.as_str()) = $L(union_body.as_str());
    })
    .ok()?;

    let filename = format!("{}.ts", name);
    let mut fb = FileSpec::builder(&filename);
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
            CodeBlock::of(&format!("    {key_lit}: {val_literal},"), ()).ok()
        })
        .collect();

    sigil_quote!(TypeScript {
        $L(format!("export const {name} = {{"))
        $C_each(entries);
        $L("};")
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
fn emit_alias_file(schema: &IrSchema, expr: &IrTypeExpr) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();
    let rhs = type_expr_to_typename(expr);

    let type_alias = sigil_quote!(TypeScript {
        export type $N(name.as_str()) = $T(rhs);
    })
    .ok()?;

    let filename = format!("{}.ts", name);
    let mut fb = FileSpec::builder(&filename);
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
) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();

    if flags.property_naming_camel_case && convertible.contains(&schema.name) {
        return emit_union_file_camel_case(schema, union, &name, convertible);
    }

    let mut members: Vec<TypeName> = union.members.iter().map(type_expr_to_typename).collect();
    if union.nullable {
        members.push(TypeName::primitive("null"));
    }
    let union_ty = TypeName::union(members);

    let type_alias = sigil_quote!(TypeScript {
        export type $N(name.as_str()) = $T(union_ty);
    })
    .ok()?;

    let filename = format!("{}.ts", name);
    let mut fb = FileSpec::builder(&filename);
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
) -> Option<FileSpec> {
    if intersection.members.is_empty() {
        return None;
    }
    let name = schema.name.to_pascal_case();

    if flags.property_naming_camel_case && convertible.contains(&schema.name) {
        return emit_intersection_file_camel_case(schema, intersection, &name, convertible);
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
    let mut fb = FileSpec::builder(&filename);
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
    tu: &IrTaggedUnion,
    flags: EmitFlags,
    convertible: &HashSet<String>,
) -> Option<FileSpec> {
    if tu.variants.is_empty() {
        return None;
    }

    let name = schema.name.to_pascal_case();

    if flags.property_naming_camel_case {
        return emit_tagged_union_file_camel_case(schema, tu, &name, flags, convertible);
    }

    // Build `export type Name = <piece> | <piece>;` where each piece
    // carries `%T` slots for the variant's content type. Adjacent / External
    // shapes don't fit TypeName's structural variants, so the tag wrapping
    // goes into the literal format fragment and the content rides on %T.
    let mut format = format!("export type {} = ", name);
    let mut pieces: Vec<String> = Vec::with_capacity(tu.variants.len());
    let mut args: Vec<Arg> = Vec::with_capacity(tu.variants.len() * 2);

    for variant in &tu.variants {
        let content_ty = type_expr_to_typename(&variant.content_type);
        let (piece, piece_args) = render_variant_piece(
            &tu.tagging,
            &tu.discriminator_field,
            &variant.discriminator_value,
            content_ty,
        );
        pieces.push(piece);
        args.extend(piece_args);
    }

    format.push_str(&pieces.join(" | "));
    format.push(';');

    // sigil_quote! needs compile-time format; fall back to CodeBlock builder
    // for the dynamic variant count.
    let mut cb = CodeBlock::builder();
    cb.add(&format, args);
    let code = cb.build().ok()?;

    let filename = format!("{}.ts", name);
    let mut fb = FileSpec::builder(&filename);
    if let Some(doc) = &schema.description {
        fb = fb.add_raw(&format!("/** {doc} */\n"));
    }
    fb = fb.add_code(code);

    if flags.emit_type_guards {
        let guards = build_tagged_union_type_guards(&name, tu);
        for guard in guards {
            fb = fb.add_code(guard);
        }
    }

    fb.build().ok()
}

/// Emit a tagged union file with dual types + fromJSON/toJSON when camelCase.
fn emit_tagged_union_file_camel_case(
    schema: &IrSchema,
    tu: &IrTaggedUnion,
    name: &str,
    flags: EmitFlags,
    convertible: &HashSet<String>,
) -> Option<FileSpec> {
    let wire_name = format!("{}$Wire", name);
    let filename = format!("{}.ts", name);
    let disc_field = &tu.discriminator_field;
    let disc_field_camel = disc_field.to_lower_camel_case();

    // --- $Wire type (wire discriminator field name + $Wire content refs) ---
    let mut wire_fmt = format!("export type {wire_name} = ");
    let mut wire_pieces: Vec<String> = Vec::new();
    let mut wire_args: Vec<Arg> = Vec::new();
    for variant in &tu.variants {
        let content_ty = type_expr_to_typename_wire(&variant.content_type, convertible);
        let (piece, piece_args) = render_variant_piece(
            &tu.tagging,
            disc_field,
            &variant.discriminator_value,
            content_ty,
        );
        wire_pieces.push(piece);
        wire_args.extend(piece_args);
    }
    wire_fmt.push_str(&wire_pieces.join(" | "));
    wire_fmt.push(';');
    let mut wire_cb = CodeBlock::builder();
    wire_cb.add(&wire_fmt, wire_args);
    let wire_code = wire_cb.build().ok()?;

    // --- Ergonomic type (camelCase discriminator field name + ergonomic content refs) ---
    let mut ergo_fmt = format!("export type {name} = ");
    let mut ergo_pieces: Vec<String> = Vec::new();
    let mut ergo_args: Vec<Arg> = Vec::new();
    for variant in &tu.variants {
        let content_ty = type_expr_to_typename(&variant.content_type);
        let (piece, piece_args) = render_variant_piece(
            &tu.tagging,
            &disc_field_camel,
            &variant.discriminator_value,
            content_ty,
        );
        ergo_pieces.push(piece);
        ergo_args.extend(piece_args);
    }
    ergo_fmt.push_str(&ergo_pieces.join(" | "));
    ergo_fmt.push(';');
    let mut ergo_cb = CodeBlock::builder();
    ergo_cb.add(&ergo_fmt, ergo_args);
    let ergo_code = ergo_cb.build().ok()?;

    // --- fromJSON / toJSON ---
    let from_json = build_tagged_union_from_json(name, &wire_name, tu, convertible);
    let to_json = build_tagged_union_to_json(name, &wire_name, tu, convertible);

    let mut fb = FileSpec::builder(&filename);
    if let Some(doc) = &schema.description {
        fb = fb.add_raw(&format!("/** {doc} */\n"));
    }
    fb = fb.add_code(wire_code);
    fb = fb.add_code(ergo_code);

    // Add converter imports for Named content types (only if convertible)
    for variant in &tu.variants {
        if let IrTypeExpr::Named(ref_name) = &variant.content_type
            && convertible.contains(ref_name)
        {
            let pascal = ref_name.to_pascal_case();
            let module = format!("./{pascal}");
            let base = fn_base_name(&pascal);
            let from_fn = format!("{base}FromJSON");
            let to_fn = format!("{base}ToJSON");
            fb = fb.add_import(ImportSpec::named(&module, &from_fn));
            fb = fb.add_import(ImportSpec::named(&module, &to_fn));
        }
    }

    fb = fb.add_code(from_json);
    fb = fb.add_code(to_json);

    if flags.emit_type_guards {
        let guards = build_tagged_union_type_guards(name, tu);
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
    tu: &IrTaggedUnion,
    convertible: &HashSet<String>,
) -> CodeBlock {
    let fn_name = format!("{}FromJSON", fn_base_name(name));
    let disc_field = &tu.discriminator_field;
    let disc_field_camel = disc_field.to_lower_camel_case();
    let disc_access = if is_valid_ts_identifier(disc_field) {
        format!("json.{disc_field}")
    } else {
        format!("json['{disc_field}']")
    };

    let mut lines: Vec<String> = Vec::new();
    lines.push(format!(
        "export function {fn_name}(json: {wire_name}): {name} {{"
    ));
    lines.push(format!("  switch ({disc_access}) {{"));

    for variant in &tu.variants {
        let val = &variant.discriminator_value;
        let case_body = match &tu.tagging {
            TaggingStyle::Internal => {
                if let IrTypeExpr::Named(ref_name) = &variant.content_type {
                    if convertible.contains(ref_name) {
                        let pascal = ref_name.to_pascal_case();
                        let converter = format!("{}FromJSON", fn_base_name(&pascal));
                        format!("return {{ {disc_field_camel}: '{val}', ...{converter}(json) }};")
                    } else {
                        format!("return {{ {disc_field_camel}: '{val}', ...json }};")
                    }
                } else {
                    format!("return {{ {disc_field_camel}: '{val}', ...json }};")
                }
            }
            TaggingStyle::Adjacent { content_field } => {
                let content_camel = content_field.to_lower_camel_case();
                let content_access = if is_valid_ts_identifier(content_field) {
                    format!("json.{content_field}")
                } else {
                    format!("json['{content_field}']")
                };
                if let IrTypeExpr::Named(ref_name) = &variant.content_type {
                    if convertible.contains(ref_name) {
                        let pascal = ref_name.to_pascal_case();
                        let converter = format!("{}FromJSON", fn_base_name(&pascal));
                        format!(
                            "return {{ {disc_field_camel}: '{val}', {content_camel}: {converter}({content_access}) }};"
                        )
                    } else {
                        format!(
                            "return {{ {disc_field_camel}: '{val}', {content_camel}: {content_access} }};"
                        )
                    }
                } else {
                    format!(
                        "return {{ {disc_field_camel}: '{val}', {content_camel}: {content_access} }};"
                    )
                }
            }
            TaggingStyle::External => {
                let val_access = format!("json['{val}']");
                if let IrTypeExpr::Named(ref_name) = &variant.content_type {
                    if convertible.contains(ref_name) {
                        let pascal = ref_name.to_pascal_case();
                        let converter = format!("{}FromJSON", fn_base_name(&pascal));
                        format!("return {{ '{val}': {converter}({val_access}) }};")
                    } else {
                        format!("return {{ '{val}': {val_access} }};")
                    }
                } else {
                    format!("return {{ '{val}': {val_access} }};")
                }
            }
        };
        lines.push(format!("    case '{val}': {case_body}"));
    }

    lines.push("  }".to_string());
    lines.push("}".to_string());

    CodeBlock::of(&lines.join("\n"), ()).expect("CodeBlock builds")
}

/// Build toJSON for a tagged union: switch on the camelCase discriminator field.
fn build_tagged_union_to_json(
    name: &str,
    wire_name: &str,
    tu: &IrTaggedUnion,
    convertible: &HashSet<String>,
) -> CodeBlock {
    let fn_name = format!("{}ToJSON", fn_base_name(name));
    let disc_field = &tu.discriminator_field;
    let disc_field_camel = disc_field.to_lower_camel_case();
    let disc_access = if is_valid_ts_identifier(&disc_field_camel) {
        format!("value.{disc_field_camel}")
    } else {
        format!("value['{disc_field_camel}']")
    };

    let mut lines: Vec<String> = Vec::new();
    lines.push(format!(
        "export function {fn_name}(value: {name}): {wire_name} {{"
    ));
    lines.push(format!("  switch ({disc_access}) {{"));

    for variant in &tu.variants {
        let val = &variant.discriminator_value;
        let disc_wire_key = if is_valid_ts_identifier(disc_field) {
            disc_field.clone()
        } else {
            format!("'{disc_field}'")
        };
        let case_body = match &tu.tagging {
            TaggingStyle::Internal => {
                if let IrTypeExpr::Named(ref_name) = &variant.content_type {
                    if convertible.contains(ref_name) {
                        let pascal = ref_name.to_pascal_case();
                        let converter = format!("{}ToJSON", fn_base_name(&pascal));
                        format!(
                            "return {{ {disc_wire_key}: '{val}', ...{converter}(value) }} as {wire_name};"
                        )
                    } else {
                        format!("return {{ {disc_wire_key}: '{val}', ...value }} as {wire_name};")
                    }
                } else {
                    format!("return {{ {disc_wire_key}: '{val}', ...value }} as {wire_name};")
                }
            }
            TaggingStyle::Adjacent { content_field } => {
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
                if let IrTypeExpr::Named(ref_name) = &variant.content_type {
                    if convertible.contains(ref_name) {
                        let pascal = ref_name.to_pascal_case();
                        let converter = format!("{}ToJSON", fn_base_name(&pascal));
                        format!(
                            "return {{ {disc_wire_key}: '{val}', {content_wire_key}: {converter}({content_access}) }};"
                        )
                    } else {
                        format!(
                            "return {{ {disc_wire_key}: '{val}', {content_wire_key}: {content_access} }};"
                        )
                    }
                } else {
                    format!(
                        "return {{ {disc_wire_key}: '{val}', {content_wire_key}: {content_access} }};"
                    )
                }
            }
            TaggingStyle::External => {
                let val_access = format!("value['{val}']");
                if let IrTypeExpr::Named(ref_name) = &variant.content_type {
                    if convertible.contains(ref_name) {
                        let pascal = ref_name.to_pascal_case();
                        let converter = format!("{}ToJSON", fn_base_name(&pascal));
                        format!("return {{ '{val}': {converter}({val_access}) }};")
                    } else {
                        format!("return {{ '{val}': {val_access} }};")
                    }
                } else {
                    format!("return {{ '{val}': {val_access} }};")
                }
            }
        };
        lines.push(format!("    case '{val}': {case_body}"));
    }

    lines.push("  }".to_string());
    lines.push("}".to_string());

    CodeBlock::of(&lines.join("\n"), ()).expect("CodeBlock builds")
}

// ---------------------------------------------------------------------------
// Union camelCase: cast-through pattern
// ---------------------------------------------------------------------------

fn emit_union_file_camel_case(
    schema: &IrSchema,
    union: &IrUnion,
    name: &str,
    convertible: &HashSet<String>,
) -> Option<FileSpec> {
    let filename = format!("{}.ts", name);
    let mut fb = FileSpec::builder(&filename);

    if let Some(doc) = &schema.description {
        fb = fb.add_raw(&format!("/** {doc} */\n"));
    }

    // $Wire type alias
    let mut wire_members: Vec<String> = union
        .members
        .iter()
        .map(|m| type_expr_str_wire(m, convertible))
        .collect();
    if union.nullable {
        wire_members.push("null".to_string());
    }
    let wire_body = wire_members.join(" | ");
    fb = fb.add_raw(&format!("export type {name}$Wire = {wire_body};\n"));

    // Ergonomic type alias
    let mut ergo_members: Vec<String> = union.members.iter().map(type_expr_str).collect();
    if union.nullable {
        ergo_members.push("null".to_string());
    }
    let ergo_body = ergo_members.join(" | ");
    fb = fb.add_raw(&format!("export type {name} = {ergo_body};\n"));

    // fromJSON: cast-through
    let base = fn_base_name(name);
    fb = fb.add_raw(&format!(
        "export function {base}FromJSON(json: {name}$Wire): {name} {{\n  return json as unknown as {name};\n}}\n"
    ));

    // toJSON: cast-through
    fb = fb.add_raw(&format!(
        "export function {base}ToJSON(value: {name}): {name}$Wire {{\n  return value as unknown as {name}$Wire;\n}}\n"
    ));

    // Add imports for Named members (wire + ergonomic types only if convertible)
    for member in &union.members {
        if let IrTypeExpr::Named(ref_name) = member {
            let member_pascal = ref_name.to_pascal_case();
            fb = fb.add_import(ImportSpec::named_type(
                &format!("./{member_pascal}"),
                &member_pascal,
            ));
            if convertible.contains(ref_name) {
                fb = fb.add_import(ImportSpec::named_type(
                    &format!("./{member_pascal}"),
                    &format!("{member_pascal}$Wire"),
                ));
            }
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
) -> Option<FileSpec> {
    let filename = format!("{}.ts", name);
    let mut fb = FileSpec::builder(&filename);

    if let Some(doc) = &schema.description {
        fb = fb.add_raw(&format!("/** {doc} */\n"));
    }

    // $Wire type alias: A$Wire & B$Wire & ... (only $Wire for convertible refs)
    let wire_members: Vec<String> = intersection
        .members
        .iter()
        .map(|m| type_expr_str_wire(m, convertible))
        .collect();
    let wire_body = wire_members.join(" & ");
    fb = fb.add_raw(&format!("export type {name}$Wire = {wire_body};\n"));

    // Ergonomic type alias: A & B & ...
    let ergo_members: Vec<String> = intersection.members.iter().map(type_expr_str).collect();
    let ergo_body = ergo_members.join(" & ");
    fb = fb.add_raw(&format!("export type {name} = {ergo_body};\n"));

    // fromJSON: spread each convertible Named member's converter
    let base = fn_base_name(name);
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

    fb = fb.add_raw(&format!(
        "export function {base}FromJSON(json: {name}$Wire): {name} {{\n  return {{ {} }} as {name};\n}}\n",
        from_spreads.join(", ")
    ));

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

    fb = fb.add_raw(&format!(
        "export function {base}ToJSON(value: {name}): {name}$Wire {{\n  return {{ {} }} as {name}$Wire;\n}}\n",
        to_spreads.join(", ")
    ));

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

/// Build `is*` type guard functions for a tagged union.
///
/// Returns one CodeBlock per contentful (non-empty) variant.
fn build_tagged_union_type_guards(name: &str, tu: &IrTaggedUnion) -> Vec<CodeBlock> {
    tu.variants
        .iter()
        .filter(|variant| {
            !is_unspecified_variant(
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
            );

            let predicate = format!("value is {guard_type}");
            sigil_quote!(TypeScript {
                $L(format!("export function {guard_name}("))
                $L(format!("    value: {name}"))
                $L(format!("): {predicate} {{"))
                $L(format!("    return {check_body};"))
                $L("}")
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

// ---------------------------------------------------------------------------
// Variant helpers — shared between tagged-union type emission and guard emission
// ---------------------------------------------------------------------------

/// Build the TS type expression for one variant, with `content` slotted in.
fn variant_type_format(
    tagging: &TaggingStyle,
    disc_field: &str,
    disc_value: &str,
    content: &str,
) -> String {
    match tagging {
        TaggingStyle::Internal => {
            format!("({{ {disc_field}: '{disc_value}' }} & {content})")
        }
        TaggingStyle::Adjacent { content_field } => {
            format!("{{ {disc_field}: '{disc_value}'; {content_field}: {content} }}")
        }
        TaggingStyle::External => {
            format!("{{ '{disc_value}': {content} }}")
        }
    }
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
            IrTypeExpr::Nullable(inner) => write!(f, "{} | null", TsTypeDisplay(inner)),
            IrTypeExpr::Array(inner) => write!(f, "readonly {}[]", TsTypeDisplay(inner)),
            IrTypeExpr::Map(inner) => {
                write!(f, "Record<string, {}>", TsTypeDisplay(inner))
            }
            IrTypeExpr::StringLiteral(s) => write!(f, "'{s}'"),
            IrTypeExpr::StringEnum(values) => {
                let parts: Vec<String> = values.iter().map(|v| format!("'{v}'")).collect();
                write!(f, "{}", parts.join(" | "))
            }
            IrTypeExpr::Union(members) => {
                let parts: Vec<String> = members
                    .iter()
                    .map(|m| TsTypeDisplay(m).to_string())
                    .collect();
                write!(f, "{}", parts.join(" | "))
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
) -> (String, String) {
    let check = variant_check_body(tagging, disc_field, disc_value);
    let ty = variant_type_format(
        tagging,
        disc_field,
        disc_value,
        &TsTypeDisplay(content_type).to_string(),
    );
    (check, ty)
}

/// Render one variant of a tagged union as a format fragment + its args.
///
/// Returns `(format_piece, args)` where `format_piece` contains `%T` slots
/// in the same order as `args`. Caller joins pieces with ` | `.
fn render_variant_piece(
    tagging: &TaggingStyle,
    discriminator_field: &str,
    discriminator_value: &str,
    content_ty: TypeName,
) -> (String, Vec<Arg>) {
    let fmt = variant_type_format(tagging, discriminator_field, discriminator_value, "%T");
    (fmt, vec![Arg::TypeName(content_ty)])
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
    Some(parts.join(" | "))
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

fn build_field(prop: &IrProperty) -> FieldSpec {
    let field_name = if is_valid_ts_identifier(&prop.name) {
        prop.name.clone()
    } else {
        format!("'{}'", prop.name)
    };
    let inner_ty = type_expr_to_typename(&prop.type_expr);
    let field_ty = if prop.nullable && prop.required {
        TypeName::optional(inner_ty)
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
    fb.build().expect("FieldSpec builds")
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
        IrTypeExpr::Nullable(inner) => TypeName::optional(type_expr_to_typename(inner)),
        IrTypeExpr::StringLiteral(s) => TypeName::raw(&format!("'{s}'")),
        IrTypeExpr::StringEnum(values) => TypeName::union(
            values
                .iter()
                .map(|v| TypeName::raw(&format!("'{v}'")))
                .collect(),
        ),
        IrTypeExpr::Map(inner) => TypeName::generic(
            TypeName::primitive("Record"),
            vec![TypeName::primitive("string"), type_expr_to_typename(inner)],
        ),
        IrTypeExpr::Union(members) => {
            TypeName::union(members.iter().map(type_expr_to_typename).collect())
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
pub fn generate_model_files(ir: &IrSpec, flags: EmitFlags) -> Result<Vec<FileInfo>, String> {
    let header = super::project_files::render_file_header(&ir.info);
    let convertible = build_convertible_set(ir, flags);
    let mut files = Vec::with_capacity(ir.schemas.len());

    for (name, schema) in &ir.schemas {
        let file_spec = emit_model_file(schema, flags, &convertible).ok_or_else(|| {
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
        IrPrimitive::String
        | IrPrimitive::Binary
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
