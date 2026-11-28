use std::path::PathBuf;
use std::{env, fs};

use openapi_nexus::codegen::WriteError;
use openapi_nexus::codegen::traits::file_writer::{FileInfo, FileWriter};
use openapi_nexus::generators::rust::reqwest::RustReqwestCodeGenerator;
use openapi_nexus::test_utils::generate_files;

struct Writer(Option<&'static str>);

impl FileWriter for Writer {
    fn source_dir(&self) -> Option<&str> {
        self.0
    }
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(case: &str) -> Self {
        let path = env::temp_dir().join(format!("nexus-output-{}-{case}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn writer_rejects_destinations_that_collide_after_category_layout() {
    let scratch = Scratch::new("duplicate");
    let output = scratch.0.join("generated");
    let files = [
        FileInfo::project(
            "README.md".into(),
            "does not belong to a failed write".into(),
        ),
        FileInfo::model("same.rs".into(), "first".into()),
        FileInfo::project("src/models/same.rs".into(), "second".into()),
    ];

    let error = Writer(Some("src"))
        .write_files(&output, &files)
        .unwrap_err();
    assert!(error.to_string().contains("src/models/same.rs"), "{error}");
    assert!(
        !output.exists(),
        "the writer must preflight the whole batch"
    );
}

#[test]
fn writer_refuses_to_replace_a_file_needed_as_a_directory() {
    let scratch = Scratch::new("prefix");
    let output = scratch.0.join("generated");
    let files = [
        FileInfo::project("src".into(), "file".into()),
        FileInfo::project("src-docs".into(), "sorts between src and src/".into()),
        FileInfo::model("example.rs".into(), "needs a directory".into()),
    ];

    let error = Writer(Some("src"))
        .write_files(&output, &files)
        .unwrap_err();
    assert!(
        error.to_string().contains("src/models/example.rs"),
        "{error}"
    );
    assert!(!output.exists());
}

#[test]
fn writer_only_accepts_canonical_relative_paths() {
    let scratch = Scratch::new("invalid");
    let output = scratch.0.join("generated");
    let absolute = scratch.0.join("escaped.txt");
    for path in [
        absolute.to_str().unwrap(),
        "../escaped.txt",
        "a/./b",
        "a//b",
        "a\\b",
        "C:/file",
        "a\0b",
    ] {
        let files = [FileInfo::project(path.into(), "must not be written".into())];
        assert!(
            Writer(None).write_files(&output, &files).is_err(),
            "{path:?}"
        );
        assert!(!output.exists(), "{path:?}");
    }
    let files = [FileInfo::model(
        "good.rs".into(),
        "must not be written".into(),
    )];
    assert!(
        Writer(Some("../escaped"))
            .write_files(&output, &files)
            .is_err()
    );
    assert!(!output.exists());
    assert_eq!(fs::read_dir(&scratch.0).unwrap().count(), 0);
}

#[test]
fn golden_generation_reports_the_same_normalized_model_collision() {
    let fixture = "\
openapi: 3.1.0
info: {title: Duplicate normalized schemas, version: '1'}
paths: {}
components:
  schemas:
    shared-key:
      type: object
      properties:
        first: {type: string}
    shared_key:
      type: object
      properties:
        second: {type: string}
";
    let generator = RustReqwestCodeGenerator::new(Default::default());
    let error = generate_files(&generator, fixture).unwrap_err();
    assert!(
        matches!(
            error.downcast_ref::<WriteError>(),
            Some(WriteError::DuplicateOutputPath { path }) if path == "src/models/shared_key.rs"
        ),
        "{error}"
    );
}

#[test]
fn writer_preserves_category_layout_and_ignores_internal_artifacts() {
    let scratch = Scratch::new("layout");
    let files = [
        FileInfo::runtime("codec.rs".into(), "runtime\n".into()),
        FileInfo::api("nested/client.rs".into(), "api\n".into()),
        FileInfo::model("choice.rs".into(), "model\n".into()),
        FileInfo::readme("README.md".into(), "readme\n".into()),
        FileInfo::project("Cargo.toml".into(), "manifest\n".into()),
        FileInfo::none("unwritten".into(), "not a materialized file".into()),
    ];
    Writer(Some("lib/src"))
        .write_files(&scratch.0, &files)
        .unwrap();
    for (path, bytes) in [
        ("lib/src/runtime/codec.rs", "runtime\n"),
        ("lib/src/apis/nested/client.rs", "api\n"),
        ("lib/src/models/choice.rs", "model\n"),
        ("README.md", "readme\n"),
        ("Cargo.toml", "manifest\n"),
    ] {
        assert_eq!(fs::read(scratch.0.join(path)).unwrap(), bytes.as_bytes());
    }
    assert!(!scratch.0.join("unwritten").exists());
}
