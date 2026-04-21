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

/// Generate every model file from the IR.
pub fn generate_model_files(ir: &IrSpec, header: &str) -> Result<Vec<FileInfo>, String> {
    let mut files = Vec::new();
    let mut mod_entries = Vec::new();

    for (name, schema) in &ir.schemas {
        let Some(body) = emit_model_body(schema) else {
            return Err(format!(
                "unsupported schema kind for {name}: {:?}",
                schema.kind
            ));
        };
        let stem = schema.name.to_snake_case();
        let filename = format!("{stem}.rs");
        mod_entries.push(stem);

        let mut content = String::with_capacity(header.len() + body.len());
        content.push_str(header);
        content.push_str(&body);
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
// Object → struct
// ---------------------------------------------------------------------------

fn emit_object(schema: &IrSchema, obj: &IrObject) -> Option<String> {
    let name = schema.name.to_pascal_case();
    let mut out = String::new();

    out.push_str(&imports_for_object(obj));

    if let Some(doc) = &schema.description {
        for line in doc.lines() {
            out.push_str(&format!("/// {line}\n"));
        }
    }
    out.push_str("#[derive(Debug, Clone, Serialize, Deserialize)]\n");
    out.push_str(&format!("pub struct {name} {{\n"));

    for (json_name, prop) in &obj.properties {
        if let Some(desc) = &prop.description {
            for line in desc.lines() {
                out.push_str(&format!("    /// {line}\n"));
            }
        }
        let field_name = json_name.to_snake_case();
        let needs_rename = field_name != *json_name;
        let is_optional = !prop.required || prop.nullable;

        if needs_rename {
            out.push_str(&format!("    #[serde(rename = \"{json_name}\")]\n"));
        }
        if is_optional {
            out.push_str("    #[serde(skip_serializing_if = \"Option::is_none\", default)]\n");
        }

        let rust_type = rust_type_str(&prop.type_expr);
        if is_optional {
            out.push_str(&format!("    pub {field_name}: Option<{rust_type}>,\n"));
        } else {
            out.push_str(&format!("    pub {field_name}: {rust_type},\n"));
        }
    }

    if let Some(additional) = &obj.additional_properties {
        out.push_str("    #[serde(flatten)]\n");
        let val_type = rust_type_str(additional);
        out.push_str(&format!(
            "    pub additional_properties: std::collections::HashMap<String, {val_type}>,\n"
        ));
    }

    out.push_str("}\n");
    Some(out)
}

fn imports_for_object(obj: &IrObject) -> String {
    let mut needs_hashmap = obj.additional_properties.is_some();
    for (_, prop) in &obj.properties {
        if type_needs_hashmap(&prop.type_expr) {
            needs_hashmap = true;
        }
    }

    let mut out = String::new();
    out.push_str("use serde::{Deserialize, Serialize};\n");
    if needs_hashmap {
        out.push_str("use std::collections::HashMap;\n");
    }
    out.push('\n');
    out
}

// ---------------------------------------------------------------------------
// Enum
// ---------------------------------------------------------------------------

fn emit_enum(schema: &IrSchema, en: &IrEnum) -> Option<String> {
    let name = schema.name.to_pascal_case();

    match en.value_type {
        IrEnumValueType::Mixed => {
            return Some(emit_type_alias(
                &name,
                "serde_json::Value",
                schema.description.as_deref(),
            ));
        }
        IrEnumValueType::Number => {
            return Some(emit_type_alias(
                &name,
                "serde_json::Value",
                schema.description.as_deref(),
            ));
        }
        IrEnumValueType::Integer => {
            return emit_integer_enum(schema, en);
        }
        IrEnumValueType::String => {}
    }

    let mut out = String::new();
    out.push_str("use serde::{Deserialize, Serialize};\n\n");

    if let Some(doc) = &schema.description {
        for line in doc.lines() {
            out.push_str(&format!("/// {line}\n"));
        }
    }
    out.push_str("#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\n");
    out.push_str(&format!("pub enum {name} {{\n"));

    for v in &en.values {
        let s = v.value.as_str()?;
        let variant = s.to_pascal_case();
        if variant != s {
            out.push_str(&format!("    #[serde(rename = \"{}\")]\n", escape_str(s)));
        }
        out.push_str(&format!("    {variant},\n"));
    }

    out.push_str("}\n");
    Some(out)
}

fn emit_integer_enum(schema: &IrSchema, en: &IrEnum) -> Option<String> {
    let name = schema.name.to_pascal_case();
    let mut out = String::new();
    out.push_str("use serde_repr::{Deserialize_repr, Serialize_repr};\n\n");

    if let Some(doc) = &schema.description {
        for line in doc.lines() {
            out.push_str(&format!("/// {line}\n"));
        }
    }
    out.push_str(
        "#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]\n",
    );
    out.push_str("#[repr(i64)]\n");
    out.push_str(&format!("pub enum {name} {{\n"));

    for v in &en.values {
        let n = v.value.as_i64()?;
        let variant_name = if n < 0 {
            format!("Neg{}", n.unsigned_abs())
        } else {
            format!("N{n}")
        };
        out.push_str(&format!("    {variant_name} = {n},\n"));
    }

    out.push_str("}\n");
    Some(out)
}

// ---------------------------------------------------------------------------
// Alias → pub type
// ---------------------------------------------------------------------------

fn emit_alias(schema: &IrSchema, expr: &IrTypeExpr) -> Option<String> {
    let name = schema.name.to_pascal_case();
    let rhs = rust_type_str(expr);
    Some(emit_type_alias(&name, &rhs, schema.description.as_deref()))
}

fn emit_type_alias(name: &str, rhs: &str, doc: Option<&str>) -> String {
    let mut out = String::new();
    if rhs.contains("HashMap") {
        out.push_str("use std::collections::HashMap;\n\n");
    }
    if let Some(d) = doc {
        for line in d.lines() {
            out.push_str(&format!("/// {line}\n"));
        }
    }
    out.push_str(&format!("pub type {name} = {rhs};\n"));
    out
}

// ---------------------------------------------------------------------------
// Union → #[serde(untagged)] enum
// ---------------------------------------------------------------------------

fn emit_union(schema: &IrSchema, union: &IrUnion) -> Option<String> {
    let name = schema.name.to_pascal_case();
    let mut out = String::new();
    out.push_str("use serde::{Deserialize, Serialize};\n\n");

    if let Some(doc) = &schema.description {
        for line in doc.lines() {
            out.push_str(&format!("/// {line}\n"));
        }
    }
    out.push_str("#[derive(Debug, Clone, Serialize, Deserialize)]\n");
    out.push_str("#[serde(untagged)]\n");
    out.push_str(&format!("pub enum {name} {{\n"));

    for (i, member) in union.members.iter().enumerate() {
        let variant_name = union_variant_name(member, i);
        let rust_type = rust_type_str(member);
        out.push_str(&format!("    {variant_name}({rust_type}),\n"));
    }

    out.push_str("}\n");
    Some(out)
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
// TaggedUnion → serde-tagged enum
// ---------------------------------------------------------------------------

fn emit_tagged_union(schema: &IrSchema, tu: &IrTaggedUnion) -> Option<String> {
    if tu.variants.is_empty() {
        return None;
    }

    let name = schema.name.to_pascal_case();
    let mut out = String::new();
    out.push_str("use serde::{Deserialize, Serialize};\n\n");

    if let Some(doc) = &schema.description {
        for line in doc.lines() {
            out.push_str(&format!("/// {line}\n"));
        }
    }
    out.push_str("#[derive(Debug, Clone, Serialize, Deserialize)]\n");

    match &tu.tagging {
        TaggingStyle::Internal => {
            out.push_str(&format!(
                "#[serde(tag = \"{}\")]\n",
                escape_str(&tu.discriminator_field)
            ));
        }
        TaggingStyle::Adjacent { content_field } => {
            out.push_str(&format!(
                "#[serde(tag = \"{}\", content = \"{}\")]\n",
                escape_str(&tu.discriminator_field),
                escape_str(content_field)
            ));
        }
        TaggingStyle::External => {
            // Default serde representation — no attribute needed.
        }
    }

    out.push_str(&format!("pub enum {name} {{\n"));

    for variant in &tu.variants {
        let variant_name = variant.discriminator_value.to_pascal_case();
        if variant_name != variant.discriminator_value {
            out.push_str(&format!(
                "    #[serde(rename = \"{}\")]\n",
                escape_str(&variant.discriminator_value)
            ));
        }
        let rust_type = rust_type_str(&variant.content_type);
        out.push_str(&format!("    {variant_name}({rust_type}),\n"));
    }

    out.push_str("}\n");
    Some(out)
}

// ---------------------------------------------------------------------------
// Intersection → flattened struct
// ---------------------------------------------------------------------------

fn emit_intersection(schema: &IrSchema, inter: &IrIntersection) -> Option<String> {
    let name = schema.name.to_pascal_case();
    let mut out = String::new();
    out.push_str("use serde::{Deserialize, Serialize};\n\n");

    if let Some(doc) = &schema.description {
        for line in doc.lines() {
            out.push_str(&format!("/// {line}\n"));
        }
    }
    out.push_str("#[derive(Debug, Clone, Serialize, Deserialize)]\n");
    out.push_str(&format!("pub struct {name} {{\n"));

    for (i, member) in inter.members.iter().enumerate() {
        let field_name = match member {
            IrTypeExpr::Named(n) => n.to_snake_case(),
            _ => format!("member_{i}"),
        };
        let rust_type = rust_type_str(member);
        out.push_str("    #[serde(flatten)]\n");
        out.push_str(&format!("    pub {field_name}: {rust_type},\n"));
    }

    out.push_str("}\n");
    Some(out)
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

fn type_needs_hashmap(expr: &IrTypeExpr) -> bool {
    match expr {
        IrTypeExpr::Map(_) => true,
        IrTypeExpr::Array(inner) => type_needs_hashmap(inner),
        IrTypeExpr::Nullable(inner) => type_needs_hashmap(inner),
        _ => false,
    }
}

fn escape_str(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
