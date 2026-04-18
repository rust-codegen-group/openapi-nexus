use openapi_nexus_spec::oas31::spec::Parameter;

pub trait OperationInfoExt {
    /// Returns the computed method name for this operation, using the
    /// `operation_id` if available, or generating one based on the HTTP method
    /// and path if not.
    fn method_name(&self) -> String;

    fn parameters(&self) -> Vec<Parameter>;
}
