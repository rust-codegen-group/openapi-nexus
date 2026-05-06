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
    #[serde(default)]
    pub per_type: Option<BTreeMap<String, ExtraDeriveConfig>>,
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
        if let Some(per_type) = &self.per_type {
            for cfg in per_type.values() {
                for (k, v) in &cfg.dependencies {
                    deps.entry(k.clone()).or_insert_with(|| v.clone());
                }
            }
        }
        deps
    }

    /// Warn about derives that reference external crates without a matching
    /// entry in `dependencies`. Call this during codegen so users get feedback.
    pub fn warn_missing_dependencies(&self) {
        let deps = self.all_dependencies();
        for cfg in [
            &self.structs,
            &self.enums,
            &self.unions,
            &self.response_structs,
        ]
        .into_iter()
        .flatten()
        {
            Self::check_derives(&cfg.derives, &deps);
        }
        if let Some(per_type) = &self.per_type {
            for cfg in per_type.values() {
                Self::check_derives(&cfg.derives, &deps);
            }
        }
    }

    fn check_derives(derives: &[String], deps: &BTreeMap<String, String>) {
        for d in derives {
            if let Some(crate_name) = d.split("::").next()
                && crate_name != d
            {
                let normalized = crate_name.replace('-', "_");
                if !deps.contains_key(&normalized) {
                    tracing::warn!(
                        "Derive `{d}` references crate `{normalized}` but no matching entry \
                         exists in `extra_derives.*.dependencies`. Add \
                         `[extra_derives.<kind>.dependencies]\n{normalized} = '{{ version = \"...\" }}'` \
                         to include it in the generated Cargo.toml."
                    );
                }
            }
        }
    }
}

/// How dependencies are rendered in the generated Cargo.toml.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceDepsMode {
    /// Version and features specified inline (default).
    #[default]
    Explicit,
    /// `dep = { workspace = true, features = [...] }` — version from workspace, features explicit.
    WorkspaceVersion,
    /// `dep.workspace = true` — everything from workspace.
    Full,
}

/// Base configuration for Rust generators.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RustGeneratorConfig {
    #[serde(default, alias = "package_name")]
    pub crate_name: Option<String>,
    #[serde(default)]
    pub extra_derives: Option<ExtraDerivesConfig>,
    #[serde(default)]
    pub workspace_mode: Option<bool>,
    #[serde(default)]
    pub workspace_deps: Option<WorkspaceDepsMode>,
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
            per_type: None,
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

    #[test]
    fn package_name_alias_resolves_to_crate_name() {
        let config = RustGeneratorConfig::from(table(r#"package_name = "my-api""#));
        assert_eq!(config.crate_name.as_deref(), Some("my-api"));
    }

    #[test]
    fn workspace_deps_mode_explicit() {
        let config = RustGeneratorConfig::from(table(r#"workspace_deps = "explicit""#));
        assert_eq!(
            config.workspace_deps,
            Some(super::WorkspaceDepsMode::Explicit)
        );
    }

    #[test]
    fn workspace_deps_mode_workspace_version() {
        let config = RustGeneratorConfig::from(table(r#"workspace_deps = "workspace_version""#));
        assert_eq!(
            config.workspace_deps,
            Some(super::WorkspaceDepsMode::WorkspaceVersion)
        );
    }

    #[test]
    fn workspace_deps_mode_full() {
        let config = RustGeneratorConfig::from(table(r#"workspace_deps = "full""#));
        assert_eq!(config.workspace_deps, Some(super::WorkspaceDepsMode::Full));
    }

    #[test]
    fn per_type_extra_derives() {
        let config = RustGeneratorConfig::from(table(
            r#"
            [extra_derives.per_type.PaymentMethod]
            derives = ["Hash", "PartialEq"]

            [extra_derives.per_type.PaymentMethod.dependencies]
            dummy = '"1.0"'
            "#,
        ));
        let extra = config.extra_derives.unwrap();
        let per_type = extra.per_type.unwrap();
        let pm = per_type.get("PaymentMethod").unwrap();
        assert_eq!(pm.derives, vec!["Hash", "PartialEq"]);
        assert_eq!(pm.dependencies.get("dummy").unwrap(), "\"1.0\"");
    }

    #[test]
    fn all_dependencies_includes_per_type() {
        let extra = ExtraDerivesConfig {
            structs: Some(ExtraDeriveConfig {
                derives: vec!["utoipa::ToSchema".into()],
                dependencies: BTreeMap::from([("utoipa".into(), r#"{ version = "5" }"#.into())]),
            }),
            enums: None,
            unions: None,
            response_structs: None,
            per_type: Some(BTreeMap::from([(
                "MyType".into(),
                ExtraDeriveConfig {
                    derives: vec!["Hash".into()],
                    dependencies: BTreeMap::from([("custom".into(), "\"2.0\"".into())]),
                },
            )])),
        };
        let deps = extra.all_dependencies();
        assert_eq!(deps.len(), 2);
        assert!(deps.contains_key("utoipa"));
        assert!(deps.contains_key("custom"));
    }

    #[test]
    fn all_dependencies_does_not_auto_infer() {
        let extra = ExtraDerivesConfig {
            structs: Some(ExtraDeriveConfig {
                derives: vec!["utoipa::ToSchema".into(), "Clone".into()],
                dependencies: BTreeMap::new(),
            }),
            enums: Some(ExtraDeriveConfig {
                derives: vec!["strum::Display".into()],
                dependencies: BTreeMap::new(),
            }),
            unions: None,
            response_structs: None,
            per_type: None,
        };
        let deps = extra.all_dependencies();
        assert!(deps.is_empty());
    }

    #[test]
    fn warn_missing_dependencies_does_not_panic() {
        let extra = ExtraDerivesConfig {
            structs: Some(ExtraDeriveConfig {
                derives: vec!["utoipa::ToSchema".into()],
                dependencies: BTreeMap::from([("utoipa".into(), r#"{ version = "5" }"#.into())]),
            }),
            enums: Some(ExtraDeriveConfig {
                derives: vec!["strum::Display".into()],
                dependencies: BTreeMap::new(),
            }),
            unions: None,
            response_structs: None,
            per_type: None,
        };
        extra.warn_missing_dependencies();
    }
}
