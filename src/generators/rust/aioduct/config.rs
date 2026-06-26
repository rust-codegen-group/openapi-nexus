//! Rust aioduct generator-specific configuration.

pub use crate::generators::rust::common::config::RustGeneratorConfig as RustAioductConfig;

use serde::{Deserialize, Serialize};

const DEFAULT_AIODUCT_VERSION: &str = "0.2";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AioductRuntime {
    #[default]
    Tokio,
    Smol,
    Compio,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AioductTls {
    #[default]
    RustlsRing,
    RustlsAwsLcRs,
    #[serde(rename = "false", alias = "disabled", alias = "none")]
    Disabled,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AioductFeatureConfig {
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub runtime: Option<AioductRuntime>,
    #[serde(default)]
    pub tls: Option<AioductTls>,
    #[serde(default)]
    pub compression: Option<Vec<String>>,
    #[serde(default)]
    pub features: Option<Vec<String>>,
}

impl AioductFeatureConfig {
    pub fn version(&self) -> &str {
        self.version.as_deref().unwrap_or(DEFAULT_AIODUCT_VERSION)
    }

    pub fn resolved_features(&self) -> Vec<String> {
        let mut feats = Vec::new();

        // Runtime (exactly one required)
        match self.runtime.as_ref().unwrap_or(&AioductRuntime::Tokio) {
            AioductRuntime::Tokio => feats.push("tokio".to_string()),
            AioductRuntime::Smol => feats.push("smol".to_string()),
            AioductRuntime::Compio => feats.push("compio".to_string()),
        }

        // TLS
        match self.tls.as_ref().unwrap_or(&AioductTls::RustlsRing) {
            AioductTls::RustlsRing => {
                feats.push("rustls".to_string());
                feats.push("rustls-ring".to_string());
            }
            AioductTls::RustlsAwsLcRs => {
                feats.push("rustls".to_string());
                feats.push("rustls-aws-lc-rs".to_string());
            }
            AioductTls::Disabled => {}
        }

        // JSON (always required for generated SDK)
        feats.push("json".to_string());

        // Compression
        if let Some(compression) = &self.compression {
            for c in compression {
                let normalized = c.to_lowercase();
                if matches!(normalized.as_str(), "gzip" | "brotli" | "zstd" | "deflate") {
                    feats.push(normalized);
                }
            }
        }

        // Pass-through extras
        if let Some(extras) = &self.features {
            for f in extras {
                if !feats.contains(f) {
                    feats.push(f.clone());
                }
            }
        }

        feats
    }

    pub fn features_toml_array(&self) -> String {
        let feats = self.resolved_features();
        let quoted: Vec<String> = feats.iter().map(|f| format!("\"{f}\"")).collect();
        format!("[{}]", quoted.join(", "))
    }
}

pub fn default_aioduct_features() -> AioductFeatureConfig {
    AioductFeatureConfig::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_resolves_to_tokio_rustls_ring_json() {
        let config = AioductFeatureConfig::default();
        assert_eq!(
            config.resolved_features(),
            vec!["tokio", "rustls", "rustls-ring", "json"]
        );
    }

    #[test]
    fn default_version_is_current() {
        let config = AioductFeatureConfig::default();
        assert_eq!(config.version(), "0.2");
    }

    #[test]
    fn custom_version_override() {
        let config = AioductFeatureConfig {
            version: Some("0.2".to_string()),
            ..Default::default()
        };
        assert_eq!(config.version(), "0.2");
    }

    #[test]
    fn smol_runtime() {
        let config = AioductFeatureConfig {
            runtime: Some(AioductRuntime::Smol),
            ..Default::default()
        };
        let feats = config.resolved_features();
        assert!(feats.contains(&"smol".to_string()));
        assert!(!feats.contains(&"tokio".to_string()));
    }

    #[test]
    fn aws_lc_rs_tls() {
        let config = AioductFeatureConfig {
            tls: Some(AioductTls::RustlsAwsLcRs),
            ..Default::default()
        };
        let feats = config.resolved_features();
        assert!(feats.contains(&"rustls".to_string()));
        assert!(feats.contains(&"rustls-aws-lc-rs".to_string()));
        assert!(!feats.contains(&"rustls-ring".to_string()));
    }

    #[test]
    fn tls_disabled() {
        let config = AioductFeatureConfig {
            tls: Some(AioductTls::Disabled),
            ..Default::default()
        };
        let feats = config.resolved_features();
        assert!(!feats.contains(&"rustls".to_string()));
        assert!(!feats.contains(&"rustls-ring".to_string()));
    }

    #[test]
    fn compression_features() {
        let config = AioductFeatureConfig {
            compression: Some(vec!["gzip".to_string(), "brotli".to_string()]),
            ..Default::default()
        };
        let feats = config.resolved_features();
        assert!(feats.contains(&"gzip".to_string()));
        assert!(feats.contains(&"brotli".to_string()));
    }

    #[test]
    fn invalid_compression_ignored() {
        let config = AioductFeatureConfig {
            compression: Some(vec!["lz4".to_string(), "gzip".to_string()]),
            ..Default::default()
        };
        let feats = config.resolved_features();
        assert!(feats.contains(&"gzip".to_string()));
        assert!(!feats.contains(&"lz4".to_string()));
    }

    #[test]
    fn passthrough_features() {
        let config = AioductFeatureConfig {
            features: Some(vec!["tracing".to_string(), "http3".to_string()]),
            ..Default::default()
        };
        let feats = config.resolved_features();
        assert!(feats.contains(&"tracing".to_string()));
        assert!(feats.contains(&"http3".to_string()));
    }

    #[test]
    fn passthrough_no_duplicates() {
        let config = AioductFeatureConfig {
            features: Some(vec!["tokio".to_string(), "json".to_string()]),
            ..Default::default()
        };
        let feats = config.resolved_features();
        assert_eq!(feats.iter().filter(|f| *f == "tokio").count(), 1);
        assert_eq!(feats.iter().filter(|f| *f == "json").count(), 1);
    }

    #[test]
    fn features_toml_array_format() {
        let config = AioductFeatureConfig::default();
        assert_eq!(
            config.features_toml_array(),
            r#"["tokio", "rustls", "rustls-ring", "json"]"#
        );
    }

    #[test]
    fn deserialize_from_toml() {
        let toml_str = r#"
            version = "0.2"
            runtime = "smol"
            tls = "rustls-aws-lc-rs"
            compression = ["gzip", "zstd"]
            features = ["tracing"]
        "#;
        let config: AioductFeatureConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.version(), "0.2");
        assert_eq!(config.runtime, Some(AioductRuntime::Smol));
        assert_eq!(config.tls, Some(AioductTls::RustlsAwsLcRs));
        assert_eq!(
            config.compression,
            Some(vec!["gzip".to_string(), "zstd".to_string()])
        );
        assert_eq!(config.features, Some(vec!["tracing".to_string()]));
    }

    #[test]
    fn deserialize_tls_disabled() {
        let toml_str = r#"tls = "false""#;
        let config: AioductFeatureConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.tls, Some(AioductTls::Disabled));
    }
}
