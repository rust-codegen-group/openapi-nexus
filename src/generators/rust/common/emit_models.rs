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

use std::collections::HashSet;

use crate::codegen::traits::file_writer::FileInfo;
use crate::ir::types::{
    IrEnum, IrEnumValueType, IrIntersection, IrObject, IrPrimitive, IrSchema, IrSchemaKind, IrSpec,
    IrTaggedUnion, IrTypeExpr, IrUnion, TaggingStyle,
};
use heck::{ToPascalCase, ToSnakeCase};
use sigil_stitch::prelude::{CodeBlock, sigil_quote};
use sigil_stitch::spec::file_spec::FileSpec;
use sigil_stitch::spec::import_spec::ImportSpec;
use sigil_stitch::type_name::TypeName;

use super::config::{ExtraDeriveConfig, RustGeneratorConfig};

/// Generate every model file from the IR.
pub fn generate_model_files(
    ir: &IrSpec,
    header: &str,
    config: &RustGeneratorConfig,
) -> Result<Vec<FileInfo>, String> {
    let mut files = Vec::new();
    let mut mod_entries = Vec::new();

    // Schemas inlined into Internal/Adjacent tagged unions can be skipped as standalone files,
    // BUT only if no other schema references them by name (e.g. External/Untagged tuple variants).
    let inlined_candidates: HashSet<&str> = ir
        .schemas
        .values()
        .filter_map(|s| match &s.kind {
            IrSchemaKind::TaggedUnion(tu)
                if matches!(
                    tu.tagging,
                    TaggingStyle::Internal | TaggingStyle::Adjacent { .. }
                ) =>
            {
                Some(tu.variants.iter().filter_map(|v| {
                    if let IrTypeExpr::Named(name) = &v.content_type
                        && ir
                            .schemas
                            .get(name)
                            .is_some_and(|s| matches!(s.kind, IrSchemaKind::Object(_)))
                    {
                        return Some(name.as_str());
                    }
                    None
                }))
            }
            _ => None,
        })
        .flatten()
        .collect();

    let referenced_by_name: HashSet<&str> = ir
        .schemas
        .values()
        .flat_map(|s| collect_named_type_refs(s))
        .collect();

    let inlined_schemas: HashSet<&str> = inlined_candidates
        .difference(&referenced_by_name)
        .copied()
        .collect();

    for (_name, schema) in &ir.schemas {
        if inlined_schemas.contains(schema.name.as_str()) {
            continue;
        }
        let Some(file_spec) = emit_model_file(schema, config, ir) else {
            continue;
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

fn emit_model_file(
    schema: &IrSchema,
    config: &RustGeneratorConfig,
    ir: &IrSpec,
) -> Option<FileSpec> {
    let extra = config.extra_derives.as_ref();
    let per_type_cfg = extra
        .and_then(|e| e.per_type.as_ref())
        .and_then(|m| m.get(&schema.name));
    match &schema.kind {
        IrSchemaKind::Object(obj) => {
            let derives = per_type_cfg.or_else(|| extra.and_then(|e| e.structs.as_ref()));
            emit_object(schema, obj, derives)
        }
        IrSchemaKind::Enum(en) => {
            let derives = per_type_cfg.or_else(|| extra.and_then(|e| e.enums.as_ref()));
            emit_enum(schema, en, derives)
        }
        IrSchemaKind::Alias(expr) => {
            let derives = per_type_cfg.or_else(|| extra.and_then(|e| e.structs.as_ref()));
            emit_alias(schema, expr, derives)
        }
        IrSchemaKind::Union(u) => emit_union(schema, u, per_type_cfg),
        IrSchemaKind::Intersection(i) => {
            let derives = per_type_cfg.or_else(|| extra.and_then(|e| e.structs.as_ref()));
            emit_intersection(schema, i, derives)
        }
        IrSchemaKind::TaggedUnion(tu) => {
            let derives = per_type_cfg.or_else(|| extra.and_then(|e| e.unions.as_ref()));
            emit_tagged_union(schema, tu, derives, ir)
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

    let body = sigil_quote!(RustLang {
        $if(schema.description.is_some()) {
            $L(doc_comment_block(schema.description.as_deref().unwrap()).trim_end())
        }
        $L(derive_attr("Debug, Clone, Serialize, Deserialize", extra))
        pub struct $N(name.as_str()) {
            $for((json_name, prop) in obj.properties.iter()) {
                $if(prop.description.is_some()) {
                    $L(doc_comment_block(prop.description.as_deref().unwrap()).trim_end())
                }
                $if(escape_rust_keyword(&json_name.to_snake_case()) != *json_name) {
                    $L(format!("#[serde(rename = \"{json_name}\")]"))
                }
                $if(!prop.required || prop.nullable) {
                    #[serde(skip_serializing_if = "Option::is_none", default)]
                    $L(format!("pub {}: Option<{}>,", escape_rust_keyword(&json_name.to_snake_case()), rust_type_str_model(&prop.type_expr)))
                } $else {
                    $L(format!("pub {}: {},", escape_rust_keyword(&json_name.to_snake_case()), rust_type_str_model(&prop.type_expr)))
                }
            }
            $if(obj.additional_properties.is_some()) {
                #[serde(flatten)]
                $L(format!("pub additional_properties: std::collections::HashMap<String, {}>,", rust_type_str_model(obj.additional_properties.as_ref().unwrap())))
            }
        }
    })
    .ok()?;

    fsb = fsb.add_code(body);
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

    let mut variants: Vec<(String, String)> = Vec::new();
    for v in &en.values {
        let s = v.value.as_str()?;
        variants.push((s.to_pascal_case(), s.to_string()));
    }

    let body = sigil_quote!(RustLang {
        $if(schema.description.is_some()) {
            $L(doc_comment_block(schema.description.as_deref().unwrap()).trim_end())
        }
        $L(derive_attr("Debug, Clone, PartialEq, Eq, Serialize, Deserialize", extra))
        pub enum $N(name.as_str()) {
            $for((variant, wire) in variants.iter()) {
                $if(variant != wire) {
                    $L(format!("#[serde(rename = \"{}\")]", escape_str(wire)))
                }
                $L(format!("{variant},"))
            }
        }
    })
    .ok()?;

    fsb = fsb.add_code(body);

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

    let int_variants: Vec<(String, i64)> = en
        .values
        .iter()
        .map(|v| {
            let n = v.value.as_i64()?;
            let variant_name = if n < 0 {
                format!("Neg{}", n.unsigned_abs())
            } else {
                format!("N{n}")
            };
            Some((variant_name, n))
        })
        .collect::<Option<Vec<_>>>()?;

    let body = sigil_quote!(RustLang {
        $if(schema.description.is_some()) {
            $L(doc_comment_block(schema.description.as_deref().unwrap()).trim_end())
        }
        $L(derive_attr("Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr", extra))
        #[repr(i64)]
        pub enum $N(name.as_str()) {
            $for((variant_name, n) in int_variants.iter()) {
                $L(format!("{variant_name} = {n},"))
            }
        }
    })
    .ok()?;

    fsb = fsb.add_code(body);

    // Display impl via sigil_quote!
    if let Some(display_block) = build_integer_enum_display(&name) {
        fsb = fsb.add_code(display_block);
    }

    fsb.build().ok()
}

// ---------------------------------------------------------------------------
// Alias -> pub type
// ---------------------------------------------------------------------------

fn emit_alias(
    schema: &IrSchema,
    expr: &IrTypeExpr,
    extra: Option<&ExtraDeriveConfig>,
) -> Option<FileSpec> {
    if let IrTypeExpr::Named(n) = expr
        && n.to_pascal_case() == schema.name.to_pascal_case()
    {
        return None;
    }
    // Skip trivial primitive aliases that shadow Rust builtins (e.g. schema "string" → pub type String = String)
    if schema.name.to_pascal_case() == rust_type_str_model(expr) {
        return None;
    }

    let name = schema.name.to_pascal_case();
    let stem = schema.name.to_snake_case();
    let rhs = rust_type_str_model(expr);

    let mut fsb = FileSpec::builder(&format!("{stem}.rs"));

    let has_extra = extra.is_some_and(|cfg| !cfg.derives.is_empty());

    if has_extra {
        fsb = fsb.add_import(ImportSpec::named("serde", "Deserialize"));
        fsb = fsb.add_import(ImportSpec::named("serde", "Serialize"));

        let rhs_type = TypeName::raw(&rhs);
        let block = sigil_quote!(RustLang {
            $if(schema.description.is_some()) {
                $L(doc_comment_block(schema.description.as_deref().unwrap()).trim_end())
            }
            $L(derive_attr("Debug, Clone, Serialize, Deserialize", extra))
            #[serde(transparent)]
            pub struct $N(name.as_str())(pub $T(rhs_type));
        })
        .ok()?;
        fsb = fsb.add_code(block);
    } else {
        let rhs_type = TypeName::raw(&rhs);
        let block = sigil_quote!(RustLang {
            $if(schema.description.is_some()) {
                $L(doc_comment_block(schema.description.as_deref().unwrap()).trim_end())
            }
            pub type $N(name.as_str()) = $T(rhs_type);
        })
        .ok()?;
        fsb = fsb.add_code(block);
    }

    fsb.build().ok()
}

fn emit_type_alias_file(schema: &IrSchema, rhs_str: &str) -> Option<FileSpec> {
    let name = schema.name.to_pascal_case();
    let stem = schema.name.to_snake_case();

    let mut fsb = FileSpec::builder(&format!("{stem}.rs"));

    let rhs_type = TypeName::raw(rhs_str);
    let block = sigil_quote!(RustLang {
        $if(schema.description.is_some()) {
            $L(doc_comment_block(schema.description.as_deref().unwrap()).trim_end())
        }
        pub type $N(name.as_str()) = $T(rhs_type);
    })
    .ok()?;
    fsb = fsb.add_code(block);

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

    let variants: Vec<(String, String)> = union
        .members
        .iter()
        .enumerate()
        .map(|(i, member)| {
            let variant_name = union_variant_name(member, i);
            let rust_type = rust_type_str_model(member);
            (variant_name, rust_type)
        })
        .collect();

    let body = sigil_quote!(RustLang {
        $if(schema.description.is_some()) {
            $L(doc_comment_block(schema.description.as_deref().unwrap()).trim_end())
        }
        $L(derive_attr("Debug, Clone, Serialize, Deserialize", extra))
        #[serde(untagged)]
        pub enum $N(name.as_str()) {
            $for((variant_name, rust_type) in variants.iter()) {
                $L(format!("{variant_name}({rust_type}),"))
            }
        }
    })
    .ok()?;

    fsb = fsb.add_code(body);
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
    ir: &IrSpec,
) -> Option<FileSpec> {
    if tu.variants.is_empty() {
        return None;
    }

    let name = schema.name.to_pascal_case();
    let stem = schema.name.to_snake_case();

    let mut fsb = FileSpec::builder(&format!("{stem}.rs"));
    fsb = fsb.add_import(ImportSpec::named("serde", "Deserialize"));
    fsb = fsb.add_import(ImportSpec::named("serde", "Serialize"));

    let serde_tag_attr = match &tu.tagging {
        TaggingStyle::Internal => {
            format!(
                "#[serde(tag = \"{}\")]",
                escape_str(&tu.discriminator_field)
            )
        }
        TaggingStyle::Adjacent { content_field } => {
            format!(
                "#[serde(tag = \"{}\", content = \"{}\")]",
                escape_str(&tu.discriminator_field),
                escape_str(content_field)
            )
        }
        TaggingStyle::External => String::new(),
    };

    let inline_fields = matches!(
        tu.tagging,
        TaggingStyle::Internal | TaggingStyle::Adjacent { .. }
    );

    let mut variant_blocks: Vec<String> = Vec::new();
    for variant in &tu.variants {
        let variant_name = variant.discriminator_value.to_pascal_case();
        let mut block = String::new();

        if variant.discriminator_value.to_pascal_case() != variant.discriminator_value {
            block.push_str(&format!(
                "#[serde(rename = \"{}\")]\n",
                escape_str(&variant.discriminator_value)
            ));
        }

        if inline_fields {
            if let Some(obj) = resolve_object(&variant.content_type, ir) {
                block.push_str(&format!("{variant_name} {{"));
                for (json_name, prop) in &obj.properties {
                    let field_name = escape_rust_keyword(&json_name.to_snake_case());
                    if field_name != *json_name {
                        block.push_str(&format!("\n    #[serde(rename = \"{json_name}\")]"));
                    }
                    if !prop.required || prop.nullable {
                        block.push_str(
                            "\n    #[serde(skip_serializing_if = \"Option::is_none\", default)]",
                        );
                        block.push_str(&format!(
                            "\n    {field_name}: Option<{}>,",
                            rust_type_str_model(&prop.type_expr)
                        ));
                    } else {
                        block.push_str(&format!(
                            "\n    {field_name}: {},",
                            rust_type_str_model(&prop.type_expr)
                        ));
                    }
                }
                if let Some(ap) = &obj.additional_properties {
                    block.push_str("\n    #[serde(flatten)]");
                    block.push_str(&format!(
                        "\n    additional_properties: std::collections::HashMap<String, {}>,",
                        rust_type_str_model(ap)
                    ));
                }
                block.push_str("\n},");
            } else {
                block.push_str(&format!(
                    "{}({}),",
                    variant_name,
                    rust_type_str_model(&variant.content_type)
                ));
            }
        } else {
            block.push_str(&format!(
                "{}({}),",
                variant_name,
                rust_type_str_model(&variant.content_type)
            ));
        }

        variant_blocks.push(block);
    }

    let body = sigil_quote!(RustLang {
        $if(schema.description.is_some()) {
            $L(doc_comment_block(schema.description.as_deref().unwrap()).trim_end())
        }
        $L(derive_attr("Debug, Clone, Serialize, Deserialize", extra))
        $if(!serde_tag_attr.is_empty()) {
            $L(serde_tag_attr.as_str())
        }
        pub enum $N(name.as_str()) {
            $for(block in variant_blocks.iter()) {
                $L(block.as_str())
            }
        }
    })
    .ok()?;

    fsb = fsb.add_code(body);
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

    let fields: Vec<(String, String)> = inter
        .members
        .iter()
        .enumerate()
        .map(|(i, member)| {
            let raw_name = match member {
                IrTypeExpr::Named(n) => n.to_snake_case(),
                _ => format!("member_{i}"),
            };
            let field_name = escape_rust_keyword(&raw_name);
            let rust_type = rust_type_str_model(member);
            (field_name, rust_type)
        })
        .collect();

    let body = sigil_quote!(RustLang {
        $if(schema.description.is_some()) {
            $L(doc_comment_block(schema.description.as_deref().unwrap()).trim_end())
        }
        $L(derive_attr("Debug, Clone, Serialize, Deserialize", extra))
        pub struct $N(name.as_str()) {
            $for((field_name, rust_type) in fields.iter()) {
                #[serde(flatten)]
                $L(format!("pub {field_name}: {rust_type},"))
            }
        }
    })
    .ok()?;

    fsb = fsb.add_code(body);
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
    sigil_quote!(RustLang {
        impl std::fmt::Display for $N(name) {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    $for((variant, wire_value) in variants.iter()) {
                        $L(format!("{name}::{variant} => write!(f, {wire_value:?}),"))
                    }
                }
            }
        }
    })
    .ok()
}

// ---------------------------------------------------------------------------
// Doc comment helpers
// ---------------------------------------------------------------------------

/// Build a doc-comment string for use with $L interpolation.
fn doc_comment_block(doc: &str) -> String {
    let mut out = String::new();
    for line in doc.lines() {
        if line.is_empty() {
            out.push_str("///\n");
        } else {
            out.push_str(&format!("/// {line}\n"));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Schema resolution helper
// ---------------------------------------------------------------------------

fn resolve_object<'a>(expr: &IrTypeExpr, ir: &'a IrSpec) -> Option<&'a IrObject> {
    if let IrTypeExpr::Named(name) = expr
        && let Some(schema) = ir.schemas.get(name)
        && let IrSchemaKind::Object(obj) = &schema.kind
    {
        return Some(obj);
    }
    None
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
/// Named references to suppressed primitive aliases are resolved to the primitive type.
pub fn rust_type_str_qualified(expr: &IrTypeExpr, ir: &IrSpec) -> String {
    match expr {
        IrTypeExpr::Named(name) => {
            if let Some(schema) = ir.schemas.get(name)
                && let IrSchemaKind::Alias(inner) = &schema.kind
                && name.to_pascal_case() == rust_type_str_model(inner)
            {
                return rust_type_str(inner);
            }
            format!("crate::models::{}", name.to_pascal_case())
        }
        IrTypeExpr::Array(inner) => format!("Vec<{}>", rust_type_str_qualified(inner, ir)),
        IrTypeExpr::Map(inner) => format!(
            "std::collections::HashMap<String, {}>",
            rust_type_str_qualified(inner, ir)
        ),
        IrTypeExpr::Nullable(inner) => format!("Option<{}>", rust_type_str_qualified(inner, ir)),
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
            "uint32" => "u32",
            "uint64" => "u64",
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

/// Collect schema names referenced by `IrTypeExpr::Named` in positions that require
/// a standalone struct to exist (object fields, tuple variants, union members, etc.).
fn collect_named_type_refs(schema: &IrSchema) -> Vec<&str> {
    let mut refs = Vec::new();
    match &schema.kind {
        IrSchemaKind::Object(obj) => {
            for prop in obj.properties.values() {
                collect_named_from_expr(&prop.type_expr, &mut refs);
            }
            if let Some(ap) = &obj.additional_properties {
                collect_named_from_expr(ap, &mut refs);
            }
        }
        IrSchemaKind::TaggedUnion(tu) => {
            let uses_tuple_variants = matches!(tu.tagging, TaggingStyle::External);
            if uses_tuple_variants {
                for v in &tu.variants {
                    collect_named_from_expr(&v.content_type, &mut refs);
                }
            }
        }
        IrSchemaKind::Union(u) => {
            for member in &u.members {
                collect_named_from_expr(member, &mut refs);
            }
        }
        IrSchemaKind::Alias(expr) => {
            collect_named_from_expr(expr, &mut refs);
        }
        IrSchemaKind::Intersection(inter) => {
            for member in &inter.members {
                collect_named_from_expr(member, &mut refs);
            }
        }
        IrSchemaKind::Enum(_) => {}
    }
    refs
}

fn collect_named_from_expr<'a>(expr: &'a IrTypeExpr, refs: &mut Vec<&'a str>) {
    match expr {
        IrTypeExpr::Named(name) => refs.push(name.as_str()),
        IrTypeExpr::Array(inner) | IrTypeExpr::Map(inner) | IrTypeExpr::Nullable(inner) => {
            collect_named_from_expr(inner, refs);
        }
        IrTypeExpr::Union(members) => {
            for m in members {
                collect_named_from_expr(m, refs);
            }
        }
        _ => {}
    }
}
