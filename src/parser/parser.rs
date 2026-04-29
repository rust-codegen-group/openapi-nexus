//! OpenAPI specification parser

use std::fs;
use std::path::Path;

use tracing::{debug, error};

use super::error::ParseError;
use super::serde_error::SerdeErrorExtractor;
use crate::ParsedSpec;
use crate::spec::{OpenApiV30Spec, OpenApiV31Spec, OpenApiV32Spec};

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

/// Detect the OpenAPI version from a JSON value.
fn detect_version_json(value: &serde_json::Value) -> Result<&str, ParseError> {
    value
        .get("openapi")
        .and_then(|v| v.as_str())
        .ok_or(ParseError::MissingVersionField)
}

/// Detect the OpenAPI version from a YAML value.
fn detect_version_yaml(value: &serde_norway::Value) -> Result<String, ParseError> {
    let mapping = value.as_mapping().ok_or(ParseError::MissingVersionField)?;
    for (k, v) in mapping {
        if k.as_str() == Some("openapi") {
            if let Some(s) = v.as_str() {
                return Ok(s.to_string());
            }
            // Could be a number like 3.0 (YAML parses unquoted 3.0 as float)
            if let Some(f) = v.as_f64() {
                // Preserve at least one decimal place: 3.0 → "3.0", not "3"
                let s = format!("{f}");
                if s.contains('.') {
                    return Ok(s);
                }
                return Ok(format!("{f:.1}"));
            }
        }
    }
    Err(ParseError::MissingVersionField)
}

/// Classify a version string into a major.minor bucket.
fn classify_version(version: &str) -> Result<OpenApiMajorMinor, ParseError> {
    if version.starts_with("3.0") {
        Ok(OpenApiMajorMinor::V3_0)
    } else if version.starts_with("3.1") {
        Ok(OpenApiMajorMinor::V3_1)
    } else if version.starts_with("3.2") {
        Ok(OpenApiMajorMinor::V3_2)
    } else {
        Err(ParseError::UnsupportedVersion {
            version: version.to_string(),
        })
    }
}

enum OpenApiMajorMinor {
    V3_0,
    V3_1,
    V3_2,
}

pub fn parse_content_json(content: &str) -> Result<ParsedSpec, ParseError> {
    // First parse as generic JSON to get error context and version
    let value: serde_json::Value = serde_json::from_str(content).map_err(|e| {
        let error_msg = e.to_string();
        debug!("Serde error message: {}", error_msg);
        let context = extract_error_context(content, &error_msg);
        for line in &context {
            error!("{}", line);
        }
        ParseError::JsonParse { source: e, context }
    })?;

    let version = detect_version_json(&value)?;
    match classify_version(version)? {
        OpenApiMajorMinor::V3_0 => {
            let spec: OpenApiV30Spec = serde_json::from_value(value).map_err(|e| {
                debug!("parse error: {}", e);
                ParseError::OpenApiDeserializeJson { source: e }
            })?;
            Ok(ParsedSpec::V30(Box::new(spec)))
        }
        OpenApiMajorMinor::V3_1 => {
            let spec: OpenApiV31Spec = serde_json::from_value(value).map_err(|e| {
                debug!("parse error: {}", e);
                ParseError::OpenApiDeserializeJson { source: e }
            })?;
            Ok(ParsedSpec::V31(Box::new(spec)))
        }
        OpenApiMajorMinor::V3_2 => {
            let spec: OpenApiV32Spec = serde_json::from_value(value).map_err(|e| {
                debug!("parse error: {}", e);
                ParseError::OpenApiDeserializeJson { source: e }
            })?;
            Ok(ParsedSpec::V32(Box::new(spec)))
        }
    }
}

pub fn parse_content_yaml(content: &str) -> Result<ParsedSpec, ParseError> {
    // First parse as generic YAML
    let value: serde_norway::Value = serde_norway::from_str(content).map_err(|e| {
        let error_msg = e.to_string();
        debug!("Serde error message: {}", error_msg);
        let context = extract_error_context(content, &error_msg);
        for line in &context {
            error!("{}", line);
        }
        ParseError::YamlParse { source: e, context }
    })?;

    let version = detect_version_yaml(&value)?;
    match classify_version(&version)? {
        OpenApiMajorMinor::V3_0 => {
            let spec: OpenApiV30Spec = serde_norway::from_value(value).map_err(|e| {
                debug!("parse error: {}", e);
                ParseError::OpenApiDeserializeYaml { source: e }
            })?;
            Ok(ParsedSpec::V30(Box::new(spec)))
        }
        OpenApiMajorMinor::V3_1 => {
            let spec: OpenApiV31Spec = serde_norway::from_value(value).map_err(|e| {
                debug!("parse error: {}", e);
                ParseError::OpenApiDeserializeYaml { source: e }
            })?;
            Ok(ParsedSpec::V31(Box::new(spec)))
        }
        OpenApiMajorMinor::V3_2 => {
            let spec: OpenApiV32Spec = serde_norway::from_value(value).map_err(|e| {
                debug!("parse error: {}", e);
                ParseError::OpenApiDeserializeYaml { source: e }
            })?;
            Ok(ParsedSpec::V32(Box::new(spec)))
        }
    }
}

/// Parse an OpenAPI specification from a file
pub fn parse_file(path: &Path) -> Result<ParsedSpec, ParseError> {
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

/// Parse content as an OpenAPI v3.1 spec specifically (for callers that know the version).
pub fn parse_content_yaml_v31(content: &str) -> Result<OpenApiV31Spec, ParseError> {
    let _value: serde_norway::Value = serde_norway::from_str(content).map_err(|e| {
        let error_msg = e.to_string();
        let context = extract_error_context(content, &error_msg);
        ParseError::YamlParse { source: e, context }
    })?;

    serde_norway::from_str::<OpenApiV31Spec>(content)
        .map_err(|e| ParseError::OpenApiDeserializeYaml { source: e })
}

/// Parse content as an OpenAPI v3.1 JSON spec specifically.
pub fn parse_content_json_v31(content: &str) -> Result<OpenApiV31Spec, ParseError> {
    let _value: serde_json::Value = serde_json::from_str(content).map_err(|e| {
        let error_msg = e.to_string();
        let context = extract_error_context(content, &error_msg);
        ParseError::JsonParse { source: e, context }
    })?;

    serde_json::from_str::<OpenApiV31Spec>(content)
        .map_err(|e| ParseError::OpenApiDeserializeJson { source: e })
}
