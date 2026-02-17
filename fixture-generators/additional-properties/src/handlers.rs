//! Handlers for additional-properties API (multi-level structs with HashMap fields).

use axum::{http::StatusCode, response::Json};

use crate::models::{LeafValue, MiddleLevel, RootLevel};

/// Echo root payload (all three levels with additional-properties-style maps).
#[utoipa::path(
    post,
    path = "/root",
    request_body = RootLevel,
    responses(
        (status = 200, description = "Echo root with children", body = RootLevel),
        (status = 400, description = "Invalid input")
    ),
    tag = "additional-properties"
)]
pub async fn post_root(
    Json(payload): Json<RootLevel>,
) -> Result<Json<RootLevel>, (StatusCode, Json<String>)> {
    Ok(Json(payload))
}

/// Echo middle-level payload (nested + extra maps).
#[utoipa::path(
    post,
    path = "/middle",
    request_body = MiddleLevel,
    responses(
        (status = 200, description = "Echo middle level", body = MiddleLevel),
        (status = 400, description = "Invalid input")
    ),
    tag = "additional-properties"
)]
pub async fn post_middle(
    Json(payload): Json<MiddleLevel>,
) -> Result<Json<MiddleLevel>, (StatusCode, Json<String>)> {
    Ok(Json(payload))
}

/// Echo leaf payload (attributes map).
#[utoipa::path(
    post,
    path = "/leaf",
    request_body = LeafValue,
    responses(
        (status = 200, description = "Echo leaf", body = LeafValue),
        (status = 400, description = "Invalid input")
    ),
    tag = "additional-properties"
)]
pub async fn post_leaf(
    Json(payload): Json<LeafValue>,
) -> Result<Json<LeafValue>, (StatusCode, Json<String>)> {
    Ok(Json(payload))
}
