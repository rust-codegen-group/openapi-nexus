//! API import statements collection for template rendering

use std::collections::BTreeMap;
use std::ops;

use crate::templating::data::ApiImportStatement;

/// Map of import statements keyed by module_path.
/// Each import statement can contain both types (with inline `type` keyword) and values.
#[derive(Debug, Clone, Default)]
pub struct ApiImportStatements(BTreeMap<String, ApiImportStatement>);

impl ApiImportStatements {
    /// Create a new empty ApiImportStatements
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Get a mutable reference to the underlying map
    pub fn get_mut(&mut self, key: &str) -> Option<&mut ApiImportStatement> {
        self.0.get_mut(key)
    }

    /// Insert an import statement
    pub fn insert(&mut self, key: String, value: ApiImportStatement) -> Option<ApiImportStatement> {
        self.0.insert(key, value)
    }

    /// Get an iterator over the values
    pub fn values(&self) -> impl Iterator<Item = &ApiImportStatement> {
        self.0.values()
    }

    /// Get the number of import statements
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl ops::Deref for ApiImportStatements {
    type Target = BTreeMap<String, ApiImportStatement>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ops::DerefMut for ApiImportStatements {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<BTreeMap<String, ApiImportStatement>> for ApiImportStatements {
    fn from(map: BTreeMap<String, ApiImportStatement>) -> Self {
        Self(map)
    }
}

impl FromIterator<(String, ApiImportStatement)> for ApiImportStatements {
    fn from_iter<T: IntoIterator<Item = (String, ApiImportStatement)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl serde::Serialize for ApiImportStatements {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq;

        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for import in self.0.values() {
            seq.serialize_element(import)?;
        }
        seq.end()
    }
}
