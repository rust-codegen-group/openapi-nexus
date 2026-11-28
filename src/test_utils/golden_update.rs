//! Stage snapshot writes without exposing the existing snapshots to partial output.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::{self, FileType, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub(super) fn requested(value: Option<&OsStr>) -> bool {
    value == Some(OsStr::new("1"))
}

pub(super) fn replace_tree(
    root: &Path,
    case: &str,
    generated: &HashMap<String, String>,
) -> io::Result<()> {
    if !safe_output_path(case) || case.contains('/') {
        return Err(invalid(format!("invalid golden case directory: {case:?}")));
    }
    if generated.is_empty() {
        return Err(invalid("refusing to replace snapshots with empty output"));
    }

    fs::create_dir_all(root)?;
    let target = root.join(case);
    let mut stage = Stage::claim(&target)?;
    let had_previous = if let Some(kind) = file_type(&target)? {
        if !kind.is_dir() {
            return Err(not_owned(&target));
        }
        check_owned_tree(&target)?;
        true
    } else {
        false
    };

    let next = stage.path.join("next");
    fs::create_dir(&next)?;
    for (name, content) in generated {
        if !safe_output_path(name) {
            return Err(invalid(format!("invalid generated path: {name:?}")));
        }
        let path = next.join(format!("{name}.golden"));
        fs::create_dir_all(path.parent().expect("staged file has a parent"))?;
        write_new(&path, content)?;
    }

    let previous = stage.path.join("previous");
    if had_previous {
        fs::rename(&target, &previous)?;
    }
    if let Err(error) = fs::rename(&next, &target) {
        // A directory cannot portably replace a nonempty directory in one
        // rename. Roll back on a failed publish; never clean up the only copy.
        if had_previous && let Err(restore_error) = fs::rename(&previous, &target) {
            stage.retain = true;
            return Err(io::Error::other(format!(
                "could not publish snapshots ({error}) or restore them ({restore_error}); previous snapshots are at {}",
                previous.display()
            )));
        }
        return Err(error);
    }
    Ok(())
}

pub(super) fn replace_file(target: &Path, content: &str) -> io::Result<()> {
    if !is_snapshot(target) {
        return Err(not_owned(target));
    }
    let stage = Stage::claim(target)?;
    if file_type(target)?.is_some_and(|kind| !kind.is_file()) {
        return Err(not_owned(target));
    }
    let next = stage.path.join("next");
    write_new(&next, content)?;
    fs::rename(next, target)
}

fn safe_output_path(path: &str) -> bool {
    !path.contains(['\\', ':']) && path.split('/').all(|part| !matches!(part, "" | "." | ".."))
}

fn is_snapshot(path: &Path) -> bool {
    path.extension() == Some(OsStr::new("golden"))
}

fn check_owned_tree(path: &Path) -> io::Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let path = entry.path();
        if kind.is_dir() {
            check_owned_tree(&path)?;
        } else if !kind.is_file() || !is_snapshot(&path) {
            return Err(not_owned(&path));
        }
    }
    Ok(())
}

fn file_type(path: &Path) -> io::Result<Option<FileType>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata.file_type())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn write_new(path: &Path, content: &str) -> io::Result<()> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .write_all(content.as_bytes())
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

fn not_owned(path: &Path) -> io::Error {
    invalid(format!(
        "refusing to replace unowned path: {}",
        path.display()
    ))
}

/// A fixed sibling name prevents concurrent updates of the same case and keeps
/// a recoverable backup in place if the process stops during publication.
struct Stage {
    path: PathBuf,
    retain: bool,
}

impl Stage {
    fn claim(target: &Path) -> io::Result<Self> {
        let name = target
            .file_name()
            .ok_or_else(|| invalid("golden target must have a filename"))?;
        let mut stage_name = std::ffi::OsString::from(".");
        stage_name.push(name);
        stage_name.push(".update");
        let path = target.with_file_name(stage_name);
        fs::create_dir(&path).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "cannot claim golden staging directory {}: {error}",
                    path.display()
                ),
            )
        })?;
        Ok(Self {
            path,
            retain: false,
        })
    }
}

impl Drop for Stage {
    fn drop(&mut self) {
        if !self.retain
            && let Err(error) = fs::remove_dir_all(&self.path)
        {
            eprintln!(
                "could not remove golden staging directory {}: {error}",
                self.path.display()
            );
        }
    }
}
