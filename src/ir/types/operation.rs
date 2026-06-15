//! Operation types — fully resolved API operations.

use indexmap::IndexMap;
use serde::Serialize;

use super::type_expr::IrTypeExpr;

/// A fully resolved API operation (one HTTP method + path).
#[derive(Debug, Clone, Serialize)]
pub struct IrOperation {
    pub operation_id: String,
    pub tags: Vec<String>,
    pub method: String,
    pub path: String,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub deprecated: bool,
    pub parameters: Vec<IrParameter>,
    pub request_body: Option<IrRequestBody>,
    pub responses: Vec<IrResponse>,
    pub security: Vec<IrSecurityRequirement>,
}

/// A fully resolved parameter.
#[derive(Debug, Clone, Serialize)]
pub struct IrParameter {
    pub name: String,
    pub location: ParameterLocation,
    pub type_expr: IrTypeExpr,
    pub required: bool,
    pub description: Option<String>,
    pub default_value: Option<serde_json::Value>,
}

/// Where a parameter is located.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ParameterLocation {
    Query,
    Header,
    Path,
    Cookie,
}

/// A fully resolved request body.
#[derive(Debug, Clone, Serialize)]
pub struct IrRequestBody {
    pub required: bool,
    pub description: Option<String>,
    /// Media type -> resolved schema type.
    pub content: IndexMap<String, IrTypeExpr>,
    /// Media type -> property name -> request-body encoding metadata.
    pub encoding: IndexMap<String, IndexMap<String, IrRequestBodyEncoding>>,
}

/// Request-body encoding metadata for a single media-type property.
#[derive(Debug, Clone, Serialize)]
pub struct IrRequestBodyEncoding {
    pub content_type: Option<String>,
}

/// A fully resolved response.
#[derive(Debug, Clone, Serialize)]
pub struct IrResponse {
    pub status: String,
    pub description: String,
    pub content: IndexMap<String, IrTypeExpr>,
    /// Streaming item types (from OAS 3.2 `itemSchema`).
    /// Non-empty when the response is a streaming endpoint (e.g. text/event-stream).
    pub item_content: IndexMap<String, IrTypeExpr>,
    pub headers: IndexMap<String, IrHeader>,
}

/// A resolved response header.
#[derive(Debug, Clone, Serialize)]
pub struct IrHeader {
    pub description: Option<String>,
    pub type_expr: IrTypeExpr,
    pub required: bool,
}

/// Security requirement (operation-level or spec-level).
#[derive(Debug, Clone, Serialize)]
pub struct IrSecurityRequirement {
    pub scheme_name: String,
    pub scopes: Vec<String>,
}
