//! Synthetic request-body input models for transport-specific request shapes.
//!
//! Canonical schema models stay schema-faithful. These models exist only for
//! operation request bodies whose selected media type needs a friendlier caller
//! shape than the schema itself, currently object-shaped multipart/form-data
//! with binary upload parts.

use std::collections::{HashMap, HashSet};

use heck::ToPascalCase as _;

use crate::generators::multipart::{
    MultipartValueEncoding, media_type_base, multipart_parts_for_request_body,
};
use crate::ir::types::{IrOperation, IrRequestBody, IrSpec, IrTypeExpr};

#[derive(Debug, Clone)]
pub struct RequestInputModel {
    pub name: String,
    pub operation_id: String,
    pub media_type: String,
    pub body_required: bool,
    pub fields: Vec<RequestInputField>,
}

#[derive(Debug, Clone)]
pub struct RequestInputField {
    pub wire_name: String,
    pub type_expr: IrTypeExpr,
    pub required: bool,
    pub content_type: String,
    pub value_encoding: MultipartValueEncoding,
    pub kind: RequestInputFieldKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestInputFieldKind {
    SchemaValue,
    UploadFile { default_filename: String },
}

impl RequestInputField {
    pub fn is_upload(&self) -> bool {
        matches!(self.kind, RequestInputFieldKind::UploadFile { .. })
    }

    pub fn default_filename(&self) -> &str {
        match &self.kind {
            RequestInputFieldKind::UploadFile { default_filename } => default_filename,
            RequestInputFieldKind::SchemaValue => &self.wire_name,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RequestInputPlan {
    models: Vec<RequestInputModel>,
    by_operation_media: HashMap<(String, String), usize>,
}

impl RequestInputPlan {
    pub fn empty() -> Self {
        Self {
            models: Vec::new(),
            by_operation_media: HashMap::new(),
        }
    }

    pub fn models(&self) -> &[RequestInputModel] {
        &self.models
    }

    pub fn get(&self, operation_id: &str, media_type: &str) -> Option<&RequestInputModel> {
        let key = (operation_id.to_string(), media_type.to_string());
        self.by_operation_media
            .get(&key)
            .and_then(|idx| self.models.get(*idx))
    }

    pub fn has_uploads(&self) -> bool {
        self.models
            .iter()
            .any(|model| model.fields.iter().any(RequestInputField::is_upload))
    }
}

pub fn plan_multipart_request_inputs(ir: &IrSpec) -> RequestInputPlan {
    let mut plan = RequestInputPlan::empty();
    let mut used_names: HashSet<String> = ir
        .schemas
        .keys()
        .map(|name| name.to_pascal_case())
        .collect();

    for op in &ir.operations {
        let Some(body) = &op.request_body else {
            continue;
        };
        let Some(media_type) = preferred_request_media_type(body) else {
            continue;
        };
        if media_type_base(&media_type) != "multipart/form-data" {
            continue;
        }
        let Some(parts) = multipart_parts_for_request_body(body, &media_type, ir) else {
            continue;
        };

        let base_name = format!(
            "{}MultipartRequestBody",
            sanitize_operation_id(&op.operation_id, &op.method, &op.path).to_pascal_case()
        );
        let name = unique_type_name(&base_name, &mut used_names);
        let fields = parts
            .into_iter()
            .map(|part| {
                let kind = if part.is_binary {
                    RequestInputFieldKind::UploadFile {
                        default_filename: part.default_filename.clone(),
                    }
                } else {
                    RequestInputFieldKind::SchemaValue
                };
                RequestInputField {
                    wire_name: part.wire_name,
                    type_expr: part.type_expr,
                    required: part.required,
                    content_type: part.content_type,
                    value_encoding: part.value_encoding,
                    kind,
                }
            })
            .collect();
        let index = plan.models.len();
        plan.by_operation_media
            .insert((op.operation_id.clone(), media_type.clone()), index);
        plan.models.push(RequestInputModel {
            name,
            operation_id: op.operation_id.clone(),
            media_type,
            body_required: body.required,
            fields,
        });
    }

    plan
}

pub fn request_input_for_operation<'a>(
    plan: &'a RequestInputPlan,
    op: &IrOperation,
    media_type: &str,
) -> Option<&'a RequestInputModel> {
    plan.get(&op.operation_id, media_type)
}

pub fn preferred_request_media_type(body: &IrRequestBody) -> Option<String> {
    pick_media_type(&body.content, |media_type| {
        media_type_base(media_type) == "application/json"
    })
    .or_else(|| pick_media_type(&body.content, is_json_media_type))
    .or_else(|| {
        pick_media_type(&body.content, |media_type| {
            media_type_base(media_type) == "multipart/form-data"
        })
    })
    .or_else(|| {
        pick_media_type(&body.content, |media_type| {
            media_type_base(media_type) == "application/x-www-form-urlencoded"
        })
    })
    .or_else(|| pick_media_type(&body.content, is_xml_media_type))
    .or_else(|| {
        pick_media_type(&body.content, |media_type| {
            media_type_base(media_type) == "text/plain"
        })
    })
    .or_else(|| {
        pick_media_type(&body.content, |media_type| {
            media_type_base(media_type) == "application/octet-stream"
        })
    })
    .or_else(|| body.content.keys().next().cloned())
}

fn unique_type_name(base: &str, used: &mut HashSet<String>) -> String {
    if used.insert(base.to_string()) {
        return base.to_string();
    }
    for i in 2..=u32::MAX {
        let candidate = format!("{base}{i}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!("request input model name collision space exhausted")
}

fn sanitize_operation_id(op_id: &str, method: &str, path: &str) -> String {
    if !op_id.is_empty() {
        return op_id.to_string();
    }
    let path_part: String = path
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    format!("{method}_{path_part}")
}

fn pick_media_type(
    content: &indexmap::IndexMap<String, IrTypeExpr>,
    predicate: impl Fn(&str) -> bool,
) -> Option<String> {
    content
        .keys()
        .find(|media_type| predicate(media_type))
        .cloned()
}

fn is_json_media_type(media_type: &str) -> bool {
    let base = media_type_base(media_type);
    base == "application/json" || base.ends_with("+json")
}

fn is_xml_media_type(media_type: &str) -> bool {
    let base = media_type_base(media_type);
    base == "application/xml" || base == "text/xml" || base.ends_with("+xml")
}
