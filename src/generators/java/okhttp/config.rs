use serde::{Deserialize, Serialize};

/// Structural selection policy for generated untagged Gson adapters.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum UntaggedUnionStrategy {
    #[default]
    #[serde(rename = "firstMatch")]
    FirstMatch,
    #[serde(rename = "strict")]
    Strict,
    #[serde(rename = "raw")]
    Raw,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JavaOkhttpConfig {
    #[serde(default)]
    pub package_name: Option<String>,
    #[serde(default)]
    pub untagged_union_strategy: UntaggedUnionStrategy,
}

impl From<toml::value::Table> for JavaOkhttpConfig {
    fn from(table: toml::value::Table) -> Self {
        toml::Value::Table(table).try_into().unwrap_or_else(|e| {
            tracing::warn!("Failed to parse java-okhttp config: {e}; using defaults");
            Self::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untagged_union_strategy_is_explicit_and_defaults_to_first_match() {
        assert_eq!(
            JavaOkhttpConfig::default().untagged_union_strategy,
            UntaggedUnionStrategy::FirstMatch
        );
        for (value, expected) in [
            ("firstMatch", UntaggedUnionStrategy::FirstMatch),
            ("strict", UntaggedUnionStrategy::Strict),
            ("raw", UntaggedUnionStrategy::Raw),
        ] {
            let config: JavaOkhttpConfig =
                toml::from_str(&format!("untagged_union_strategy = '{value}'")).unwrap();
            assert_eq!(config.untagged_union_strategy, expected);
        }
        assert!(toml::from_str::<JavaOkhttpConfig>("untagged_union_strategy = 'unknown'").is_err());
    }
}
