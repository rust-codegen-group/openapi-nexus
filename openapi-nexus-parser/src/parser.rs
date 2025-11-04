//! OpenAPI specification parser

use std::fs;
use std::path::Path;

use tracing::{debug, error};
use utoipa::openapi::OpenApi;

use crate::error::ParseError;
use crate::serde_error::SerdeErrorExtractor;

fn extract_error_context(content: &str, error_msg: &str) -> Vec<String> {
    let (line, column) = SerdeErrorExtractor::new(error_msg).extract_location();

    if line > 0 {
        debug!("Error at line {}, column {}", line, column);

        let lines: Vec<&str> = content.lines().collect();
        if line <= lines.len() {
            let error_line_idx = line - 1;
            let start_line = error_line_idx.saturating_sub(5);
            let end_line = (error_line_idx + 10).min(lines.len());

            debug!(
                "Raw content around error (lines {} to {}):",
                start_line + 1,
                end_line
            );
            for (i, line_content) in lines.iter().enumerate().take(end_line).skip(start_line) {
                let line_num = i + 1;
                let is_error_line = line_num == line;
                let marker = if is_error_line { ">>>" } else { "   " };
                debug!("{} {} | {}", marker, line_num, line_content);
            }

            if error_line_idx + 1 < lines.len() {
                let next_line = lines[error_line_idx + 1];
                debug!("Next line after error: {}", next_line);
                debug!(
                    "Next line indentation: {} spaces",
                    next_line.chars().take_while(|c| *c == ' ').count()
                );
            }

            let error_line = lines[line - 1];
            vec![
                format!("Error at line {}: {}", line, error_line),
                format!("Column: {}", column),
            ]
        } else {
            vec![format!("Error: {}", error_msg)]
        }
    } else {
        vec![format!("Error: {}", error_msg)]
    }
}

pub fn parse_content_json(content: &str) -> Result<OpenApi, ParseError> {
    let value: serde_json::Value = serde_json::from_str(content).map_err(|e| {
        let error_msg = e.to_string();
        debug!("Serde error message: {}", error_msg);
        let context = extract_error_context(content, &error_msg);

        for line in &context {
            error!("{}", line);
        }

        ParseError::JsonParse { source: e, context }
    })?;

    serde_json::from_value(value).map_err(|e| ParseError::OpenApiDeserializeJson { source: e })
}

pub fn parse_content_yaml(content: &str) -> Result<OpenApi, ParseError> {
    let value: serde_norway::Value = serde_norway::from_str(content).map_err(|e| {
        let error_msg = e.to_string();
        debug!("Serde error message: {}", error_msg);
        let context = extract_error_context(content, &error_msg);

        for line in &context {
            error!("{}", line);
        }

        ParseError::YamlParse { source: e, context }
    })?;

    serde_norway::from_value(value).map_err(|e| ParseError::OpenApiDeserializeYaml { source: e })
}

/// Parse an OpenAPI specification from a file
pub fn parse_file(path: &Path) -> Result<OpenApi, ParseError> {
    let content = fs::read_to_string(path).map_err(|e| ParseError::FileRead {
        path: path.to_string_lossy().to_string(),
        source: e,
    })?;

    let file_extension = path.extension().and_then(|ext| ext.to_str());

    match file_extension {
        Some("json") => parse_content_json(&content),
        Some("yaml") | Some("yml") => parse_content_yaml(&content),
        Some(ext) => Err(ParseError::UnsupportedFormat {
            format: ext.to_string(),
        }),
        None => Err(ParseError::UnsupportedFormat {
            format: "<unknown>".to_string(),
        }),
    }
}
