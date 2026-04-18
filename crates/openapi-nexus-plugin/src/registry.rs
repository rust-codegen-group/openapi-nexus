//! Plugin registry implementation

use std::collections::HashMap;

use crate::traits::{Plugin, PluginCapability, PluginError, PluginRegistry};

/// Simple in-memory plugin registry
pub struct SimplePluginRegistry {
    plugins: HashMap<String, Box<dyn Plugin>>,
}

impl SimplePluginRegistry {
    /// Create a new plugin registry
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }
}

impl PluginRegistry for SimplePluginRegistry {
    fn register_plugin(&mut self, plugin: Box<dyn Plugin>) -> Result<(), PluginError> {
        let name = plugin.metadata().name.clone();
        if self.plugins.contains_key(&name) {
            return Err(PluginError::Generic {
                message: format!("Plugin '{}' is already registered", name),
            });
        }
        self.plugins.insert(name, plugin);
        Ok(())
    }

    fn get_plugin(&self, name: &str) -> Option<&dyn Plugin> {
        self.plugins.get(name).map(|p| p.as_ref())
    }

    fn list_plugins(&self) -> Vec<&dyn Plugin> {
        self.plugins.values().map(|p| p.as_ref()).collect()
    }

    fn get_plugins_by_capability(&self, capability: &PluginCapability) -> Vec<&dyn Plugin> {
        self.plugins
            .values()
            .filter(|plugin| plugin.metadata().capabilities.contains(capability))
            .map(|p| p.as_ref())
            .collect()
    }

    fn unregister_plugin(&mut self, name: &str) -> Result<(), PluginError> {
        if self.plugins.remove(name).is_some() {
            Ok(())
        } else {
            Err(PluginError::PluginNotFound {
                name: name.to_string(),
            })
        }
    }
}

impl Default for SimplePluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}
