//! Configuration file structure for TOML deserialization

use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use serde::Deserialize;

use super::global_config::GlobalConfig;
use crate::codegen::GeneratorType;

/// Configuration file structure (for deserialization from TOML)
#[derive(Debug, Clone, Default)]
pub struct ConfigFile {
    /// Global settings
    pub global: GlobalConfig,
    /// Generator-specific configurations stored as TOML tables
    pub generators: HashMap<GeneratorType, toml::value::Table>,
}

impl<'de> Deserialize<'de> for ConfigFile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{Error, MapAccess, Visitor};

        struct ConfigFileVisitor;

        impl<'de> Visitor<'de> for ConfigFileVisitor {
            type Value = ConfigFile;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a TOML table")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut global = None;
                let mut generators = HashMap::new();

                while let Some(key) = map.next_key::<String>()? {
                    if key == "global" {
                        if global.is_some() {
                            return Err(A::Error::duplicate_field("global"));
                        }
                        global = Some(map.next_value()?);
                    } else {
                        // All other keys are treated as generator configs
                        // Parse the string key as a Generator
                        let generator = GeneratorType::from_str(&key).map_err(|e| {
                            A::Error::custom(format!("Invalid generator name '{}': {}", key, e))
                        })?;
                        let table = map.next_value::<toml::value::Table>()?;
                        generators.insert(generator, table);
                    }
                }

                Ok(ConfigFile {
                    global: global.unwrap_or_default(),
                    generators,
                })
            }
        }

        deserializer.deserialize_map(ConfigFileVisitor)
    }
}
