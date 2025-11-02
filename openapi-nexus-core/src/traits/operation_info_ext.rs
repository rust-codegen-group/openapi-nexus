use utoipa::openapi;

pub trait OperationInfoExt {
    /// Returns the computed method name for this operation, using the
    /// `operation_id` if available, or generating one based on the HTTP method
    /// and path if not.
    fn method_name(&self) -> String;

    fn parameters(&self) -> Vec<openapi::path::Parameter>;
}
