//! Stage B: prove `sigil_emit::generate_model_files` handles Union + Intersection.
//!
//! Union covers primitive members with `| null`, and named object members whose
//! imports must be tracked. Intersection covers `allOf` over named refs.

use std::fs;
use std::path::Path;

use openapi_nexus_typescript::sigil_emit;

fn read_fixture(rel: &str) -> String {
    for base in ["tests/fixtures", "../tests/fixtures", "../../tests/fixtures"] {
        let p = Path::new(base).join(rel);
        if p.exists() {
            return fs::read_to_string(p).unwrap();
        }
    }
    panic!("fixture not found: {rel}");
}

fn render_models(fixture: &str) -> Vec<(String, String)> {
    let yaml = read_fixture(fixture);
    let parsed = openapi_nexus_parser::parse_content_yaml(&yaml).unwrap();
    let ir = openapi_nexus_ir::lower::lower(parsed).unwrap();
    sigil_emit::generate_model_files(&ir)
        .expect("all kinds in fixture should be supported")
        .into_iter()
        .map(|f| (f.filename, f.content))
        .collect()
}

fn find<'a>(files: &'a [(String, String)], filename: &str) -> &'a str {
    files
        .iter()
        .find(|(n, _)| n == filename)
        .map(|(_, c)| c.as_str())
        .unwrap_or_else(|| {
            panic!(
                "expected file {filename} in: {:?}",
                files.iter().map(|(n, _)| n).collect::<Vec<_>>()
            )
        })
}

#[test]
fn union_of_primitives_with_null() {
    let files = render_models("valid/type-aliases/union-with-primitives.yaml");
    let foo = find(&files, "Foo.ts");
    assert!(
        foo.contains("export type Foo = string | number | boolean | null;"),
        "expected primitive+null union, got:\n{foo}"
    );
    assert!(
        !foo.contains("import "),
        "primitive union should have no imports:\n{foo}"
    );
}

#[test]
fn union_of_named_interfaces_imports_each_member() {
    let files = render_models("valid/type-aliases/union-with-interfaces.yaml");
    let foo = find(&files, "Foo.ts");

    assert!(
        foo.contains("export type Foo = Bar | Baz | Qux;"),
        "expected named union, got:\n{foo}"
    );
    for member in ["Bar", "Baz", "Qux"] {
        let want = format!("import type {{ {member} }} from './{member}';");
        assert!(
            foo.contains(&want),
            "expected `{want}` in:\n{foo}"
        );
    }
}

#[test]
fn intersection_of_named_interfaces_imports_each_member() {
    let files = render_models("valid/type-aliases/intersection-allof.yaml");
    let foo = find(&files, "Foo.ts");

    assert!(
        foo.contains("export type Foo = Bar & Baz;"),
        "expected intersection, got:\n{foo}"
    );
    for member in ["Bar", "Baz"] {
        let want = format!("import type {{ {member} }} from './{member}';");
        assert!(foo.contains(&want), "expected `{want}` in:\n{foo}");
    }
}
