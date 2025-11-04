//! OpenAPI specification parser

use std::fs;
use std::path::Path;

use tracing;
use utoipa::openapi::OpenApi;

use crate::error::ParseError;
use crate::serde_error::SerdeErrorExtractor;
use crate::yaml_context::YamlContextExtractor;

/// Parse an OpenAPI specification from a file
pub fn parse_file(path: &Path) -> Result<OpenApi, ParseError> {
    let content = fs::read_to_string(path).map_err(|e| ParseError::FileRead {
        path: path.to_string_lossy().to_string(),
        source: e,
    })?;

    let file_extension = path.extension().and_then(|ext| ext.to_str());

    match file_extension {
        Some("json") => {
            serde_json::from_str(&content).map_err(|e| ParseError::JsonParse { source: e })
        }
        Some("yaml") | Some("yml") => {
            serde_norway::from_str(&content).map_err(|e| {
                // Extract location information from error message
                let error_msg = e.to_string();
                let (line, column) = SerdeErrorExtractor::new(&error_msg).extract_location();

                // Build context lines
                let context = if line > 0 {
                    YamlContextExtractor::new(&content).extract_context(line, column)
                } else {
                    vec![
                        format!("Error: {}", error_msg),
                        String::new(),
                        "Unable to extract location context.".to_string(),
                    ]
                };

                // Print context lines using tracing
                for line in &context {
                    tracing::error!("{}", line);
                }

                ParseError::YamlParse { source: e, context }
            })
        }
        Some(ext) => Err(ParseError::UnsupportedFormat {
            format: ext.to_string(),
        }),
        None => {
            // Try JSON first, then YAML
            serde_json::from_str(&content)
                .or_else(|_| {
                    serde_norway::from_str(&content).map_err(|e| {
                        let error_msg = e.to_string();
                        let (line, column) =
                            SerdeErrorExtractor::new(&error_msg).extract_location();
                        let context = if line > 0 {
                            YamlContextExtractor::new(&content).extract_context(line, column)
                        } else {
                            vec![format!("Error: {}", error_msg)]
                        };

                        // Print context lines using tracing
                        for line in &context {
                            tracing::error!("{}", line);
                        }

                        ParseError::YamlParse { source: e, context }
                    })
                })
                .map_err(|_| ParseError::UnsupportedFormat {
                    format: "unknown".to_string(),
                })
        }
    }
}
