//! Sigil-stitch emit for IR schemas (Rust models).
//!
//! Each supported `IrSchemaKind` maps to one `models/<name>.rs` file.
//!
//! Coverage:
//! - `Object` — struct with serde derives and `#[serde(rename)]` tags.
//! - `Enum` — Rust enum with serde rename per variant.
//! - `Alias` — `pub type X = Y;`
//! - `Union` — `#[serde(untagged)]` enum.
//! - `Intersection` — struct with `#[serde(flatten)]` fields.
//! - `TaggedUnion` — serde-tagged enum (internal/adjacent/external).

use heck::{ToPascalCase, ToSnakeCase};
use openapi_nexus_core::traits::file_writer::FileInfo;
use openapi_nexus_ir::types::{
    IrEnum, IrEnumValueType, IrIntersection, IrObject, IrPrimitive, IrSchema, IrSchemaKind, IrSpec,
    IrTaggedUnion, IrTypeExpr, IrUnion, TaggingStyle,
};
use sigil_stitch::code_block::{CodeBlock, StringLitArg};
use sigil_stitch::prelude::sigil_quote;
use sigil_stitch::spec::file_spec::FileSpec;
use sigil_stitch::spec::import_spec::ImportSpec;
use sigil_stitch::type_name::TypeName;

use crate::config::{ExtraDeriveConfig, RustGeneratorConfig};

/// Generate every model file from the IR.
pub fn generate_model_files(
    ir: &IrSpec,
    header: &str,
    config: &RustGeneratorConfig,
) -> Result<Vec<FileInfo>, String> {
    let mut files = Vec::new();
    let mut mod_entries = Vec::new();

    for (_name, schema) in &ir.schemas {
        let Some(file_spec) = emit_model_file(schema, config) else {
            return Err(format!(
                "unsupported schema kind for {}: {:?}",
                schema.name, schema.kind
            ));
        };
        let stem = schema.name.to_snake_case();
        let filename = format!("{stem}.rs");
        mod_entries.push(stem);

        let rendered = file_spec
            .render(100)
            .map_err(|e| format!("render error for {}: {e}", schema.name))?;

        let mut content = String::with_capacity(header.len() + rendered.len());
        content.push_str(header);
        content.push_str(&rendered);
        files.push(FileInfo::model(filename, content));
    }

    // mod.rs that re-exports all model modules
    let mut mod_content = String::from(header);
    for entry in &mod_entries {
        mod_content.push_str(&format!("mod {entry};\npub use {entry}::*;\n"));
    }
    files.push(FileInfo::model("mod.rs".to_string(), mod_content));

    Ok(files)
}

fn emit_model_file(schema: &IrSchema, config: &RustGeneratorConfig) -> Option<FileSpec> {
    let extra = config.extra_derives.as_ref();
    match &schema.kind {
        IrSchemaKind::Object(obj) => {
            emit_object(schema, obj, extra.and_then(|e| e.structs.as_ref()))
        }
        IrSchemaKind::Enum(en) => emit_enum(schema, en, extra.and_then(|e| e.enums.as_ref())),
        IrSchemaKind::Alias(expr) => emit_alias(schema, expr),
        IrSchemaKind::Union(u) => emit_union(schema, u, extra.and_then(|e| e.unions.as_ref())),
        IrSchemaKind::Intersection(i) => {
            emit_intersection(schema, i, extra.and_then(|e| e.structs.as_ref()))
        }
        IrSchemaKind::TaggedUnion(tu) => {
            emit_tagged_union(schema, tu, extra.and_then(|e| e.unions.as_ref()))
        }
    }
}

// ---------------------------------------------------------------------------
// Derive attribute helper
// ---------------------------------------------------------------------------

fn derive_attr(base: &str, extra: Option<&ExtraDeriveConfig>) -> String {
    match extra {
        Some(cfg) if !cfg.derives.is_empty() => {
            format!("#[derive({base}, {})]", cfg.derives.join(", "))
        }
        _ => format!("#[derive({base})]"),
    }
}

// ---------------------------------------------------------------------------
// Object -> struct
// ---------------------------------------------------------------------------

fn emit_object(
    schema: &IrSchema,
    obj: &IrObject,
    extra: Option<&ExtraDeriveConfig>,
) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();
    let stem = schema.name.to_snake_case();

    let mut fsb = FileSpec::builder(&format!("{stem}.rs"));
    fsb = fsb.add_import(ImportSpec::named("serde", "Deserialize"));
    fsb = fsb.add_import(ImportSpec::named("serde", "Serialize"));

    let mut body = CodeBlock::builder();

    if let Some(doc) = &schema.description {
        emit_doc_comment(&mut body, doc);
    }
    body.add(
        &derive_attr("Debug, Clone, Serialize, Deserialize", extra),
        (),
    );
    body.add_line();
    body.add(&format!("pub struct {name}"), ());
    body.begin_control_flow("", ());

    for (json_name, prop) in &obj.properties {
        if let Some(desc) = &prop.description {
            emit_doc_comment(&mut body, desc);
        }
        let raw_field_name = json_name.to_snake_case();
        let field_name = escape_rust_keyword(&raw_field_name);
        let needs_rename = field_name != *json_name;
        let is_optional = !prop.required || prop.nullable;

        if needs_rename {
            body.add(&format!("#[serde(rename = \"{json_name}\")]"), ());
            body.add_line();
        }
        if is_optional {
            body.add(
                "#[serde(skip_serializing_if = \"Option::is_none\", default)]",
                (),
            );
            body.add_line();
        }

        let rust_type = rust_type_str_model(&prop.type_expr);
        if is_optional {
            body.add(&format!("pub {field_name}: Option<{rust_type}>,"), ());
        } else {
            body.add(&format!("pub {field_name}: {rust_type},"), ());
        }
        body.add_line();
    }

    if let Some(additional) = &obj.additional_properties {
        body.add("#[serde(flatten)]", ());
        body.add_line();
        let val_type = rust_type_str_model(additional);
        body.add(
            &format!("pub additional_properties: std::collections::HashMap<String, {val_type}>,"),
            (),
        );
        body.add_line();
    }

    body.end_control_flow();
    fsb = fsb.add_code(body.build().expect("object body builds"));
    fsb.build().ok()
}

// ---------------------------------------------------------------------------
// Enum
// ---------------------------------------------------------------------------

fn emit_enum(
    schema: &IrSchema,
    en: &IrEnum,
    extra: Option<&ExtraDeriveConfig>,
) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();

    match en.value_type {
        IrEnumValueType::Mixed | IrEnumValueType::Number => {
            return emit_type_alias_file(schema, "serde_json::Value");
        }
        IrEnumValueType::Integer => {
            return emit_integer_enum(schema, en, extra);
        }
        IrEnumValueType::String => {}
    }

    let stem = schema.name.to_snake_case();
    let mut fsb = FileSpec::builder(&format!("{stem}.rs"));
    fsb = fsb.add_import(ImportSpec::named("serde", "Deserialize"));
    fsb = fsb.add_import(ImportSpec::named("serde", "Serialize"));

    let mut body = CodeBlock::builder();

    if let Some(doc) = &schema.description {
        emit_doc_comment(&mut body, doc);
    }
    body.add(
        &derive_attr("Debug, Clone, PartialEq, Eq, Serialize, Deserialize", extra),
        (),
    );
    body.add_line();
    body.add(&format!("pub enum {name}"), ());
    body.begin_control_flow("", ());

    let mut variants: Vec<(String, String)> = Vec::new();
    for v in &en.values {
        let s = v.value.as_str()?;
        let variant = s.to_pascal_case();
        if variant != s {
            body.add(&format!("#[serde(rename = \"{}\")]", escape_str(s)), ());
            body.add_line();
        }
        body.add(&format!("{variant},"), ());
        body.add_line();
        variants.push((variant, s.to_string()));
    }

    body.end_control_flow();
    fsb = fsb.add_code(body.build().expect("enum body builds"));

    // Display impl via sigil_quote!
    if let Some(display_block) = build_string_enum_display(&name, &variants) {
        fsb = fsb.add_code(display_block);
    }

    fsb.build().ok()
}

fn emit_integer_enum(
    schema: &IrSchema,
    en: &IrEnum,
    extra: Option<&ExtraDeriveConfig>,
) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();
    let stem = schema.name.to_snake_case();

    let mut fsb = FileSpec::builder(&format!("{stem}.rs"));
    fsb = fsb.add_import(ImportSpec::named("serde_repr", "Deserialize_repr"));
    fsb = fsb.add_import(ImportSpec::named("serde_repr", "Serialize_repr"));

    let mut body = CodeBlock::builder();

    if let Some(doc) = &schema.description {
        emit_doc_comment(&mut body, doc);
    }
    body.add(
        &derive_attr(
            "Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr",
            extra,
        ),
        (),
    );
    body.add_line();
    body.add("#[repr(i64)]", ());
    body.add_line();
    body.add(&format!("pub enum {name}"), ());
    body.begin_control_flow("", ());

    for v in &en.values {
        let n = v.value.as_i64()?;
        let variant_name = if n < 0 {
            format!("Neg{}", n.unsigned_abs())
        } else {
            format!("N{n}")
        };
        body.add(&format!("{variant_name} = {n},"), ());
        body.add_line();
    }

    body.end_control_flow();
    fsb = fsb.add_code(body.build().expect("integer enum body builds"));

    // Display impl via sigil_quote!
    if let Some(display_block) = build_integer_enum_display(&name) {
        fsb = fsb.add_code(display_block);
    }

    fsb.build().ok()
}

// ---------------------------------------------------------------------------
// Alias -> pub type
// ---------------------------------------------------------------------------

fn emit_alias(schema: &IrSchema, expr: &IrTypeExpr) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();
    let stem = schema.name.to_snake_case();
    let rhs = rust_type_str_model(expr);
    let rhs_type = TypeName::raw(&rhs);

    let mut fsb = FileSpec::builder(&format!("{stem}.rs"));

    let mut cb = CodeBlock::builder();
    if let Some(doc) = &schema.description {
        cb.add("%L", doc_comment_block(doc));
    }

    let alias = sigil_quote!(RustLang {
        pub type $N(name.as_str()) = $T(rhs_type);
    })
    .ok()?;
    cb.add_code(alias);

    fsb = fsb.add_code(cb.build().ok()?);
    fsb.build().ok()
}

fn emit_type_alias_file(schema: &IrSchema, rhs_str: &str) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();
    let stem = schema.name.to_snake_case();

    let mut fsb = FileSpec::builder(&format!("{stem}.rs"));

    let mut cb = CodeBlock::builder();
    if let Some(doc) = &schema.description {
        cb.add("%L", doc_comment_block(doc));
    }

    let rhs_type = TypeName::raw(rhs_str);
    let alias = sigil_quote!(RustLang {
        pub type $N(name.as_str()) = $T(rhs_type);
    })
    .ok()?;
    cb.add_code(alias);

    fsb = fsb.add_code(cb.build().ok()?);
    fsb.build().ok()
}

// ---------------------------------------------------------------------------
// Union -> #[serde(untagged)] enum
// ---------------------------------------------------------------------------

fn emit_union(
    schema: &IrSchema,
    union: &IrUnion,
    extra: Option<&ExtraDeriveConfig>,
) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();
    let stem = schema.name.to_snake_case();

    let mut fsb = FileSpec::builder(&format!("{stem}.rs"));
    fsb = fsb.add_import(ImportSpec::named("serde", "Deserialize"));
    fsb = fsb.add_import(ImportSpec::named("serde", "Serialize"));

    let mut body = CodeBlock::builder();

    if let Some(doc) = &schema.description {
        emit_doc_comment(&mut body, doc);
    }
    body.add(
        &derive_attr("Debug, Clone, Serialize, Deserialize", extra),
        (),
    );
    body.add_line();
    body.add("#[serde(untagged)]", ());
    body.add_line();
    body.add(&format!("pub enum {name}"), ());
    body.begin_control_flow("", ());

    for (i, member) in union.members.iter().enumerate() {
        let variant_name = union_variant_name(member, i);
        let rust_type = rust_type_str_model(member);
        body.add(&format!("{variant_name}({rust_type}),"), ());
        body.add_line();
    }

    body.end_control_flow();
    fsb = fsb.add_code(body.build().expect("union body builds"));
    fsb.build().ok()
}

fn union_variant_name(expr: &IrTypeExpr, index: usize) -> String {
    match expr {
        IrTypeExpr::Named(n) => n.to_pascal_case(),
        IrTypeExpr::Primitive(p) => primitive_variant_name(p),
        IrTypeExpr::Array(_) => format!("Array{index}"),
        IrTypeExpr::Map(_) => format!("Map{index}"),
        _ => format!("Variant{index}"),
    }
}

fn primitive_variant_name(p: &IrPrimitive) -> String {
    match p {
        IrPrimitive::String | IrPrimitive::StringWithFormat(_) => "String".to_string(),
        IrPrimitive::Integer | IrPrimitive::IntegerWithFormat(_) => "Integer".to_string(),
        IrPrimitive::Number | IrPrimitive::NumberWithFormat(_) => "Number".to_string(),
        IrPrimitive::Boolean => "Boolean".to_string(),
        IrPrimitive::Binary => "Binary".to_string(),
        IrPrimitive::Date => "Date".to_string(),
        IrPrimitive::DateTime => "DateTime".to_string(),
        IrPrimitive::Uuid => "Uuid".to_string(),
    }
}

// ---------------------------------------------------------------------------
// TaggedUnion -> serde-tagged enum
// ---------------------------------------------------------------------------

fn emit_tagged_union(
    schema: &IrSchema,
    tu: &IrTaggedUnion,
    extra: Option<&ExtraDeriveConfig>,
) -> Option<FileSpec> {
    if tu.variants.is_empty() {
        return None;
    }

    let name = schema.name.to_pascal_case();
    let stem = schema.name.to_snake_case();

    let mut fsb = FileSpec::builder(&format!("{stem}.rs"));
    fsb = fsb.add_import(ImportSpec::named("serde", "Deserialize"));
    fsb = fsb.add_import(ImportSpec::named("serde", "Serialize"));

    let mut body = CodeBlock::builder();

    if let Some(doc) = &schema.description {
        emit_doc_comment(&mut body, doc);
    }
    body.add(
        &derive_attr("Debug, Clone, Serialize, Deserialize", extra),
        (),
    );
    body.add_line();

    match &tu.tagging {
        TaggingStyle::Internal => {
            body.add(
                &format!(
                    "#[serde(tag = \"{}\")]",
                    escape_str(&tu.discriminator_field)
                ),
                (),
            );
            body.add_line();
        }
        TaggingStyle::Adjacent { content_field } => {
            body.add(
                &format!(
                    "#[serde(tag = \"{}\", content = \"{}\")]",
                    escape_str(&tu.discriminator_field),
                    escape_str(content_field)
                ),
                (),
            );
            body.add_line();
        }
        TaggingStyle::External => {}
    }

    body.add(&format!("pub enum {name}"), ());
    body.begin_control_flow("", ());

    for variant in &tu.variants {
        let variant_name = variant.discriminator_value.to_pascal_case();
        if variant_name != variant.discriminator_value {
            body.add(
                &format!(
                    "#[serde(rename = \"{}\")]",
                    escape_str(&variant.discriminator_value)
                ),
                (),
            );
            body.add_line();
        }
        let rust_type = rust_type_str_model(&variant.content_type);
        body.add(&format!("{variant_name}({rust_type}),"), ());
        body.add_line();
    }

    body.end_control_flow();
    fsb = fsb.add_code(body.build().expect("tagged union body builds"));
    fsb.build().ok()
}

// ---------------------------------------------------------------------------
// Intersection -> flattened struct
// ---------------------------------------------------------------------------

fn emit_intersection(
    schema: &IrSchema,
    inter: &IrIntersection,
    extra: Option<&ExtraDeriveConfig>,
) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();
    let stem = schema.name.to_snake_case();

    let mut fsb = FileSpec::builder(&format!("{stem}.rs"));
    fsb = fsb.add_import(ImportSpec::named("serde", "Deserialize"));
    fsb = fsb.add_import(ImportSpec::named("serde", "Serialize"));

    let mut body = CodeBlock::builder();

    if let Some(doc) = &schema.description {
        emit_doc_comment(&mut body, doc);
    }
    body.add(
        &derive_attr("Debug, Clone, Serialize, Deserialize", extra),
        (),
    );
    body.add_line();
    body.add(&format!("pub struct {name}"), ());
    body.begin_control_flow("", ());

    for (i, member) in inter.members.iter().enumerate() {
        let raw_name = match member {
            IrTypeExpr::Named(n) => n.to_snake_case(),
            _ => format!("member_{i}"),
        };
        let field_name = escape_rust_keyword(&raw_name);
        let rust_type = rust_type_str_model(member);
        body.add("#[serde(flatten)]", ());
        body.add_line();
        body.add(&format!("pub {field_name}: {rust_type},"), ());
        body.add_line();
    }

    body.end_control_flow();
    fsb = fsb.add_code(body.build().expect("intersection body builds"));
    fsb.build().ok()
}

// ---------------------------------------------------------------------------
// Display impl helpers
// ---------------------------------------------------------------------------

fn build_integer_enum_display(name: &str) -> Option<CodeBlock> {
    sigil_quote!(RustLang {
        impl std::fmt::Display for $N(name) {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", *self as i64)
            }
        }
    })
    .ok()
}

fn build_string_enum_display(name: &str, variants: &[(String, String)]) -> Option<CodeBlock> {
    let mut match_arms = CodeBlock::builder();
    for (variant, wire_value) in variants {
        match_arms.add(
            &format!("{name}::{variant} => write!(f, %S),\n"),
            (StringLitArg(wire_value.clone()),),
        );
    }
    let match_body = match_arms.build().ok()?;

    sigil_quote!(RustLang {
        impl std::fmt::Display for $N(name) {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self { $C(match_body) }
            }
        }
    })
    .ok()
}

// ---------------------------------------------------------------------------
// Doc comment helpers
// ---------------------------------------------------------------------------

/// Emit doc comment lines into a CodeBlockBuilder.
fn emit_doc_comment(cb: &mut sigil_stitch::code_block::CodeBlockBuilder, doc: &str) {
    for line in doc.lines() {
        cb.add(&format!("/// {line}"), ());
        cb.add_line();
    }
}

/// Build a doc-comment string for use with %L interpolation.
fn doc_comment_block(doc: &str) -> String {
    let mut out = String::new();
    for line in doc.lines() {
        out.push_str(&format!("/// {line}\n"));
    }
    out
}

// ---------------------------------------------------------------------------
// Type mapping helpers
// ---------------------------------------------------------------------------

pub fn rust_type_str(expr: &IrTypeExpr) -> String {
    match expr {
        IrTypeExpr::Named(name) => name.to_pascal_case(),
        IrTypeExpr::Primitive(p) => rust_primitive(p).to_string(),
        IrTypeExpr::Array(inner) => format!("Vec<{}>", rust_type_str(inner)),
        IrTypeExpr::Map(inner) => format!(
            "std::collections::HashMap<String, {}>",
            rust_type_str(inner)
        ),
        IrTypeExpr::Nullable(inner) => format!("Option<{}>", rust_type_str(inner)),
        IrTypeExpr::StringLiteral(_) | IrTypeExpr::StringEnum(_) => "String".to_string(),
        IrTypeExpr::Union(_) | IrTypeExpr::Any => "serde_json::Value".to_string(),
    }
}

/// Map an IR type to a Rust type string, qualified for use from API modules.
pub fn rust_type_str_qualified(expr: &IrTypeExpr) -> String {
    match expr {
        IrTypeExpr::Named(name) => format!("crate::models::{}", name.to_pascal_case()),
        IrTypeExpr::Array(inner) => format!("Vec<{}>", rust_type_str_qualified(inner)),
        IrTypeExpr::Map(inner) => format!(
            "std::collections::HashMap<String, {}>",
            rust_type_str_qualified(inner)
        ),
        IrTypeExpr::Nullable(inner) => format!("Option<{}>", rust_type_str_qualified(inner)),
        other => rust_type_str(other),
    }
}

/// Map an IR type for use within model files (sibling references use `super::`).
fn rust_type_str_model(expr: &IrTypeExpr) -> String {
    match expr {
        IrTypeExpr::Named(name) => format!("super::{}", name.to_pascal_case()),
        IrTypeExpr::Array(inner) => format!("Vec<{}>", rust_type_str_model(inner)),
        IrTypeExpr::Map(inner) => format!(
            "std::collections::HashMap<String, {}>",
            rust_type_str_model(inner)
        ),
        IrTypeExpr::Nullable(inner) => format!("Option<{}>", rust_type_str_model(inner)),
        other => rust_type_str(other),
    }
}

fn rust_primitive(p: &IrPrimitive) -> &'static str {
    match p {
        IrPrimitive::String
        | IrPrimitive::Date
        | IrPrimitive::DateTime
        | IrPrimitive::Uuid
        | IrPrimitive::StringWithFormat(_) => "String",
        IrPrimitive::Binary => "Vec<u8>",
        IrPrimitive::Integer => "i64",
        IrPrimitive::IntegerWithFormat(format) => match format.as_str() {
            "int32" => "i32",
            "int64" => "i64",
            _ => "i64",
        },
        IrPrimitive::Number => "f64",
        IrPrimitive::NumberWithFormat(format) => match format.as_str() {
            "float" => "f32",
            _ => "f64",
        },
        IrPrimitive::Boolean => "bool",
    }
}

fn escape_str(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn escape_rust_keyword(name: &str) -> String {
    const KEYWORDS: &[&str] = &[
        "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
        "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move",
        "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait",
        "true", "type", "union", "unsafe", "use", "where", "while", "yield",
    ];
    if KEYWORDS.contains(&name) {
        format!("r#{name}")
    } else {
        name.to_string()
    }
}
