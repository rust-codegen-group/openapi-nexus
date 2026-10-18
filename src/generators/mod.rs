pub(crate) mod closed_sum;
pub mod error;
pub mod go;
pub mod java;
pub mod kotlin;
pub mod multipart;
pub mod python;
pub mod registry;
pub mod request_inputs;
pub mod response_headers;
pub mod response_names;
pub mod rust;
pub mod typescript;

pub use error::{ArtifactKind, GeneratorError};

/// Build a semantic type application using the sigil-stitch 0.7 API.
///
/// The helper keeps repetitive application construction consistent while
/// preserving the type arguments for import collection and language lowering.
pub(crate) fn type_application(
    base: sigil_stitch::type_name::TypeName,
    arguments: Vec<sigil_stitch::type_name::TypeName>,
) -> sigil_stitch::type_name::TypeName {
    sigil_stitch::type_name::TypeName::application(
        base,
        arguments
            .into_iter()
            .map(sigil_stitch::spec::where_spec::TypeArgument::Single)
            .collect(),
    )
}

mod orchestrator;

pub use orchestrator::OpenApiCodeGenerator;
pub use registry::GeneratorRegistry;
