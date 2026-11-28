//! Resolve the logical paths of emitted files before writing or comparing them.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use super::traits::error::WriteError;
use super::traits::file_writer::{FileCategory, FileInfo};

/// Return the same portable, ordered paths for the disk writer and snapshots.
/// The source directory must be resolved by the generator after emission.
pub(crate) fn plan_output<'a>(
    source_dir: Option<&str>,
    files: &'a [FileInfo],
) -> Result<BTreeMap<String, &'a str>, WriteError> {
    let source_dir = source_dir.filter(|path| !path.is_empty());
    if let Some(path) = source_dir {
        validate_relative(path)?;
    }

    let mut output = BTreeMap::new();
    for file in files {
        let category = match file.category {
            FileCategory::None => continue,
            FileCategory::Readme | FileCategory::ProjectFiles => None,
            FileCategory::Apis => Some("apis"),
            FileCategory::Models => Some("models"),
            FileCategory::Runtime => Some("runtime"),
        };
        validate_relative(&file.filename)?;
        let path = match (source_dir, category) {
            (Some(root), Some(category)) => format!("{root}/{category}/{}", file.filename),
            (None, Some(category)) => format!("{category}/{}", file.filename),
            (_, None) => file.filename.clone(),
        };
        match output.entry(path) {
            Entry::Vacant(entry) => {
                entry.insert(file.content.as_str());
            }
            Entry::Occupied(entry) => {
                return Err(WriteError::DuplicateOutputPath {
                    path: entry.key().clone(),
                });
            }
        }
    }

    for path in output.keys() {
        for (index, _) in path.match_indices('/') {
            let parent = &path[..index];
            if output.contains_key(parent) {
                return Err(WriteError::OutputPathConflict {
                    path: parent.into(),
                    child: path.clone(),
                });
            }
        }
    }

    Ok(output)
}

fn validate_relative(path: &str) -> Result<(), WriteError> {
    // Do not use Path::components to normalize aliases. File paths are emitted
    // with '/' separators and must mean the same thing on each host platform.
    if path.contains(['\\', ':', '\0'])
        || path.split('/').any(|part| matches!(part, "" | "." | ".."))
    {
        return Err(WriteError::InvalidOutputPath { path: path.into() });
    }
    Ok(())
}
