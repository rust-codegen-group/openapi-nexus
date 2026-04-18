//! Error types for the IR (Intermediate Representation) layer

use snafu::Snafu;

use openapi_nexus_common::SourceLocation;

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum IrError {
    #[snafu(display("Circular reference detected: {} in path {:?}", reference, path))]
    CircularReference {
        reference: String,
        path: Vec<String>,
        location: SourceLocation,
    },

    #[snafu(display("Unresolved reference: {}", reference))]
    UnresolvedReference {
        reference: String,
        location: SourceLocation,
    },

    #[snafu(display("Invalid reference '{}': {}", reference, reason))]
    InvalidReference {
        reference: String,
        reason: String,
        location: SourceLocation,
    },

    #[snafu(display("Schema analysis error: {}", message))]
    AnalysisError {
        message: String,
        location: SourceLocation,
    },

    #[snafu(display("External reference not supported: {}", reference))]
    ExternalReference {
        reference: String,
        location: SourceLocation,
    },
}

#[cfg(test)]
mod tests {
    use crate::error::IrError;
    use openapi_nexus_common::SourceLocation;

    #[test]
    fn test_circular_reference_error() {
        let error = IrError::CircularReference {
            reference: "User".to_string(),
            path: vec![
                "User".to_string(),
                "Profile".to_string(),
                "User".to_string(),
            ],
            location: SourceLocation::new(),
        };

        let error_msg = format!("{}", error);
        assert!(error_msg.contains("Circular reference detected"));
        assert!(error_msg.contains("User"));
    }

    #[test]
    fn test_unresolved_reference_error() {
        let error = IrError::UnresolvedReference {
            reference: "#/components/schemas/NonExistent".to_string(),
            location: SourceLocation::new(),
        };

        let error_msg = format!("{}", error);
        assert!(error_msg.contains("Unresolved reference"));
        assert!(error_msg.contains("NonExistent"));
    }

    #[test]
    fn test_external_reference_error() {
        let error = IrError::ExternalReference {
            reference: "https://example.com/schema.json".to_string(),
            location: SourceLocation::new(),
        };

        let error_msg = format!("{}", error);
        assert!(error_msg.contains("External reference not supported"));
        assert!(error_msg.contains("https://example.com/schema.json"));
    }

    #[test]
    fn test_invalid_reference_error() {
        let error = IrError::InvalidReference {
            reference: "invalid-ref".to_string(),
            reason: "Must start with '#/components/'".to_string(),
            location: SourceLocation::new(),
        };

        let error_msg = format!("{}", error);
        assert!(error_msg.contains("Invalid reference"));
        assert!(error_msg.contains("invalid-ref"));
        assert!(error_msg.contains("Must start with '#/components/'"));
    }

    #[test]
    fn test_analysis_error() {
        let error = IrError::AnalysisError {
            message: "Schema validation failed".to_string(),
            location: SourceLocation::new(),
        };

        let error_msg = format!("{}", error);
        assert!(error_msg.contains("Schema analysis error"));
        assert!(error_msg.contains("Schema validation failed"));
    }

    #[test]
    fn test_error_display_formatting() {
        let location = SourceLocation::new();
        let error = IrError::AnalysisError {
            message: "Test error message".to_string(),
            location: location.clone(),
        };

        let error_string = format!("{}", error);
        assert!(error_string.contains("Test error message"));
    }

    #[test]
    fn test_error_debug_formatting() {
        let location = SourceLocation::new();
        let error = IrError::AnalysisError {
            message: "Test error message".to_string(),
            location: location.clone(),
        };

        let debug_string = format!("{:?}", error);
        assert!(debug_string.contains("AnalysisError"));
        assert!(debug_string.contains("Test error message"));
    }

    #[test]
    fn test_circular_reference_with_complex_path() {
        let error = IrError::CircularReference {
            reference: "User".to_string(),
            path: vec![
                "User".to_string(),
                "Profile".to_string(),
                "Address".to_string(),
                "Country".to_string(),
                "User".to_string(),
            ],
            location: SourceLocation::new(),
        };

        let error_msg = format!("{}", error);
        assert!(error_msg.contains("Circular reference detected"));
        assert!(error_msg.contains("User"));
        assert!(error_msg.contains("Profile"));
        assert!(error_msg.contains("Address"));
        assert!(error_msg.contains("Country"));
    }

    #[test]
    fn test_unresolved_reference_with_special_characters() {
        let error = IrError::UnresolvedReference {
            reference: "#/components/schemas/User-Profile_v2.0".to_string(),
            location: SourceLocation::new(),
        };

        let error_msg = format!("{}", error);
        assert!(error_msg.contains("Unresolved reference"));
        assert!(error_msg.contains("User-Profile_v2.0"));
    }

    #[test]
    fn test_external_reference_with_different_protocols() {
        let error1 = IrError::ExternalReference {
            reference: "http://example.com/schema.json".to_string(),
            location: SourceLocation::new(),
        };

        let error2 = IrError::ExternalReference {
            reference: "https://api.example.com/schema.json".to_string(),
            location: SourceLocation::new(),
        };

        let error_msg1 = format!("{}", error1);
        let error_msg2 = format!("{}", error2);

        assert!(error_msg1.contains("External reference not supported"));
        assert!(error_msg1.contains("http://example.com/schema.json"));

        assert!(error_msg2.contains("External reference not supported"));
        assert!(error_msg2.contains("https://api.example.com/schema.json"));
    }

    #[test]
    fn test_invalid_reference_with_different_formats() {
        let error1 = IrError::InvalidReference {
            reference: "invalid-ref".to_string(),
            reason: "Must start with '#/components/'".to_string(),
            location: SourceLocation::new(),
        };

        let error2 = IrError::InvalidReference {
            reference: "/components/schemas/User".to_string(),
            reason: "Missing '#' prefix".to_string(),
            location: SourceLocation::new(),
        };

        let error_msg1 = format!("{}", error1);
        let error_msg2 = format!("{}", error2);

        assert!(error_msg1.contains("Invalid reference"));
        assert!(error_msg1.contains("invalid-ref"));
        assert!(error_msg1.contains("Must start with '#/components/'"));

        assert!(error_msg2.contains("Invalid reference"));
        assert!(error_msg2.contains("/components/schemas/User"));
        assert!(error_msg2.contains("Missing '#' prefix"));
    }

    #[test]
    fn test_analysis_error_with_long_message() {
        let long_message = "This is a very long error message that explains in detail what went wrong during the analysis process. It includes multiple sentences and provides comprehensive information about the issue.";

        let error = IrError::AnalysisError {
            message: long_message.to_string(),
            location: SourceLocation::new(),
        };

        let error_msg = format!("{}", error);
        assert!(error_msg.contains("Schema analysis error"));
        assert!(error_msg.contains(long_message));
    }

    #[test]
    fn test_error_with_empty_strings() {
        let error = IrError::AnalysisError {
            message: "".to_string(),
            location: SourceLocation::new(),
        };

        let error_msg = format!("{}", error);
        assert!(error_msg.contains("Schema analysis error"));
    }

    #[test]
    fn test_error_with_unicode_characters() {
        let error = IrError::AnalysisError {
            message: "测试错误消息 🚀".to_string(),
            location: SourceLocation::new(),
        };

        let error_msg = format!("{}", error);
        assert!(error_msg.contains("测试错误消息 🚀"));
    }

    #[test]
    fn test_circular_reference_with_empty_path() {
        let error = IrError::CircularReference {
            reference: "User".to_string(),
            path: vec![],
            location: SourceLocation::new(),
        };

        let error_msg = format!("{}", error);
        assert!(error_msg.contains("Circular reference detected"));
        assert!(error_msg.contains("User"));
    }

    #[test]
    fn test_error_pattern_matching() {
        let location = SourceLocation::new();
        let error = IrError::AnalysisError {
            message: "Test error".to_string(),
            location: location.clone(),
        };

        match error {
            IrError::AnalysisError {
                message,
                location: loc,
            } => {
                assert_eq!(message, "Test error");
                assert_eq!(loc, location);
            }
            _ => panic!("Expected AnalysisError"),
        }
    }

    #[test]
    fn test_error_equality() {
        let location1 = SourceLocation::new();
        let location2 = SourceLocation::new();

        let error1 = IrError::AnalysisError {
            message: "Same error".to_string(),
            location: location1,
        };

        let error2 = IrError::AnalysisError {
            message: "Same error".to_string(),
            location: location2,
        };

        // Note: This test assumes the error types implement PartialEq
        // If they don't, we can test the individual fields instead
        let msg1 = format!("{}", error1);
        let msg2 = format!("{}", error2);
        assert_eq!(msg1, msg2);
    }
}
