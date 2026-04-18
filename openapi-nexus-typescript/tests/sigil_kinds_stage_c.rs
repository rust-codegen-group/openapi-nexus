//! Stage C: prove `sigil_emit::generate_model_files` handles TaggedUnion across
//! all three tagging styles (Internal / Adjacent / External).
//!
//! Fixtures available today mostly exercise Internal (the common OpenAPI
//! `oneOf` + discriminator shape). Adjacent and External are derived shapes
//! — if future fixtures land, they can extend this suite.

use std::fs;
use std::path::Path;

use openapi_nexus_typescript::sigil_emit;

fn read_fixture(rel: &str) -> String {
    for base in [
        "tests/fixtures",
        "../tests/fixtures",
        "../../tests/fixtures",
    ] {
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

/// Internal tagging: `({ kind: 'VAL' } & ContentType)` per variant. Variants
/// here are inferred from oneOf-of-allOf pairs — the IR lowerer promotes each
/// variant to its own named schema, so the content type is a Named ref.
#[test]
fn internally_tagged_union_emits_discriminator_intersection() {
    let files = render_models("valid/type-aliases/discriminated-union-internally-tagged.yaml");
    let resource = find(&files, "Resource.ts");

    // Each variant should appear as `({ kind: 'VAL' } & SomeType)`.
    for value in ["RESOURCE_UNSPECIFIED", "RESOURCE_QUICK", "RESOURCE_CUSTOM"] {
        assert!(
            resource.contains(&format!("kind: '{value}'")),
            "expected discriminator literal {value} in:\n{resource}"
        );
    }
    assert!(
        resource.contains("export type Resource ="),
        "expected type alias header, got:\n{resource}"
    );
    // Intersection operator used for Internal style.
    assert!(
        resource.contains(" & "),
        "expected `&` intersection in Internal tagging, got:\n{resource}"
    );
    // Pipe separator between variants.
    assert!(
        resource.contains(" | "),
        "expected `|` between variants, got:\n{resource}"
    );
}

/// Ref-based discriminated union: inferred from oneOf over component refs.
/// Each variant content type is a plain Named ref — simplest import case.
#[test]
fn discriminated_union_with_refs_imports_each_variant() {
    let files = render_models("valid/type-aliases/discriminated-union-with-refs.yaml");
    let container = find(&files, "ContainerImage.ts");

    assert!(
        container.contains("export type ContainerImage ="),
        "expected type alias, got:\n{container}"
    );
    // At least one `{ kind: '...' } & Something` variant should render.
    assert!(
        container.contains(" & "),
        "expected intersection operator, got:\n{container}"
    );
    assert!(
        container.contains("import type"),
        "expected import lines for variant types, got:\n{container}"
    );
}
