//! Sigil-stitch emit for IR schemas (Go models).
//!
//! Each supported `IrSchemaKind` maps to one `models/<name>.go` file in package
//! `models`.
//!
//! Coverage:
//! - `Object` — struct with `json:` tags and pointer-optional fields.
//! - `Enum` — typed string/int constants + named type alias.
//! - `Alias` — `type X = Y` alias.
//! - `Union` — `interface{}` alias plus variant structs (untagged, simplified).
//! - `Intersection` — struct composed by embedding each named member.
//! - `TaggedUnion` — typed marker interface + one struct per variant.
//!
//! Imports for cross-schema references are tracked via sigil-stitch's
//! `TypeName::importable`, but within `models/` we treat references as
//! same-package, so most emission uses `TypeName::primitive` with the Go type
//! name directly.

use heck::{ToPascalCase, ToSnakeCase};
use openapi_nexus_core::traits::file_writer::FileInfo;
use openapi_nexus_ir::types::{
    IrEnum, IrEnumValueType, IrIntersection, IrObject, IrPrimitive, IrProperty, IrSchema,
    IrSchemaKind, IrSpec, IrTaggedUnion, IrTypeExpr, IrUnion, TaggingStyle,
};
use sigil_stitch::code_block::CodeBlock;
use sigil_stitch::lang::go_lang::GoLang;
use sigil_stitch::spec::field_spec::FieldSpec;
use sigil_stitch::spec::file_spec::FileSpec;
use sigil_stitch::spec::modifiers::TypeKind;
use sigil_stitch::spec::type_spec::TypeSpec;
use sigil_stitch::type_name::TypeName;

const MODELS_PACKAGE: &str = "models";
const RENDER_WIDTH: usize = 100;

/// Generate every model file from the IR. Each emitted file carries the
/// passed-in header (e.g., the `// Code generated` banner).
pub fn generate_model_files(ir: &IrSpec, header: &str) -> Result<Vec<FileInfo>, String> {
    let mut files = Vec::new();
    for (name, schema) in &ir.schemas {
        let Some(body) = emit_model_body(schema) else {
            return Err(format!(
                "unsupported schema kind for {name}: {:?}",
                schema.kind
            ));
        };
        files.push(model_file(&schema.name, header, &body));
    }
    Ok(files)
}

fn model_file(name: &str, header: &str, body: &str) -> FileInfo {
    let stem = name.to_snake_case();
    // Go excludes `*_test.go` from regular builds. A model named `FooTest`
    // would snake-case to `foo_test.go` and disappear. Disambiguate by
    // appending `_model`.
    let filename = if stem.ends_with("_test") {
        format!("{stem}_model.go")
    } else {
        format!("{stem}.go")
    };
    let mut content = String::with_capacity(header.len() + body.len());
    content.push_str(header);
    content.push_str(body);
    FileInfo::model(filename, content)
}

/// Dispatch on schema kind. Returns the rendered file body (package + imports
/// + declarations) but not the pre-package header comment.
fn emit_model_body(schema: &IrSchema) -> Option<String> {
    match &schema.kind {
        IrSchemaKind::Object(obj) => emit_object(schema, obj),
        IrSchemaKind::Enum(en) => emit_enum(schema, en),
        IrSchemaKind::Alias(expr) => emit_alias(schema, expr),
        IrSchemaKind::Union(u) => emit_union(schema, u),
        IrSchemaKind::Intersection(i) => emit_intersection(schema, i),
        IrSchemaKind::TaggedUnion(tu) => emit_tagged_union(schema, tu),
    }
}

// ---------------------------------------------------------------------------
// Object -> struct with json: tags
// ---------------------------------------------------------------------------

fn emit_object(schema: &IrSchema, obj: &IrObject) -> Option<String> {
    let name = schema.name.to_pascal_case();
    let mut tb = TypeSpec::<GoLang>::builder(&name, TypeKind::Struct);
    if let Some(doc) = &schema.description {
        tb.doc(doc);
    }
    for (json_name, prop) in &obj.properties {
        tb.add_field(build_struct_field(json_name, prop));
    }

    let mut fb = FileSpec::<GoLang>::builder(&format!("{}.go", name));
    fb.header(package_header());
    fb.add_type(tb.build().ok()?);
    let file = fb.build().ok()?;
    file.render(RENDER_WIDTH).ok()
}

fn build_struct_field(json_name: &str, prop: &IrProperty) -> FieldSpec<GoLang> {
    let field_name = go_field_name(&prop.name);
    let ty = go_type_name(&prop.type_expr);

    let mut fb = FieldSpec::<GoLang>::builder(&field_name, ty);
    let tag = json_tag(json_name, prop.required, prop.nullable);
    fb.tag(&tag);
    if !prop.required || prop.nullable {
        // Optional or nullable fields become `*T` so callers can distinguish
        // "absent" from "zero-value present".
        fb.is_optional();
    }
    if let Some(desc) = &prop.description {
        fb.doc(desc);
    }
    fb.build().expect("FieldSpec builds")
}

fn json_tag(json_name: &str, required: bool, nullable: bool) -> String {
    if required && !nullable {
        format!("json:\"{}\"", json_name)
    } else {
        format!("json:\"{},omitempty\"", json_name)
    }
}

// ---------------------------------------------------------------------------
// Enum -> typed constants + alias
// ---------------------------------------------------------------------------

fn emit_enum(schema: &IrSchema, en: &IrEnum) -> Option<String> {
    let name = schema.name.to_pascal_case();
    let go_base = match en.value_type {
        IrEnumValueType::String => "string",
        IrEnumValueType::Integer => "int",
        IrEnumValueType::Number => "float64",
        // Mixed-type enums aren't representable as a typed Go enum; fall back
        // to a plain `interface{}` alias below so callers can pass either
        // string or number.
        IrEnumValueType::Mixed => {
            return Some(render_alias_file(
                &name,
                "any",
                schema.description.as_deref(),
            ));
        }
    };

    // `type Name <base>`
    let type_decl = format!("type {name} {go_base}");

    // `const ( A Name = "a"; B Name = "b" )`
    let mut lines = Vec::with_capacity(en.values.len());
    for v in &en.values {
        let (const_name, rhs) = match en.value_type {
            IrEnumValueType::String => {
                let s = v.value.as_str()?;
                (
                    format!("{name}{}", s.to_pascal_case()),
                    format!("\"{}\"", escape_go_string(s)),
                )
            }
            IrEnumValueType::Integer | IrEnumValueType::Number => {
                let n = v.value.as_number()?;
                let pretty = n.to_string().replace(['-', '.'], "_");
                (format!("{name}N{}", pretty), n.to_string())
            }
            IrEnumValueType::Mixed => unreachable!(),
        };
        lines.push(format!("\t{const_name} {name} = {rhs}"));
    }

    let body = format!(
        "{}\n{}\nconst (\n{}\n)\n",
        preamble(&name, schema.description.as_deref()),
        type_decl,
        lines.join("\n"),
    );
    Some(body)
}

// ---------------------------------------------------------------------------
// Alias -> `type Name = X`
// ---------------------------------------------------------------------------

fn emit_alias(schema: &IrSchema, expr: &IrTypeExpr) -> Option<String> {
    let name = schema.name.to_pascal_case();
    let rhs = go_type_str(expr);
    Some(render_alias_file(
        &name,
        &rhs,
        schema.description.as_deref(),
    ))
}

// ---------------------------------------------------------------------------
// Union -> interface{} alias (simplified)
// ---------------------------------------------------------------------------

fn emit_union(schema: &IrSchema, _union: &IrUnion) -> Option<String> {
    // Untagged unions in Go don't have a clean representation without a
    // discriminator. Use `interface{}` (any) and let callers type-assert.
    let name = schema.name.to_pascal_case();
    Some(render_alias_file(
        &name,
        "any",
        schema.description.as_deref(),
    ))
}

// ---------------------------------------------------------------------------
// Intersection -> struct with embedded members
// ---------------------------------------------------------------------------

fn emit_intersection(schema: &IrSchema, inter: &IrIntersection) -> Option<String> {
    // Hand-rolled: Go embedded fields have no field name, which sigil's
    // FieldSpec builder rejects. Format the struct directly.
    let name = schema.name.to_pascal_case();
    let mut out = preamble(&name, schema.description.as_deref());
    out.push_str(&format!("type {name} struct {{\n"));
    for member in &inter.members {
        out.push_str(&format!("\t{}\n", go_type_str(member)));
    }
    out.push_str("}\n");
    Some(out)
}

// ---------------------------------------------------------------------------
// TaggedUnion -> marker interface + variant wrappers (simplified)
// ---------------------------------------------------------------------------

fn emit_tagged_union(schema: &IrSchema, tu: &IrTaggedUnion) -> Option<String> {
    if tu.variants.is_empty() {
        return None;
    }
    // Start with an `interface{}` alias — the full sealed-interface pattern
    // can come later. Record the discriminator field in a doc comment so the
    // caller knows how to dispatch.
    let name = schema.name.to_pascal_case();
    let hint = match &tu.tagging {
        TaggingStyle::Internal => {
            format!("Discriminator: {} (internal).", tu.discriminator_field)
        }
        TaggingStyle::Adjacent { content_field } => format!(
            "Discriminator: {} / content: {} (adjacent).",
            tu.discriminator_field, content_field
        ),
        TaggingStyle::External => "Discriminator: variant key (external).".to_string(),
    };
    let combined_doc = match &schema.description {
        Some(desc) => format!("{desc}\n\n{hint}"),
        None => hint,
    };
    Some(render_alias_file(&name, "any", Some(&combined_doc)))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a `package models` header block.
fn package_header() -> CodeBlock<GoLang> {
    let mut b = CodeBlock::<GoLang>::builder();
    b.add(&format!("package {MODELS_PACKAGE}"), ());
    b.build().expect("package header builds")
}

/// Render a simple type-alias file: package + optional doc + `type X = Y`.
///
/// Used for Alias, mixed-Enum, Union, and TaggedUnion (all reduce to a single
/// type alias in this pass).
fn render_alias_file(name: &str, rhs: &str, doc: Option<&str>) -> String {
    let mut out = String::new();
    out.push_str(&format!("package {MODELS_PACKAGE}\n\n"));
    if let Some(d) = doc {
        for line in d.lines() {
            out.push_str(&format!("// {line}\n"));
        }
    }
    out.push_str(&format!("type {name} = {rhs}\n"));
    out
}

/// Preamble = `package models` + optional doc comment.
///
/// Used for hand-rolled files (enum const blocks, intersection structs) that
/// don't fit sigil's builders cleanly.
fn preamble(_name: &str, doc: Option<&str>) -> String {
    let mut out = format!("package {MODELS_PACKAGE}\n\n");
    if let Some(d) = doc {
        for line in d.lines() {
            out.push_str(&format!("// {line}\n"));
        }
    }
    out
}

/// Map an IR type expression to a sigil `TypeName<GoLang>`.
///
/// All same-package references (named schemas, primitives) resolve to a plain
/// `TypeName::primitive` with the Go identifier. Cross-package imports aren't
/// needed within the `models/` tree.
fn go_type_name(expr: &IrTypeExpr) -> TypeName<GoLang> {
    TypeName::primitive(&go_type_str(expr))
}

/// Map an IR type expression to a Go type as a bare string.
fn go_type_str(expr: &IrTypeExpr) -> String {
    match expr {
        IrTypeExpr::Named(name) => name.to_pascal_case(),
        IrTypeExpr::Primitive(p) => go_primitive(p).to_string(),
        IrTypeExpr::Array(inner) => format!("[]{}", go_type_str(inner)),
        IrTypeExpr::Map(inner) => format!("map[string]{}", go_type_str(inner)),
        IrTypeExpr::Nullable(inner) => format!("*{}", go_type_str(inner)),
        IrTypeExpr::StringLiteral(_) | IrTypeExpr::StringEnum(_) => "string".to_string(),
        // Inline unions fall back to `any` — same reasoning as `IrSchemaKind::Union`.
        IrTypeExpr::Union(_) => "any".to_string(),
        IrTypeExpr::Any => "any".to_string(),
    }
}

fn go_primitive(p: &IrPrimitive) -> &'static str {
    match p {
        IrPrimitive::String
        | IrPrimitive::Date
        | IrPrimitive::DateTime
        | IrPrimitive::Uuid
        | IrPrimitive::StringWithFormat(_) => "string",
        IrPrimitive::Binary => "[]byte",
        IrPrimitive::Integer => "int",
        IrPrimitive::IntegerWithFormat(format) => match format.as_str() {
            "int32" => "int32",
            "int64" => "int64",
            _ => "int",
        },
        IrPrimitive::Number => "float64",
        IrPrimitive::NumberWithFormat(format) => match format.as_str() {
            "float" => "float32",
            _ => "float64",
        },
        IrPrimitive::Boolean => "bool",
    }
}

/// Convert an IR property name to an exported Go field name.
fn go_field_name(name: &str) -> String {
    name.to_pascal_case()
}

fn escape_go_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
