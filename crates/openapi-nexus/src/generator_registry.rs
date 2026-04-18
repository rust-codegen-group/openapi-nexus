//! Generator registry for managing code generators

use std::collections::HashMap;

use tracing::error;

use openapi_nexus_common::GeneratorType;
use openapi_nexus_core::CombinedGenerator;

/// Registry for managing code generators
pub struct GeneratorRegistry {
    generators: HashMap<GeneratorType, Box<dyn CombinedGenerator + Send + Sync>>,
}

impl GeneratorRegistry {
    /// Create a new empty generator registry
    pub fn new() -> Self {
        Self {
            generators: HashMap::new(),
        }
    }

    /// Register a generator, logging an error and skipping if already registered.
    pub fn register_generator<G>(&mut self, generator_type: GeneratorType, generator: G)
    where
        G: CombinedGenerator + Send + Sync + 'static,
    {
        if self.generators.contains_key(&generator_type) {
            error!(
                "Generator '{}' is already registered, skipping registration.",
                generator_type
            );
            return;
        }

        self.generators.insert(generator_type, Box::new(generator));
    }

    /// Get a generator for a specific generator type
    pub fn get_generator(
        &self,
        generator_type: GeneratorType,
    ) -> Option<&(dyn CombinedGenerator + Send + Sync)> {
        self.generators.get(&generator_type).map(|g| g.as_ref())
    }

    /// Check if a generator is registered
    pub fn has_generator(&self, generator_type: GeneratorType) -> bool {
        self.generators.contains_key(&generator_type)
    }

    /// Get all registered generators
    pub fn registered_generators(&self) -> Vec<GeneratorType> {
        self.generators.keys().cloned().collect()
    }

    /// Get the number of registered generators
    pub fn count(&self) -> usize {
        self.generators.len()
    }
}

impl Default for GeneratorRegistry {
    fn default() -> Self {
        Self::new()
    }
}
