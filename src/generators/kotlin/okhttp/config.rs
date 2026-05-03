use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KotlinOkhttpConfig {
    #[serde(default)]
    pub package_name: Option<String>,
}

impl From<toml::value::Table> for KotlinOkhttpConfig {
    fn from(table: toml::value::Table) -> Self {
        toml::Value::Table(table).try_into().unwrap_or_else(|e| {
            tracing::warn!("Failed to parse kotlin-okhttp config: {e}; using defaults");
            Self::default()
        })
    }
}
