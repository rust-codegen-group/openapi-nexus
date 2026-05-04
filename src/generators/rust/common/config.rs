//! Base configuration shared across all Rust generators.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use tracing::error;

/// Extra derives for a single type-kind (structs, enums, unions, or response structs).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtraDeriveConfig {
    #[serde(default)]
    pub derives: Vec<String>,
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
}

/// Per-type-kind extra derive configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtraDerivesConfig {
    #[serde(default)]
    pub structs: Option<ExtraDeriveConfig>,
    #[serde(default)]
    pub enums: Option<ExtraDeriveConfig>,
    #[serde(default)]
    pub unions: Option<ExtraDeriveConfig>,
    #[serde(default)]
    pub response_structs: Option<ExtraDeriveConfig>,
}

impl ExtraDerivesConfig {
    /// Collect all unique crate dependencies across every type-kind.
    pub fn all_dependencies(&self) -> BTreeMap<String, String> {
        let mut deps = BTreeMap::new();
        for cfg in [
            &self.structs,
            &self.enums,
            &self.unions,
            &self.response_structs,
        ]
        .into_iter()
        .flatten()
        {
            for (k, v) in &cfg.dependencies {
                deps.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
        deps
    }
}

/// Base configuration for Rust generators.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RustGeneratorConfig {
    #[serde(default)]
    pub crate_name: Option<String>,
    #[serde(default)]
    pub extra_derives: Option<ExtraDerivesConfig>,
    #[serde(default)]
    pub workspace_mode: Option<bool>,
}

impl From<toml::value::Table> for RustGeneratorConfig {
    fn from(value: toml::value::Table) -> Self {
        use serde::Deserialize;
        match RustGeneratorConfig::deserialize(value) {
            Ok(config) => config,
            Err(e) => {
                error!(
                    "Failed to parse Rust generator config: {}. Using default configuration.",
                    e
                );
                Self::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(toml_str: &str) -> toml::value::Table {
        toml::from_str::<toml::value::Table>(toml_str).unwrap()
    }

    #[test]
    fn default_config_has_no_extra_derives() {
        let config = RustGeneratorConfig::default();
        assert!(config.crate_name.is_none());
        assert!(config.extra_derives.is_none());
    }

    #[test]
    fn deserialize_empty_table() {
        let config = RustGeneratorConfig::from(toml::value::Table::new());
        assert!(config.crate_name.is_none());
        assert!(config.extra_derives.is_none());
    }

    #[test]
    fn deserialize_crate_name_only() {
        let config = RustGeneratorConfig::from(table(r#"crate_name = "my-sdk""#));
        assert_eq!(config.crate_name.as_deref(), Some("my-sdk"));
        assert!(config.extra_derives.is_none());
    }

    #[test]
    fn deserialize_extra_derives_structs() {
        let config = RustGeneratorConfig::from(table(
            r#"
            [extra_derives.structs]
            derives = ["PartialEq", "Hash"]
            "#,
        ));
        let extra = config.extra_derives.unwrap();
        let structs = extra.structs.unwrap();
        assert_eq!(structs.derives, vec!["PartialEq", "Hash"]);
        assert!(structs.dependencies.is_empty());
        assert!(extra.enums.is_none());
        assert!(extra.unions.is_none());
        assert!(extra.response_structs.is_none());
    }

    #[test]
    fn deserialize_extra_derives_all_kinds() {
        let config = RustGeneratorConfig::from(table(
            r#"
            [extra_derives.structs]
            derives = ["PartialEq"]
            [extra_derives.enums]
            derives = ["Hash"]
            [extra_derives.unions]
            derives = ["Clone"]
            [extra_derives.response_structs]
            derives = ["PartialEq", "Eq"]
            "#,
        ));
        let extra = config.extra_derives.unwrap();
        assert_eq!(extra.structs.unwrap().derives, vec!["PartialEq"]);
        assert_eq!(extra.enums.unwrap().derives, vec!["Hash"]);
        assert_eq!(extra.unions.unwrap().derives, vec!["Clone"]);
        assert_eq!(
            extra.response_structs.unwrap().derives,
            vec!["PartialEq", "Eq"]
        );
    }

    #[test]
    fn deserialize_extra_derives_with_dependencies() {
        let config = RustGeneratorConfig::from(table(
            r#"
            [extra_derives.structs]
            derives = ["utoipa::ToSchema"]
            [extra_derives.structs.dependencies]
            utoipa = '{ version = "5", features = ["openapi_extensions"] }'
            "#,
        ));
        let structs = config.extra_derives.unwrap().structs.unwrap();
        assert_eq!(structs.derives, vec!["utoipa::ToSchema"]);
        assert_eq!(
            structs.dependencies.get("utoipa").unwrap(),
            r#"{ version = "5", features = ["openapi_extensions"] }"#
        );
    }

    #[test]
    fn all_dependencies_merges_across_kinds() {
        let extra = ExtraDerivesConfig {
            structs: Some(ExtraDeriveConfig {
                derives: vec!["utoipa::ToSchema".into()],
                dependencies: BTreeMap::from([
                    ("utoipa".into(), r#"{ version = "5" }"#.into()),
                    ("shared".into(), "\"1.0\"".into()),
                ]),
            }),
            enums: Some(ExtraDeriveConfig {
                derives: vec!["strum::Display".into()],
                dependencies: BTreeMap::from([
                    ("strum".into(), r#"{ version = "0.26" }"#.into()),
                    ("shared".into(), "\"2.0\"".into()),
                ]),
            }),
            unions: None,
            response_structs: None,
        };
        let deps = extra.all_dependencies();
        assert_eq!(deps.len(), 3);
        assert_eq!(deps["shared"], "\"1.0\"");
        assert!(deps.contains_key("utoipa"));
        assert!(deps.contains_key("strum"));
    }

    #[test]
    fn all_dependencies_empty_when_no_extra() {
        let extra = ExtraDerivesConfig::default();
        assert!(extra.all_dependencies().is_empty());
    }

    #[test]
    fn invalid_config_falls_back_to_default() {
        let mut t = toml::value::Table::new();
        t.insert(
            "extra_derives".into(),
            toml::Value::String("invalid".into()),
        );
        let config = RustGeneratorConfig::from(t);
        assert!(config.crate_name.is_none());
        assert!(config.extra_derives.is_none());
    }
}
