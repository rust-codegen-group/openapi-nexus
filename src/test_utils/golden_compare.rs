//! Read-only comparison of complete golden snapshot trees.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::{fs, io, path::Path};

use similar::TextDiff;

/// Compare exact output bytes and report every difference in path order.
pub(super) fn compare(
    spec_dir: &Path,
    generated: &HashMap<String, String>,
) -> io::Result<Option<String>> {
    let metadata = at_path(spec_dir, fs::symlink_metadata(spec_dir))?;
    if !metadata.is_dir() {
        return Err(invalid(
            spec_dir,
            "expected a golden directory, not a symlink or file",
        ));
    }

    let mut expected = BTreeMap::new();
    read_snapshots(spec_dir, "", &mut expected)?;
    let paths: BTreeSet<_> = expected.keys().chain(generated.keys()).collect();
    let mut report = String::new();
    for path in paths {
        match (expected.get(path), generated.get(path)) {
            (None, Some(_)) => writeln!(report, "Added: {path}").unwrap(),
            (Some(_), None) => writeln!(report, "Removed (stale snapshot): {path}").unwrap(),
            (Some(old), Some(new)) if old != new => {
                writeln!(report, "Changed: {path}").unwrap();
                let diff = TextDiff::from_lines(old, new);
                writeln!(
                    report,
                    "{}",
                    diff.unified_diff().context_radius(3).header(
                        &format!("golden/{path}.golden"),
                        &format!("generated/{path}")
                    )
                )
                .unwrap();
            }
            _ => {}
        }
    }
    Ok((!report.is_empty()).then_some(report))
}

fn read_snapshots(
    directory: &Path,
    prefix: &str,
    expected: &mut BTreeMap<String, String>,
) -> io::Result<()> {
    let mut entries = at_path(directory, fs::read_dir(directory))?
        .collect::<io::Result<Vec<_>>>()
        .map_err(|error| with_path(directory, error))?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let kind = at_path(&path, entry.file_type())?;
        let name = entry.file_name();
        // Append the actual OS component. Only directory boundaries contribute
        // slashes, so a literal Unix backslash cannot alias a nested file.
        let name = name
            .to_str()
            .ok_or_else(|| invalid(&path, "golden path is not UTF-8"))?;
        if kind.is_dir() {
            read_snapshots(&path, &format!("{prefix}{name}/"), expected)?;
        } else if kind.is_file() {
            let stem = name
                .strip_suffix(".golden")
                .filter(|stem| !stem.is_empty())
                .ok_or_else(|| invalid(&path, "golden file does not have a .golden suffix"))?;
            let content = at_path(&path, fs::read_to_string(&path))?;
            expected.insert(format!("{prefix}{stem}"), content);
        } else {
            return Err(invalid(
                &path,
                "golden entries must be regular files or directories",
            ));
        }
    }
    Ok(())
}

fn invalid(path: &Path, message: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("{}: {message}", path.display()),
    )
}

fn with_path(path: &Path, error: io::Error) -> io::Error {
    io::Error::new(error.kind(), format!("{}: {error}", path.display()))
}

fn at_path<T>(path: &Path, result: io::Result<T>) -> io::Result<T> {
    result.map_err(|error| with_path(path, error))
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            loop {
                let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
                let path = env::temp_dir()
                    .join(format!("nexus-golden-compare-{}-{seq}", std::process::id()));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("could not create {}: {error}", path.display()),
                }
            }
        }

        fn write(&self, name: impl AsRef<Path>, bytes: impl AsRef<[u8]>) {
            let path = self.0.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn files(entries: &[(&str, &str)]) -> HashMap<String, String> {
        entries
            .iter()
            .map(|&(path, content)| (path.into(), content.into()))
            .collect()
    }

    #[test]
    fn reports_added_stale_and_changed_files_together_in_path_order() {
        let tree = TempDir::new();
        tree.write("models/c.rs.golden", "stale\n");
        tree.write("models/b.rs.golden", "old\n");
        tree.write("runtime.rs.golden", "unchanged\n");
        let generated = files(&[
            ("runtime.rs", "unchanged\n"),
            ("models/b.rs", "new\n"),
            ("models/a.rs", "added\n"),
        ]);

        let report = compare(&tree.0, &generated).unwrap().unwrap();
        let statuses: Vec<_> = report
            .lines()
            .filter(|line| {
                ["Added:", "Changed:", "Removed ("]
                    .into_iter()
                    .any(|prefix| line.starts_with(prefix))
            })
            .collect();
        assert_eq!(
            statuses,
            [
                "Added: models/a.rs",
                "Changed: models/b.rs",
                "Removed (stale snapshot): models/c.rs",
            ]
        );
        assert!(report.contains("--- golden/models/b.rs.golden\n+++ generated/models/b.rs"));
        assert!(report.contains("-old\n+new\n"));
        assert!(!report.contains("runtime.rs"));
    }

    #[test]
    fn exact_line_endings_and_final_newline_matter() {
        let tree = TempDir::new();
        tree.write("README.md.golden", "one\r\ntwo");
        assert_eq!(
            compare(&tree.0, &files(&[("README.md", "one\r\ntwo")])).unwrap(),
            None
        );
        for bytes in ["one\ntwo", "one\r\ntwo\n"] {
            let report = compare(&tree.0, &files(&[("README.md", bytes)]))
                .unwrap()
                .unwrap();
            assert!(report.contains("Changed: README.md"));
        }
    }

    #[test]
    fn foreign_files_and_non_utf8_snapshots_are_errors() {
        let tree = TempDir::new();
        tree.write("foreign.txt", "not a snapshot");
        let error = compare(&tree.0, &HashMap::new()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("foreign.txt"));

        let tree = TempDir::new();
        tree.write("binary.golden", [0xff]);
        let error = compare(&tree.0, &HashMap::new()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("binary.golden"));
    }

    #[test]
    fn a_missing_case_directory_is_not_an_empty_snapshot() {
        let tree = TempDir::new();
        let error = compare(&tree.0.join("missing"), &HashMap::new()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(error.to_string().contains("missing"));
    }

    #[cfg(unix)]
    #[test]
    fn literal_backslash_does_not_alias_a_nested_snapshot() {
        let tree = TempDir::new();
        tree.write("a/b.golden", "nested");
        tree.write(r"a\b.golden", "literal backslash");
        assert_eq!(
            compare(
                &tree.0,
                &files(&[("a/b", "nested"), (r"a\b", "literal backslash")])
            )
            .unwrap(),
            None
        );
        let report = compare(&tree.0, &files(&[("a/b", "nested")]))
            .unwrap()
            .unwrap();
        assert_eq!(report, "Removed (stale snapshot): a\\b\n");
    }

    #[cfg(unix)]
    #[test]
    fn neither_case_nor_entry_symlinks_are_followed() {
        use std::os::unix::fs::symlink;

        let tree = TempDir::new();
        tree.write("target/normal.golden", "snapshot");
        symlink(tree.0.join("target"), tree.0.join("case-link")).unwrap();
        let error = compare(&tree.0.join("case-link"), &HashMap::new()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("case-link"));

        let case = tree.0.join("target");
        symlink(case.join("normal.golden"), case.join("entry-link.golden")).unwrap();
        let error = compare(&case, &HashMap::new()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("entry-link.golden"));
    }

    // Linux filesystems can hold byte names that macOS APFS rejects at create time.
    #[cfg(target_os = "linux")]
    #[test]
    fn non_utf8_paths_are_errors_instead_of_lossy_keys() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let tree = TempDir::new();
        tree.write(OsStr::from_bytes(b"invalid-\xff.golden"), "snapshot");
        let error = compare(&tree.0, &HashMap::new()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("golden path is not UTF-8"));
    }
}
