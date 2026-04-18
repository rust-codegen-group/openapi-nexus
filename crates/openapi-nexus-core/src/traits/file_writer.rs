//! Trait for language-specific file writing operations

use std::collections::HashMap;
use std::fs;

/// File category for organizing generated files
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FileCategory {
    /// Represents files without a category. Used internally.
    /// These files are not written to the output directory.
    None,
    /// README or documentation files (e.g., README.md)
    Readme,
    /// API client classes
    Apis,
    /// Data models and schemas
    Models,
    /// Project configuration files (index.ts, package.json, etc.)
    ProjectFiles,
    /// Runtime utilities
    Runtime,
}

/// Generic file information for writing
#[derive(Debug, Clone)]
pub struct FileInfo {
    pub filename: String,
    pub content: String,
    pub category: FileCategory,
}

impl FileInfo {
    /// Create a new FileInfo with the specified category
    pub fn new(filename: String, content: String, category: FileCategory) -> Self {
        Self {
            filename,
            content,
            category,
        }
    }

    /// Create a new FileInfo without a category
    pub fn none(filename: String, content: String) -> Self {
        Self::new(filename, content, FileCategory::None)
    }

    /// Create a new FileInfo for README files
    pub fn readme(filename: String, content: String) -> Self {
        Self::new(filename, content, FileCategory::Readme)
    }

    /// Create a new FileInfo for API files
    pub fn api(filename: String, content: String) -> Self {
        Self::new(filename, content, FileCategory::Apis)
    }

    /// Create a new FileInfo for model files
    pub fn model(filename: String, content: String) -> Self {
        Self::new(filename, content, FileCategory::Models)
    }

    /// Create a new FileInfo for project files
    pub fn project(filename: String, content: String) -> Self {
        Self::new(filename, content, FileCategory::ProjectFiles)
    }

    /// Create a new FileInfo for runtime files
    pub fn runtime(filename: String, content: String) -> Self {
        Self::new(filename, content, FileCategory::Runtime)
    }
}

/// Trait for language-specific file writing operations
pub trait FileWriter {
    /// Write generated files to the output directory
    fn write_files(
        &self,
        output_dir: &std::path::Path,
        files: &[FileInfo],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Group files by category
        let mut files_by_category: HashMap<FileCategory, Vec<&FileInfo>> = HashMap::new();
        for file in files {
            files_by_category
                .entry(file.category.clone())
                .or_default()
                .push(file);
        }

        // Write files for each category
        for (category, category_files) in files_by_category {
            let category_dir = match category {
                FileCategory::None => continue,
                FileCategory::Readme => output_dir.to_path_buf(),
                FileCategory::Apis => output_dir.join("apis"),
                FileCategory::Models => output_dir.join("models"),
                FileCategory::ProjectFiles => output_dir.to_path_buf(),
                FileCategory::Runtime => output_dir.join("runtime"),
            };

            // Create directory if it doesn't exist
            if !category_dir.exists() {
                fs::create_dir_all(&category_dir)?;
            }

            // Write files in this category
            for file in category_files {
                let file_path = category_dir.join(&file.filename);

                // Create parent directories if they don't exist (for subdirectories)
                if let Some(parent) = file_path.parent() {
                    fs::create_dir_all(parent)?;
                }

                fs::write(&file_path, &file.content)?;
            }
        }

        Ok(())
    }
}
