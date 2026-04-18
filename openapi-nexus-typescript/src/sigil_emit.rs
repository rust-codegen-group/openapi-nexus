//! Phase 3: sigil-stitch emit for IR schemas.
//!
//! Dispatches on `IrSchemaKind` and produces a `FileSpec<TypeScript>` per
//! schema. Each model file carries a `@generated` header and the declaration
//! (interface / type alias / union / etc.) matching
//! `docs/target-output-spec.md`.
//!
//! Coverage:
//! - `Object` — `export interface`
//! - `Enum` — string-literal union alias (string/integer/number/mixed values)
//! - `Alias` — `export type X = Y;`
//! - `Union` — `export type X = A | B | C [| null];` (untagged oneOf/anyOf)
//! - `Intersection` — `export type X = A & B & C;` (allOf)
//! - Remaining kind (`TaggedUnion`) lands in Stage C.
//!
//! Not yet wired into [`crate::codegen::TypeScriptFetchCodeGenerator`]. This
//! lives in the public surface so integration tests can exercise it directly.
//!
//! # Known upstream gaps
//!
//! 1. **Field-level doc comment indentation is broken.** Inner `*` lines sit
//!    flush-left instead of aligning with the field indent. Cosmetic.
//! 2. **`Visibility::Public` on interface fields leaks `public` keyword.** TS
//!    interfaces don't accept `public` — field visibility is left unset.

use heck::ToLowerCamelCase;
use openapi_nexus_core::traits::file_writer::FileInfo;
use openapi_nexus_ir::types::{
    IrEnum, IrEnumValueType, IrInfo, IrIntersection, IrObject, IrPrimitive, IrProperty, IrSchema,
    IrSchemaKind, IrSpec, IrTypeExpr, IrUnion,
};
use sigil_stitch::lang::typescript::TypeScript;
use sigil_stitch::prelude::sigil_quote;
use sigil_stitch::spec::field_spec::FieldSpec;
use sigil_stitch::spec::file_spec::FileSpec;
use sigil_stitch::spec::modifiers::{TypeKind, Visibility};
use sigil_stitch::spec::type_spec::TypeSpec;
use sigil_stitch::type_name::TypeName;

/// Emit a TypeScript model file for an IR schema. Dispatches on `schema.kind`.
///
/// Returns `None` only for kinds not yet implemented (Stage B/C).
pub fn emit_model_file(schema: &IrSchema) -> Option<FileSpec<TypeScript>> {
    match &schema.kind {
        IrSchemaKind::Object(obj) => Some(emit_object_file(schema, obj)),
        IrSchemaKind::Enum(en) => emit_enum_file_from(schema, en),
        IrSchemaKind::Alias(expr) => emit_alias_file(schema, expr),
        IrSchemaKind::Union(u) => emit_union_file(schema, u),
        IrSchemaKind::Intersection(i) => emit_intersection_file(schema, i),
        IrSchemaKind::TaggedUnion(_) => None,
    }
}

/// Back-compat alias used by the enum prototype test. Prefer `emit_model_file`.
pub fn emit_enum_file(schema: &IrSchema) -> Option<FileSpec<TypeScript>> {
    let IrSchemaKind::Enum(en) = &schema.kind else {
        return None;
    };
    emit_enum_file_from(schema, en)
}

fn emit_object_file(schema: &IrSchema, obj: &IrObject) -> FileSpec<TypeScript> {
    let mut tb = TypeSpec::<TypeScript>::builder(&schema.name, TypeKind::Interface);
    // `Visibility::Public` on the TypeSpec emits `export`; on an interface
    // FieldSpec it leaks a stray `public` keyword, so field visibility stays
    // unset.
    tb.visibility(Visibility::Public);
    if let Some(doc) = &schema.description {
        tb.doc(doc);
    }

    for (_json_name, prop) in &obj.properties {
        tb.add_field(build_field(prop));
    }

    let filename = format!("{}.ts", schema.name);
    let mut fb = FileSpec::<TypeScript>::builder(&filename);
    fb.add_type(tb.build().expect("TypeSpec builds"));
    fb.build().expect("FileSpec builds")
}

/// Enum file: `export type Name = 'a' | 'b' | 1 | 2 | null;`
///
/// Handles string / integer / number / mixed value types. Returns `None` if
/// any enum value can't be rendered as a TS literal.
fn emit_enum_file_from(schema: &IrSchema, en: &IrEnum) -> Option<FileSpec<TypeScript>> {
    let name = schema.name.clone();
    let union_body = enum_union_body(en)?;

    let type_alias = sigil_quote!(TypeScript {
        export type $N(name.as_str()) = $L(union_body.as_str());
    })
    .ok()?;

    let filename = format!("{}.ts", schema.name);
    let mut fb = FileSpec::<TypeScript>::builder(&filename);
    if let Some(doc) = &schema.description {
        // FileSpec has no structural doc slot — use a raw prelude comment.
        fb.add_raw(&format!("/** {doc} */\n"));
    }
    fb.add_code(type_alias);
    fb.build().ok()
}

/// Alias file: `export type Name = Inner;` — where `Inner` may be a named ref
/// (import auto-emitted), a primitive, a readonly array, etc.
fn emit_alias_file(schema: &IrSchema, expr: &IrTypeExpr) -> Option<FileSpec<TypeScript>> {
    let name = schema.name.clone();
    let rhs = type_expr_to_typename(expr);

    let type_alias = sigil_quote!(TypeScript {
        export type $N(name.as_str()) = $T(rhs);
    })
    .ok()?;

    let filename = format!("{}.ts", schema.name);
    let mut fb = FileSpec::<TypeScript>::builder(&filename);
    if let Some(doc) = &schema.description {
        fb.add_raw(&format!("/** {doc} */\n"));
    }
    fb.add_code(type_alias);
    fb.build().ok()
}

/// Union file: `export type Name = A | B | C [| null];`
///
/// `nullable` appends a `null` member. Imports for each `IrTypeExpr::Named`
/// member are tracked via `TypeName::Importable` inside `TypeName::union`.
fn emit_union_file(schema: &IrSchema, union: &IrUnion) -> Option<FileSpec<TypeScript>> {
    let name = schema.name.clone();
    let mut members: Vec<TypeName<TypeScript>> =
        union.members.iter().map(type_expr_to_typename).collect();
    if union.nullable {
        members.push(TypeName::primitive("null"));
    }
    let union_ty = TypeName::union(members);

    let type_alias = sigil_quote!(TypeScript {
        export type $N(name.as_str()) = $T(union_ty);
    })
    .ok()?;

    let filename = format!("{}.ts", schema.name);
    let mut fb = FileSpec::<TypeScript>::builder(&filename);
    if let Some(doc) = &schema.description {
        fb.add_raw(&format!("/** {doc} */\n"));
    }
    fb.add_code(type_alias);
    fb.build().ok()
}

/// Intersection file: `export type Name = A & B & C;`
///
/// Empty `members` is degenerate — skip by returning `None` so the caller
/// reports it as unsupported rather than emitting `export type Name = ;`.
fn emit_intersection_file(
    schema: &IrSchema,
    intersection: &IrIntersection,
) -> Option<FileSpec<TypeScript>> {
    if intersection.members.is_empty() {
        return None;
    }
    let name = schema.name.clone();
    let members: Vec<TypeName<TypeScript>> = intersection
        .members
        .iter()
        .map(type_expr_to_typename)
        .collect();
    let inter_ty = TypeName::intersection(members);

    let type_alias = sigil_quote!(TypeScript {
        export type $N(name.as_str()) = $T(inter_ty);
    })
    .ok()?;

    let filename = format!("{}.ts", schema.name);
    let mut fb = FileSpec::<TypeScript>::builder(&filename);
    if let Some(doc) = &schema.description {
        fb.add_raw(&format!("/** {doc} */\n"));
    }
    fb.add_code(type_alias);
    fb.build().ok()
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

fn build_field(prop: &IrProperty) -> FieldSpec<TypeScript> {
    let ts_field_name = prop.name.to_lower_camel_case();
    let inner_ty = type_expr_to_typename(&prop.type_expr);
    let field_ty = if prop.nullable {
        TypeName::optional(inner_ty)
    } else {
        inner_ty
    };

    let mut fb = FieldSpec::<TypeScript>::builder(&ts_field_name, field_ty);
    fb.is_readonly();
    if !prop.required {
        fb.is_optional();
    }
    if let Some(desc) = &prop.description {
        fb.doc(desc);
    }
    fb.build().expect("FieldSpec builds")
}

fn type_expr_to_typename(expr: &IrTypeExpr) -> TypeName<TypeScript> {
    match expr {
        IrTypeExpr::Named(name) => {
            let module = format!("./{name}");
            TypeName::importable_type(&module, name)
        }
        IrTypeExpr::Primitive(p) => TypeName::primitive(primitive_to_ts(p)),
        IrTypeExpr::Array(inner) => TypeName::readonly_array(type_expr_to_typename(inner)),
        IrTypeExpr::Nullable(inner) => TypeName::optional(type_expr_to_typename(inner)),
        IrTypeExpr::StringLiteral(s) => TypeName::raw(&format!("'{s}'")),
        IrTypeExpr::StringEnum(values) => {
            TypeName::union(values.iter().map(|v| TypeName::raw(&format!("'{v}'"))).collect())
        }
        IrTypeExpr::Map(inner) => {
            TypeName::map(TypeName::primitive("string"), type_expr_to_typename(inner))
        }
        IrTypeExpr::Union(members) => {
            TypeName::union(members.iter().map(type_expr_to_typename).collect())
        }
        IrTypeExpr::Any => TypeName::primitive("unknown"),
    }
}

/// Lower every supported `IrSchema` in the spec into a sigil-rendered
/// `FileInfo`. Fails if a schema kind isn't yet implemented — Stage B/C close
/// that gap.
pub fn generate_model_files(ir: &IrSpec) -> Result<Vec<FileInfo>, String> {
    let header = render_file_header(&ir.info);
    let mut files = Vec::with_capacity(ir.schemas.len());

    for (name, schema) in &ir.schemas {
        let file_spec = emit_model_file(schema).ok_or_else(|| {
            format!(
                "sigil_emit: unsupported schema kind for {name}: {:?}",
                schema.kind
            )
        })?;
        let body = file_spec
            .render(100)
            .map_err(|e| format!("sigil_emit: render {name}: {e}"))?;
        let content = format!("{header}{body}");
        let filename = format!("{}.ts", schema.name);
        files.push(FileInfo::model(filename, content));
    }

    Ok(files)
}

/// Render the `@generated` file header matching `docs/target-output-spec.md`.
fn render_file_header(info: &IrInfo) -> String {
    let mut out = String::new();
    out.push_str("/**\n");
    out.push_str(" * @generated by openapi-nexus. Do not edit.\n");
    out.push_str(" *\n");
    out.push_str(&format!(" * {} — {}\n", info.title, info.version));
    if let Some(desc) = &info.description {
        for line in desc.lines() {
            out.push_str(&format!(" * {line}\n"));
        }
    }
    out.push_str(" */\n");
    out
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
