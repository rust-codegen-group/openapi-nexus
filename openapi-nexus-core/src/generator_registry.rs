//! Generator registry for managing language-specific code generators

use std::collections::HashMap;

use crate::traits::code_generator::LanguageCodeGenerator;
use crate::traits::file_writer::FileWriter;
use openapi_nexus_common::Language;

/// Combined trait for generators that can both generate code and write files
pub trait LanguageGenerator: LanguageCodeGenerator + FileWriter {}

/// Registry for managing language-specific code generators
pub struct GeneratorRegistry {
    generators: HashMap<Language, Box<dyn LanguageGenerator + Send + Sync>>,
}

impl GeneratorRegistry {
    /// Create a new empty generator registry
    pub fn new() -> Self {
        Self {
            generators: HashMap::new(),
        }
    }

    /// Register a language generator
    pub fn register_generator<G>(&mut self, language: Language, generator: G) -> Result<(), String>
    where
        G: LanguageGenerator + Send + Sync + 'static,
    {
        if self.generators.contains_key(&language) {
            return Err(format!(
                "Generator for language '{}' is already registered",
                language
            ));
        }

        self.generators.insert(language, Box::new(generator));
        Ok(())
    }

    /// Get a generator for a specific language
    pub fn get_generator(
        &self,
        language: Language,
    ) -> Option<&(dyn LanguageGenerator + Send + Sync)> {
        self.generators.get(&language).map(|g| g.as_ref())
    }

    /// Check if a generator is registered for a language
    pub fn has_generator(&self, language: Language) -> bool {
        self.generators.contains_key(&language)
    }

    /// Get all registered languages
    pub fn registered_languages(&self) -> Vec<Language> {
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
