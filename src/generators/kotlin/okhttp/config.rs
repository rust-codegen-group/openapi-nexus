use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KotlinOkhttpConfig {
    #[serde(default)]
    pub package_name: Option<String>,
    #[serde(default)]
    pub untagged_union_strategy: UntaggedUnionStrategy,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UntaggedUnionStrategy {
    #[default]
    FirstMatch,
    Strict,
    Raw,
}

impl From<toml::value::Table> for KotlinOkhttpConfig {
    fn from(table: toml::value::Table) -> Self {
        toml::Value::Table(table).try_into().unwrap_or_else(|e| {
            tracing::warn!("Failed to parse kotlin-okhttp config: {e}; using defaults");
            Self::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policies_are_explicit_and_default_to_first_match() {
        assert_eq!(
            KotlinOkhttpConfig::default().untagged_union_strategy,
            UntaggedUnionStrategy::FirstMatch
        );
        for (value, policy) in [
            ("firstMatch", UntaggedUnionStrategy::FirstMatch),
            ("strict", UntaggedUnionStrategy::Strict),
            ("raw", UntaggedUnionStrategy::Raw),
        ] {
            let config: KotlinOkhttpConfig =
                toml::from_str(&format!("untagged_union_strategy = {value:?}")).unwrap();
            assert_eq!(config.untagged_union_strategy, policy);
        }
        assert!(
            toml::from_str::<KotlinOkhttpConfig>("untagged_union_strategy = 'unknown'").is_err()
        );
    }
}
