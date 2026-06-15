//! Shared multipart/form-data request-body planning.

use crate::ir::types::{IrObject, IrPrimitive, IrRequestBody, IrSchemaKind, IrSpec, IrTypeExpr};

#[derive(Debug, Clone)]
pub struct MultipartPart {
    pub wire_name: String,
    pub type_expr: IrTypeExpr,
    pub is_binary: bool,
    pub required: bool,
    pub content_type: String,
    pub value_encoding: MultipartValueEncoding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MultipartValueEncoding {
    Text,
    Json,
    Unsupported,
}

pub fn multipart_parts_for_request_body(
    body: &IrRequestBody,
    media_type: &str,
    ir: &IrSpec,
) -> Option<Vec<MultipartPart>> {
    let t = body.content.get(media_type)?;
    let media_encoding = body.encoding.get(media_type);
    resolve_object(t, ir).map(|obj| {
        obj.properties
            .iter()
            .map(|(wire_name, prop)| {
                let explicit_content_type = media_encoding
                    .and_then(|encoding| encoding.get(wire_name))
                    .and_then(|encoding| encoding.content_type.clone());
                multipart_part_from_property(
                    wire_name,
                    &prop.type_expr,
                    prop.required && !prop.nullable,
                    explicit_content_type,
                    ir,
                )
            })
            .collect()
    })
}

fn multipart_part_from_property(
    wire_name: &str,
    type_expr: &IrTypeExpr,
    required: bool,
    explicit_content_type: Option<String>,
    ir: &IrSpec,
) -> MultipartPart {
    let is_binary = is_binary_type(type_expr, ir);
    let is_text = is_multipart_text_type(type_expr, ir);
    let content_type = explicit_content_type.unwrap_or_else(|| {
        if is_binary {
            "application/octet-stream".to_string()
        } else if is_text {
            "text/plain".to_string()
        } else {
            "application/json".to_string()
        }
    });
    let value_encoding = if is_binary {
        MultipartValueEncoding::Text
    } else if is_json_media_type(&content_type) {
        MultipartValueEncoding::Json
    } else if is_text {
        MultipartValueEncoding::Text
    } else {
        MultipartValueEncoding::Unsupported
    };

    MultipartPart {
        wire_name: wire_name.to_string(),
        type_expr: type_expr.clone(),
        is_binary,
        required,
        content_type,
        value_encoding,
    }
}

fn resolve_object<'a>(expr: &IrTypeExpr, ir: &'a IrSpec) -> Option<&'a IrObject> {
    match expr {
        IrTypeExpr::Named(name) => match ir.schemas.get(name).map(|schema| &schema.kind) {
            Some(IrSchemaKind::Object(obj)) => Some(obj),
            Some(IrSchemaKind::Alias(inner)) => resolve_object(inner, ir),
            _ => None,
        },
        IrTypeExpr::Nullable(inner) => resolve_object(inner, ir),
        _ => None,
    }
}

fn is_binary_type(expr: &IrTypeExpr, ir: &IrSpec) -> bool {
    match expr {
        IrTypeExpr::Primitive(IrPrimitive::Binary) => true,
        IrTypeExpr::Nullable(inner) => is_binary_type(inner, ir),
        IrTypeExpr::Named(name) => ir.schemas.get(name).is_some_and(|schema| {
            matches!(&schema.kind, IrSchemaKind::Alias(inner) if is_binary_type(inner, ir))
        }),
        _ => false,
    }
}

fn is_multipart_text_type(expr: &IrTypeExpr, ir: &IrSpec) -> bool {
    match expr {
        IrTypeExpr::Primitive(_) | IrTypeExpr::StringLiteral(_) | IrTypeExpr::StringEnum(_) => true,
        IrTypeExpr::Nullable(inner) => is_multipart_text_type(inner, ir),
        IrTypeExpr::Named(name) => ir.schemas.get(name).is_some_and(|schema| {
            matches!(&schema.kind, IrSchemaKind::Alias(inner) if is_multipart_text_type(inner, ir))
        }),
        _ => false,
    }
}

pub fn is_json_media_type(media_type: &str) -> bool {
    let base = media_type_base(media_type);
    base == "application/json" || base.ends_with("+json")
}

fn media_type_base(media_type: &str) -> String {
    media_type
        .split(';')
        .next()
        .unwrap_or(media_type)
        .trim()
        .to_ascii_lowercase()
}
