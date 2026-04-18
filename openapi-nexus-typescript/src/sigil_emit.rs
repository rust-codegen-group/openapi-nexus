//! Phase 2 spike: sigil-stitch emit for IR object schemas.
//!
//! Scope (intentionally narrow):
//! - `IrSchemaKind::Object` only
//! - `IrTypeExpr::Named`, `Primitive`, `Array` of named/primitive
//! - `IrProperty` `required` + `nullable` bits
//!
//! Not yet supported (Phase 3+): unions, enums, tagged unions, intersections,
//! aliases, maps, inline string enums, inline unions, validation, `any`.
//!
//! Not yet wired into [`crate::codegen::TypeScriptFetchCodeGenerator`]. This
//! lives in the public surface so integration tests can exercise it directly.
//!
//! # Gaps vs. `docs/target-output-spec.md`
//!
//! 1. ~~No element-level `readonly` on array types.~~ Fixed upstream by adding
//!    `TypeName::ReadonlyArray`; this module uses `readonly_array(inner)`.
//! 2. **Field-level doc comment indentation is broken.** Inner `*` lines sit
//!    flush-left instead of aligning with the field indent. Cosmetic but ugly.
//!    Upstream sigil-stitch fix required.
//! 3. **`Visibility::Public` on interface fields leaks `public` keyword.** TS
//!    interfaces don't accept `public` — we leave field visibility unset as a
//!    workaround. Setting it on the `TypeSpec` correctly produces `export`.

use heck::ToLowerCamelCase;
use openapi_nexus_core::traits::file_writer::FileInfo;
use openapi_nexus_ir::types::{
    IrEnum, IrEnumValueType, IrInfo, IrPrimitive, IrProperty, IrSchema, IrSchemaKind, IrSpec,
    IrTypeExpr,
};
use sigil_stitch::lang::typescript::TypeScript;
use sigil_stitch::prelude::sigil_quote;
use sigil_stitch::spec::field_spec::FieldSpec;
use sigil_stitch::spec::file_spec::FileSpec;
use sigil_stitch::spec::modifiers::{TypeKind, Visibility};
use sigil_stitch::spec::type_spec::TypeSpec;
use sigil_stitch::type_name::TypeName;

/// Emit a TypeScript model file for an `IrSchema::Object`.
///
/// Returns `None` for schema kinds the spike does not cover yet; callers can
/// fall back to the legacy template path for those.
pub fn emit_model_file(schema: &IrSchema) -> Option<FileSpec<TypeScript>> {
    let IrSchemaKind::Object(obj) = &schema.kind else {
        return None;
    };

    let mut tb = TypeSpec::<TypeScript>::builder(&schema.name, TypeKind::Interface);
    // Visibility::Public on the TypeSpec emits the `export` keyword; on an
    // interface FieldSpec it leaks a stray `public` keyword, so field
    // visibility is left unset.
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
    Some(fb.build().expect("FileSpec builds"))
}

/// Emit a TypeScript enum file as a string-literal union type alias.
///
/// Shape:
/// ```ts
/// export type PetStatus = 'available' | 'pending' | 'sold';
/// ```
///
/// String-valued enums only. Returns `None` for int/mixed until we decide
/// their shape. Sigil-quote sweet spot: single-line declaration with `$N`
/// (name identifier) and `$L` (prebuilt union body).
pub fn emit_enum_file(schema: &IrSchema) -> Option<FileSpec<TypeScript>> {
    let IrSchemaKind::Enum(en) = &schema.kind else {
        return None;
    };
    if en.value_type != IrEnumValueType::String {
        return None;
    }

    let name = schema.name.clone();
    let union_body = string_enum_union_body(en)?;

    let type_alias = sigil_quote!(TypeScript {
        export type $N(name.as_str()) = $L(union_body.as_str());
    })
    .ok()?;

    let filename = format!("{}.ts", schema.name);
    let mut fb = FileSpec::<TypeScript>::builder(&filename);
    if let Some(doc) = &schema.description {
        // FileSpec has no standalone doc slot; a raw line works for a prelude
        // comment. Structural builders ship docs attached to types/fields,
        // which is what we use elsewhere — this is the one spot we don't have
        // a structural target.
        fb.add_raw(&format!("/** {doc} */\n"));
    }
    fb.add_code(type_alias);
    fb.build().ok()
}

/// Build the string-literal union body: `'available' | 'pending' | 'sold'`.
fn string_enum_union_body(en: &IrEnum) -> Option<String> {
    let parts: Vec<String> = en
        .values
        .iter()
        .filter_map(|v| v.value.as_str().map(|s| format!("'{s}'")))
        .collect();
    if parts.len() != en.values.len() {
        return None;
    }
    Some(parts.join(" | "))
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
        // Out of spike scope: fall back to `unknown` so the test surfaces the gap.
        _ => TypeName::primitive("unknown"),
    }
}

/// Slice C entry point: lower every `IrSchemaKind::Object` in the spec into a
/// sigil-rendered `FileInfo` matching `docs/target-output-spec.md`.
///
/// Fails if the spec contains a non-Object schema kind (unions, enums,
/// intersections, tagged unions, aliases). Slice C fixtures must be Object-only.
/// Does not emit `models/index.ts` — the target spec drops the barrel.
pub fn generate_model_files(ir: &IrSpec) -> Result<Vec<FileInfo>, String> {
    let header = render_file_header(&ir.info);
    let mut files = Vec::with_capacity(ir.schemas.len());

    for (name, schema) in &ir.schemas {
        match &schema.kind {
            IrSchemaKind::Object(_) => {
                let file_spec = emit_model_file(schema)
                    .ok_or_else(|| format!("sigil_emit: {name} is not an Object schema"))?;
                let body = file_spec
                    .render(100)
                    .map_err(|e| format!("sigil_emit: render {name}: {e}"))?;
                let content = format!("{header}{body}");
                let filename = format!("{}.ts", schema.name);
                files.push(FileInfo::model(filename, content));
            }
            other => {
                return Err(format!(
                    "sigil_emit Slice C: unsupported schema kind for {name}: {other:?}"
                ));
            }
        }
    }

    Ok(files)
}

/// Render the `@generated` file header matching `docs/target-output-spec.md`.
///
/// Lines: `@generated …` marker, blank line, `{title} — {version}`, and (if
/// present) the description on a following line.
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
