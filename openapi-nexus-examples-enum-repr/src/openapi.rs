//! OpenAPI specification for the Enum Representation API

use crate::handlers::{
    __path_handle_adjacently_tagged, __path_handle_externally_tagged,
    __path_handle_internally_tagged, __path_handle_untagged,
};
use crate::models::{
    AdjacentlyTaggedEnum, ExternallyTaggedEnum, InternallyTaggedEnum, UntaggedEnum,
};

/// OpenAPI documentation
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        handle_externally_tagged,
        handle_internally_tagged,
        handle_adjacently_tagged,
        handle_untagged
    ),
    components(
        schemas(
            ExternallyTaggedEnum,
            InternallyTaggedEnum,
            AdjacentlyTaggedEnum,
            UntaggedEnum
        )
    ),
    tags(
        (name = "enum-repr", description = "Enum representation examples")
    ),
    info(
        title = "Enum Representation API",
        version = "1.0.0",
        description = "This API demonstrates all 4 kinds of enum representation types: Externally Tagged, Internally Tagged, Adjacently Tagged, and Untagged",
    )
)]
pub struct ApiDoc;
