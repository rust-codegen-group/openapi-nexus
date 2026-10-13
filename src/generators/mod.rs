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

/// A generation failure with its original sigil-stitch diagnostic intact.
#[derive(Debug)]
pub(crate) struct EmissionError {
    context: String,
    source: Box<sigil_stitch::error::SigilStitchError>,
}

impl EmissionError {
    pub(crate) fn api(
        language: &str,
        name: &str,
        source: sigil_stitch::error::SigilStitchError,
    ) -> Self {
        Self {
            context: format!("{language} API {name}: build/lower/import/render"),
            source: Box::new(source),
        }
    }

    pub(crate) fn model(
        language: &str,
        name: &str,
        source: sigil_stitch::error::SigilStitchError,
    ) -> Self {
        Self {
            context: format!("{language} model {name}: build/lower/import/render"),
            source: Box::new(source),
        }
    }
}

impl std::fmt::Display for EmissionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.context, self.source)
    }
}

impl std::error::Error for EmissionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source.as_ref())
    }
}

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
