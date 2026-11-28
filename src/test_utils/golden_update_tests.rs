use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::{env, fs, panic};

use super::{golden_update, update_golden_files};

struct TestCase(PathBuf);

impl TestCase {
    fn new(name: &str) -> Self {
        let path = env::temp_dir().join(format!("nexus-golden-{}-{name}", std::process::id()));
        fs::create_dir(&path).unwrap();
        let case = Self(path);
        let snapshots = case.0.join("fixture");
        fs::create_dir(&snapshots).unwrap();
        fs::write(snapshots.join("previous.golden"), "the old snapshot").unwrap();
        case
    }

    fn fixture(&self) -> PathBuf {
        self.0.join("fixture")
    }
}

impl Drop for TestCase {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn assert_previous_snapshot_preserved(fixture: &Path) {
    assert_eq!(
        fs::read_to_string(fixture.join("previous.golden")).unwrap(),
        "the old snapshot"
    );
}

#[test]
fn staging_failure_does_not_change_the_existing_snapshot_tree() {
    let case = TestCase::new("staging-failure");
    let generated = [
        ("a".into(), "would create a.golden".into()),
        (
            "a.golden/nested".into(),
            "would use a.golden as a directory".into(),
        ),
    ]
    .into();

    assert!(panic::catch_unwind(|| update_golden_files(&case.0, "fixture", &generated)).is_err());
    assert_previous_snapshot_preserved(&case.fixture());
    assert_eq!(fs::read_dir(case.fixture()).unwrap().count(), 1);
    assert_eq!(fs::read_dir(&case.0).unwrap().count(), 1);
}

#[test]
fn updating_does_not_delete_an_unowned_file() {
    let case = TestCase::new("unowned-file");
    fs::write(case.fixture().join("notes.txt"), "handwritten notes").unwrap();
    let generated = [("new".into(), "new snapshot".into())].into();

    assert!(panic::catch_unwind(|| update_golden_files(&case.0, "fixture", &generated)).is_err());
    assert_previous_snapshot_preserved(&case.fixture());
    assert_eq!(
        fs::read_to_string(case.fixture().join("notes.txt")).unwrap(),
        "handwritten notes"
    );
    assert!(!case.fixture().join("new.golden").exists());
}

#[test]
fn successful_update_replaces_only_the_selected_case() {
    let case = TestCase::new("success");
    let sibling = case.0.join("sibling.golden");
    fs::write(&sibling, "a different case").unwrap();
    let generated = [("models/new.rs".into(), "byte for byte\n".into())].into();

    update_golden_files(&case.0, "fixture", &generated);
    assert_eq!(
        fs::read(case.fixture().join("models/new.rs.golden")).unwrap(),
        b"byte for byte\n"
    );
    assert!(!case.fixture().join("previous.golden").exists());
    assert_eq!(fs::read_to_string(sibling).unwrap(), "a different case");
    assert_eq!(fs::read_dir(&case.0).unwrap().count(), 2);
}

#[test]
fn only_one_enables_snapshot_writes() {
    assert!(golden_update::requested(Some(OsStr::new("1"))));
    for setting in [None, Some(""), Some("0"), Some("true"), Some(" 1")] {
        assert!(!golden_update::requested(setting.map(OsStr::new)));
    }
}

#[test]
fn new_case_is_published_without_disturbing_existing_cases() {
    let case = TestCase::new("new");
    let generated = [("new.txt".into(), "first snapshot".into())].into();

    update_golden_files(&case.0, "new-case", &generated);
    assert_eq!(
        fs::read_to_string(case.0.join("new-case/new.txt.golden")).unwrap(),
        "first snapshot"
    );
    assert_previous_snapshot_preserved(&case.fixture());
}

#[test]
fn empty_or_escaping_output_is_refused() {
    let case = TestCase::new("invalid-output");
    let generated = [("../new".into(), "outside the case".into())].into();
    assert!(golden_update::replace_tree(&case.0, "fixture", &generated).is_err());
    assert!(golden_update::replace_tree(&case.0, "fixture", &Default::default()).is_err());
    assert_previous_snapshot_preserved(&case.fixture());
    assert_eq!(fs::read_dir(&case.0).unwrap().count(), 1);
}

#[test]
fn a_second_update_does_not_discard_an_existing_staging_area() {
    let case = TestCase::new("interrupted");
    let staging = case.0.join(".fixture.update");
    fs::create_dir(&staging).unwrap();
    fs::write(staging.join("previous"), "recoverable data").unwrap();
    let generated = [("new".into(), "new snapshot".into())].into();

    let error = golden_update::replace_tree(&case.0, "fixture", &generated).unwrap_err();
    assert!(error.to_string().contains(".fixture.update"));
    assert_previous_snapshot_preserved(&case.fixture());
    assert_eq!(
        fs::read_to_string(staging.join("previous")).unwrap(),
        "recoverable data"
    );
}

#[test]
fn selected_file_update_preserves_its_siblings() {
    let case = TestCase::new("selected-file");
    let target = case.0.join("selected.golden");
    golden_update::replace_file(&target, "first").unwrap();
    golden_update::replace_file(&target, "second").unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "second");
    assert_previous_snapshot_preserved(&case.fixture());
    assert_eq!(fs::read_dir(&case.0).unwrap().count(), 2);
}

#[cfg(unix)]
#[test]
fn symlinks_are_never_treated_as_owned_snapshots() {
    let case = TestCase::new("symlink");
    let outside = case.0.join("outside");
    fs::write(&outside, "not a snapshot").unwrap();
    let link = case.fixture().join("link.golden");
    std::os::unix::fs::symlink(&outside, &link).unwrap();

    let generated = [("new".into(), "new snapshot".into())].into();
    assert!(golden_update::replace_tree(&case.0, "fixture", &generated).is_err());
    assert!(golden_update::replace_file(&link, "new snapshot").is_err());
    assert!(link.is_symlink());
    assert_eq!(fs::read_to_string(&outside).unwrap(), "not a snapshot");
    assert_previous_snapshot_preserved(&case.fixture());
}
