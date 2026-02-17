//! Enum representation API handlers

use axum::{http::StatusCode, response::Json};

use crate::models::{
    AdjacentlyTaggedEnum, ExternallyTaggedEnum, InternallyTaggedEnum, MixedEnum, UntaggedEnum,
};

/// Handle externally tagged enum
#[utoipa::path(
    post,
    path = "/externally-tagged",
    request_body = ExternallyTaggedEnum,
    responses(
        (status = 200, description = "Successful operation", body = ExternallyTaggedEnum),
        (status = 400, description = "Invalid input")
    ),
    tag = "enum-repr"
)]
pub async fn handle_externally_tagged(
    Json(value): Json<ExternallyTaggedEnum>,
) -> Result<Json<ExternallyTaggedEnum>, (StatusCode, Json<String>)> {
    // In a real implementation, this would process the enum
    Ok(Json(value))
}

/// Handle internally tagged enum
#[utoipa::path(
    post,
    path = "/internally-tagged",
    request_body = InternallyTaggedEnum,
    responses(
        (status = 200, description = "Successful operation", body = InternallyTaggedEnum),
        (status = 400, description = "Invalid input")
    ),
    tag = "enum-repr"
)]
pub async fn handle_internally_tagged(
    Json(value): Json<InternallyTaggedEnum>,
) -> Result<Json<InternallyTaggedEnum>, (StatusCode, Json<String>)> {
    // In a real implementation, this would process the enum
    Ok(Json(value))
}

/// Handle adjacently tagged enum
#[utoipa::path(
    post,
    path = "/adjacently-tagged",
    request_body = AdjacentlyTaggedEnum,
    responses(
        (status = 200, description = "Successful operation", body = AdjacentlyTaggedEnum),
        (status = 400, description = "Invalid input")
    ),
    tag = "enum-repr"
)]
pub async fn handle_adjacently_tagged(
    Json(value): Json<AdjacentlyTaggedEnum>,
) -> Result<Json<AdjacentlyTaggedEnum>, (StatusCode, Json<String>)> {
    // In a real implementation, this would process the enum
    Ok(Json(value))
}

/// Handle untagged enum
#[utoipa::path(
    post,
    path = "/untagged",
    request_body = UntaggedEnum,
    responses(
        (status = 200, description = "Successful operation", body = UntaggedEnum),
        (status = 400, description = "Invalid input")
    ),
    tag = "enum-repr"
)]
pub async fn handle_untagged(
    Json(value): Json<UntaggedEnum>,
) -> Result<Json<UntaggedEnum>, (StatusCode, Json<String>)> {
    // In a real implementation, this would process the enum
    Ok(Json(value))
}

/// Handle mixed enum (unit variants SimpleA, SimpleB and tuple variants VariantA(VariantA), VariantB(VariantB))
#[utoipa::path(
    post,
    path = "/mixed",
    request_body = MixedEnum,
    responses(
        (status = 200, description = "Successful operation", body = MixedEnum),
        (status = 400, description = "Invalid input")
    ),
    tag = "enum-repr"
)]
pub async fn handle_mixed(
    Json(value): Json<MixedEnum>,
) -> Result<Json<MixedEnum>, (StatusCode, Json<String>)> {
    // In a real implementation, this would process the enum
    Ok(Json(value))
}
