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
use openapi_nexus_ir::types::{IrPrimitive, IrProperty, IrSchema, IrSchemaKind, IrTypeExpr};
use sigil_stitch::lang::typescript::TypeScript;
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
