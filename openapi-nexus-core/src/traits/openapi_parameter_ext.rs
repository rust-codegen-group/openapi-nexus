use utoipa::openapi;

/// Extension trait for OpenAPI `Parameter` to provide convenience methods.
pub trait OpenApiParameterExt {
    /// Returns `true` if the parameter is required.
    fn required(&self) -> bool;

    /// Returns `true` if the parameter is deprecated.
    fn deprecated(&self) -> bool;
}

impl OpenApiParameterExt for openapi::path::Parameter {
    fn required(&self) -> bool {
        matches!(self.required, openapi::Required::True)
    }

    fn deprecated(&self) -> bool {
        matches!(self.deprecated, Some(openapi::Deprecated::True))
    }
}
