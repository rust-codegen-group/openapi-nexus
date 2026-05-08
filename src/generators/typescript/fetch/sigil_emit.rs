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
use heck::ToPascalCase;
use sigil_stitch::code_block::{Arg, CodeBlock};
use sigil_stitch::prelude::sigil_quote;
use sigil_stitch::spec::field_spec::FieldSpec;
use sigil_stitch::spec::file_spec::FileSpec;
use sigil_stitch::spec::modifiers::{TypeKind, Visibility};
use sigil_stitch::spec::type_spec::TypeSpec;
use sigil_stitch::type_name::TypeName;

/// Flags controlling optional TS emissions (const objects, type guards).
#[derive(Debug, Clone, Copy, Default)]
pub struct EmitFlags {
    pub emit_enum_constants: bool,
    pub emit_type_guards: bool,
}

/// Return the value-export names a schema contributes to the barrel.
///
/// - Enums with `emit_enum_constants`: the type name itself (the const object).
/// - Tagged unions with `emit_type_guards`: one `is{Variant}` per contentful variant.
/// - Everything else: empty.
pub fn value_exports_for_schema(schema: &IrSchema, flags: EmitFlags) -> Vec<String> {
    match &schema.kind {
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

/// Emit a TypeScript model file for an IR schema. Dispatches on `schema.kind`.
pub fn emit_model_file(schema: &IrSchema, flags: EmitFlags) -> Option<FileSpec> {
    match &schema.kind {
        IrSchemaKind::Object(obj) => Some(emit_object_file(schema, obj)),
        IrSchemaKind::Enum(en) => emit_enum_file_from(schema, en, flags),
        IrSchemaKind::Alias(expr) => emit_alias_file(schema, expr),
        IrSchemaKind::Union(u) => emit_union_file(schema, u),
        IrSchemaKind::Intersection(i) => emit_intersection_file(schema, i),
        IrSchemaKind::TaggedUnion(tu) => emit_tagged_union_file(schema, tu, flags),
    }
}

/// Back-compat alias used by the enum prototype test. Prefer `emit_model_file`.
pub fn emit_enum_file(schema: &IrSchema) -> Option<FileSpec> {
    let IrSchemaKind::Enum(en) = &schema.kind else {
        return None;
    };
    emit_enum_file_from(schema, en, EmitFlags::default())
}

fn emit_object_file(schema: &IrSchema, obj: &IrObject) -> FileSpec {
    let name = schema.name.to_pascal_case();
    // `Visibility::Public` on the TypeSpec emits `export`; on an interface
    // FieldSpec it leaks a stray `public` keyword, so field visibility stays
    // unset.
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
fn emit_union_file(schema: &IrSchema, union: &IrUnion) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();
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
fn emit_intersection_file(schema: &IrSchema, intersection: &IrIntersection) -> Option<FileSpec> {
    if intersection.members.is_empty() {
        return None;
    }
    let name = schema.name.to_pascal_case();
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
) -> Option<FileSpec> {
    if tu.variants.is_empty() {
        return None;
    }

    // Build `export type Name = <piece> | <piece>;` where each piece
    // carries `%T` slots for the variant's content type. Adjacent / External
    // shapes don't fit TypeName's structural variants, so the tag wrapping
    // goes into the literal format fragment and the content rides on %T.
    let name = schema.name.to_pascal_case();
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

/// Lower every `IrSchema` in the spec into a sigil-rendered `FileInfo`.
pub fn generate_model_files(ir: &IrSpec, flags: EmitFlags) -> Result<Vec<FileInfo>, String> {
    let header = super::project_files::render_file_header(&ir.info);
    let mut files = Vec::with_capacity(ir.schemas.len());

    for (name, schema) in &ir.schemas {
        let file_spec = emit_model_file(schema, flags).ok_or_else(|| {
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
