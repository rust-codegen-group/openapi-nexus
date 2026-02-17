//! OpenAPI specification for the Additional Properties API (multi-level HashMaps).

use crate::handlers::{__path_post_leaf, __path_post_middle, __path_post_root};
use crate::models::{LeafValue, MiddleLevel, RootLevel};

/// OpenAPI documentation
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(post_root, post_middle, post_leaf),
    components(
        schemas(
            RootLevel,
            MiddleLevel,
            LeafValue
        )
    ),
    tags(
        (name = "additional-properties", description = "Multi-level structs with HashMap (additionalProperties)")
    ),
    info(
        title = "Additional Properties API",
        version = "1.0.0",
        description = "API demonstrating OpenAPI additionalProperties with multiple levels of structs (RootLevel -> MiddleLevel -> LeafValue), each with HashMap fields.",
    )
)]
pub struct ApiDoc;
