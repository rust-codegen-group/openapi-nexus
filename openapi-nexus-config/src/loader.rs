//! Configuration file loader

use std::error::Error;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::config::ConfigFile;
use toml;

/// Configuration file loader
pub struct ConfigLoader;

impl ConfigLoader {
    /// Discover and load configuration file from standard search paths
    ///
    /// Search order:
    /// 1. ./openapi-nexus-config.toml
    /// 2. ./.openapi-nexus-config.toml
    /// 3. ~/.config/openapi-nexus/config.toml
    /// 4. /etc/openapi-nexus/config.toml
    pub fn discover_config_file() -> Option<PathBuf> {
        // 1. Current directory - openapi-nexus-config.toml
        let current_dir_file = Path::new("openapi-nexus-config.toml");
        if current_dir_file.exists() {
            return Some(current_dir_file.to_path_buf());
        }

        // 2. Current directory - .openapi-nexus-config.toml
        let hidden_file = Path::new(".openapi-nexus-config.toml");
        if hidden_file.exists() {
            return Some(hidden_file.to_path_buf());
        }

        // 3. User config directory - OS-specific config directory
        // e.g., ~/.config/openapi-nexus/config.toml on Linux
        if let Some(config_dir) = dirs::config_dir() {
            let user_config = config_dir.join("openapi-nexus").join("config.toml");
            if user_config.exists() {
                return Some(user_config);
            }
        }

        // 4. System config directory - /etc/openapi-nexus/config.toml
        let system_config = Path::new("/etc/openapi-nexus/config.toml");
        if system_config.exists() {
            return Some(system_config.to_path_buf());
        }

        None
    }

    /// Load configuration from a specific file path
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<ConfigFile, LoadError> {
        let content = fs::read_to_string(path.as_ref()).map_err(|e| LoadError::FileRead {
            path: path.as_ref().to_path_buf(),
            source: e,
        })?;

        toml::from_str(&content).map_err(|e| LoadError::Parse {
            path: path.as_ref().to_path_buf(),
            source: e,
        })
    }

    /// Load configuration from discovered file or return default
    pub fn load_or_default() -> ConfigFile {
        Self::discover_config_file()
            .and_then(|path| Self::load_from_file(&path).ok())
            .unwrap_or_default()
    }
}

/// Configuration loading errors
#[derive(Debug)]
pub enum LoadError {
    FileRead {
        path: PathBuf,
        source: io::Error,
    },
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::FileRead { path, source } => {
                write!(f, "Failed to read config file at {:?}: {}", path, source)
            }
            LoadError::Parse { path, source } => {
                write!(f, "Failed to parse config file at {:?}: {}", path, source)
            }
        }
    }
}

impl Error for LoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            LoadError::FileRead { source, .. } => Some(source),
            LoadError::Parse { source, .. } => Some(source),
        }
    }
}
