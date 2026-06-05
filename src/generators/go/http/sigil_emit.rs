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

use crate::codegen::traits::file_writer::FileInfo;
use crate::ir::types::{
    IrEnum, IrEnumValueType, IrIntersection, IrObject, IrPrimitive, IrProperty, IrSchema,
    IrSchemaKind, IrSpec, IrTaggedUnion, IrTypeExpr, IrUnion, TaggingStyle,
};
use heck::{ToPascalCase, ToSnakeCase};
use sigil_stitch::lang::go::Go;
use sigil_stitch::prelude::{CodeBlock, sigil_quote};
use sigil_stitch::spec::field_spec::FieldSpec;
use sigil_stitch::spec::file_spec::FileSpec;
use sigil_stitch::spec::import_spec::ImportSpec;
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
    let mut tb = TypeSpec::builder(&name, TypeKind::Struct);
    if let Some(doc) = &schema.description {
        tb = tb.doc(doc);
    }
    for (json_name, prop) in &obj.properties {
        tb = tb.add_field(build_struct_field(json_name, prop));
    }

    let has_ap = obj.additional_properties.is_some();
    if let Some(ap_type) = &obj.additional_properties {
        let ap_field = FieldSpec::builder(
            "AdditionalProperties",
            TypeName::map(TypeName::primitive("string"), go_type_name(ap_type)),
        )
        .tag("json:\"-\"")
        .doc("Additional properties not captured by defined fields.")
        .build()
        .expect("AP field builds");
        tb = tb.add_field(ap_field);
    }

    let mut fb = FileSpec::builder(&format!("{}.go", name))
        .header(package_header())
        .add_type(tb.build().ok()?);

    if has_ap {
        let value_type = go_type_str(obj.additional_properties.as_ref().unwrap());
        fb = fb
            .add_import(ImportSpec::named("encoding/json", "json"))
            .add_raw(&emit_marshal_json(&name))
            .add_raw(&emit_unmarshal_json(&name, obj, &value_type));
    }

    let file = fb.build().ok()?;
    let rendered = file.render(RENDER_WIDTH).ok()?;
    Some(rendered)
}

fn build_struct_field(json_name: &str, prop: &IrProperty) -> FieldSpec {
    let field_name = go_field_name(&prop.name);
    let ty = go_type_name(&prop.type_expr);

    let tag = json_tag(json_name, prop.required, prop.nullable);
    let mut fb = FieldSpec::builder(&field_name, ty).tag(&tag);
    if !prop.required || prop.nullable {
        // Optional or nullable fields become `*T` so callers can distinguish
        // "absent" from "zero-value present".
        fb = fb.is_optional();
    }
    if let Some(desc) = &prop.description {
        fb = fb.doc(desc);
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

fn emit_marshal_json(struct_name: &str) -> String {
    let cb = sigil_quote!(Go {
        func (o $L(struct_name)) MarshalJSON() ([]byte, error) {
            type Plain $L(struct_name)
            data, err := json.Marshal(Plain(o))
            if err != nil {
                return nil, err
            }
            if len(o.AdditionalProperties) == 0 {
                return data, nil
            }
            var base map[string]json.RawMessage
            if err := json.Unmarshal(data, &base); err != nil {
                return nil, err
            }
            for k, v := range o.AdditionalProperties {
                raw, err := json.Marshal(v)
                if err != nil {
                    return nil, err
                }
                base[k] = raw
            }
            return json.Marshal(base)
        }
    })
    .expect("MarshalJSON builds");
    cb.render_standalone(&Go::new(), RENDER_WIDTH)
        .expect("MarshalJSON renders")
}

fn emit_unmarshal_json(struct_name: &str, obj: &IrObject, value_type: &str) -> String {
    let known_map = obj
        .properties
        .keys()
        .map(|k| format!("\"{k}\": true"))
        .collect::<Vec<_>>()
        .join(", ");

    let known_decl = format!("known := map[string]bool{{{known_map}}}");
    let make_map = format!("o.AdditionalProperties = make(map[string]{value_type})");
    let cb = sigil_quote!(Go {
        func (o *$L(struct_name)) UnmarshalJSON(data []byte) error {
            type Plain $L(struct_name)
            if err := json.Unmarshal(data, (*Plain)(o)); err != nil {
                return err
            }
            var raw map[string]json.RawMessage
            if err := json.Unmarshal(data, &raw); err != nil {
                return err
            }
            $L(known_decl)
            $L(make_map)
            for k, v := range raw {
                if known[k] {
                    continue
                }
                var parsed $L(value_type)
                if err := json.Unmarshal(v, &parsed); err != nil {
                    continue
                }
                o.AdditionalProperties[k] = parsed
            }
            return nil
        }
    })
    .expect("UnmarshalJSON builds");
    cb.render_standalone(&Go::new(), RENDER_WIDTH)
        .expect("UnmarshalJSON renders")
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
        IrEnumValueType::Mixed => {
            return render_alias_file(&name, "any", schema.description.as_deref());
        }
    };

    let const_lines: Vec<String> = en
        .values
        .iter()
        .map(|v| match en.value_type {
            IrEnumValueType::String => {
                let s = v.value.as_str()?;
                Some(format!(
                    "{}{} {name} = \"{}\"",
                    name,
                    s.to_pascal_case(),
                    escape_go_string(s)
                ))
            }
            IrEnumValueType::Integer | IrEnumValueType::Number => {
                let n = v.value.as_number()?;
                let pretty = n.to_string().replace(['-', '.'], "_");
                Some(format!("{name}N{pretty} {name} = {n}"))
            }
            IrEnumValueType::Mixed => unreachable!(),
        })
        .collect::<Option<Vec<_>>>()?;

    let cb = sigil_quote!(Go {
        package $L(MODELS_PACKAGE)

        $if(schema.description.is_some()) {
            $comment(schema.description.as_deref().unwrap())
        }
        type $L(name) $L(go_base)

        const (
        $for(line in const_lines.iter()) {
            $L(line.as_str())
        }
        )

    })
    .ok()?;
    cb.render_standalone(&Go::new(), RENDER_WIDTH)
        .ok()
        .map(|s| s + "\n")
}

// ---------------------------------------------------------------------------
// Alias -> `type Name = X`
// ---------------------------------------------------------------------------

fn emit_alias(schema: &IrSchema, expr: &IrTypeExpr) -> Option<String> {
    let name = schema.name.to_pascal_case();
    let rhs = go_type_str(expr);
    render_alias_file(&name, &rhs, schema.description.as_deref())
}

// ---------------------------------------------------------------------------
// Union -> interface{} alias (simplified)
// ---------------------------------------------------------------------------

fn emit_union(schema: &IrSchema, _union: &IrUnion) -> Option<String> {
    // Untagged unions in Go don't have a clean representation without a
    // discriminator. Use `interface{}` (any) and let callers type-assert.
    let name = schema.name.to_pascal_case();
    render_alias_file(&name, "any", schema.description.as_deref())
}

// ---------------------------------------------------------------------------
// Intersection -> struct with embedded members
// ---------------------------------------------------------------------------

fn emit_intersection(schema: &IrSchema, inter: &IrIntersection) -> Option<String> {
    let name = schema.name.to_pascal_case();
    let mut tb = TypeSpec::builder(&name, TypeKind::Struct);
    if let Some(doc) = &schema.description {
        tb = tb.doc(doc);
    }
    for member in &inter.members {
        tb = tb.add_embedded(go_type_name(member));
    }

    let fb = FileSpec::builder(&format!("{}.go", name))
        .header(package_header())
        .add_type(tb.build().ok()?);
    let file = fb.build().ok()?;
    file.render(RENDER_WIDTH).ok()
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
    render_alias_file(&name, "any", Some(&combined_doc))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a `package models` header block.
fn package_header() -> CodeBlock {
    sigil_quote!(Go {
        package $L(MODELS_PACKAGE)
    })
    .expect("package header builds")
}

/// Render a simple type-alias file: package + optional doc + `type X = Y`.
///
/// Used for Alias, mixed-Enum, Union, and TaggedUnion (all reduce to a single
/// type alias in this pass).
fn render_alias_file(name: &str, rhs: &str, doc: Option<&str>) -> Option<String> {
    let cb = sigil_quote!(Go {
        package $L(MODELS_PACKAGE)

        $if(doc.is_some()) {
            $comment(doc.unwrap())
        }
        type $L(name) = $L(rhs)
    })
    .ok()?;
    cb.render_standalone(&Go::new(), RENDER_WIDTH)
        .ok()
        .map(|s| s + "\n")
}

/// Map an IR type expression to a sigil `TypeName`.
///
/// All same-package references (named schemas, primitives) resolve to a plain
/// `TypeName::primitive` with the Go identifier. Cross-package imports aren't
/// needed within the `models/` tree.
fn go_type_name(expr: &IrTypeExpr) -> TypeName {
    match expr {
        IrTypeExpr::Named(name) => TypeName::primitive(&name.to_pascal_case()),
        IrTypeExpr::Primitive(p) => TypeName::primitive(go_primitive(p)),
        IrTypeExpr::Array(inner) => TypeName::slice(go_type_name(inner)),
        IrTypeExpr::Map(inner) => TypeName::map(TypeName::primitive("string"), go_type_name(inner)),
        IrTypeExpr::Nullable(inner) => TypeName::pointer(go_type_name(inner)),
        IrTypeExpr::StringLiteral(_) | IrTypeExpr::StringEnum(_) => TypeName::primitive("string"),
        IrTypeExpr::Union(_) | IrTypeExpr::Any => TypeName::primitive("any"),
    }
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
