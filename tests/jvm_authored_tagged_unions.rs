use openapi_nexus::codegen::traits::{code_generator::CodeGenerator, file_writer::FileInfo};
use openapi_nexus::generators::{
    java::okhttp::JavaOkhttpCodeGenerator, kotlin::okhttp::KotlinOkhttpCodeGenerator,
};

const AUTHORED: &str = include_str!("fixtures/valid/jvm-authored-tagged-union.yaml");
const RECURSIVE: &str = include_str!("fixtures/valid/inferred-union-selection.yaml");
const COMPOSED: &str = include_str!("fixtures/valid/rust-inferred-tagged-wire.yaml");
const FIXED: &str =
    include_str!("fixtures/valid/type-aliases/discriminated-union-mixed-unit-and-allof.yaml");
const EXPLICIT: &str = r##"
openapi: 3.1.0
info: { title: Explicit mapping, version: '1' }
paths: {}
components:
  schemas:
    Left:
      type: object
      properties:
        a: {type: string}
    Right:
      type: object
      properties:
        b: {type: string}
    Example:
      oneOf:
        - {$ref: '#/components/schemas/Left'}
        - {$ref: '#/components/schemas/Right'}
      discriminator:
        propertyName: kind
        mapping:
          a: '#/components/schemas/Left'
          b: '#/components/schemas/Right'
"##;

fn generate(spec: &str, policy: &str) -> [Vec<FileInfo>; 2] {
    let parsed = openapi_nexus::parser::parse_content_yaml(spec).unwrap();
    let ir = openapi_nexus::ir::lower::lower(parsed).unwrap();
    let config: toml::Table =
        toml::from_str(&format!("tagged_union_strategy = '{policy}'")).unwrap();
    [
        JavaOkhttpCodeGenerator::new(config.clone())
            .generate(&ir)
            .unwrap(),
        KotlinOkhttpCodeGenerator::new(config)
            .generate(&ir)
            .unwrap(),
    ]
}

#[test]
fn authored_policy_is_applied_by_both_public_generators() {
    for files in generate(AUTHORED, "strict") {
        let choice = model(&files, "Choice");
        assert!(choice.contains("ambiguous inferred tagged union Choice"));
        assert!(choice.contains("9007199254740993"));
        assert!(choice.contains("inferred discriminator requires an object"));
        let outer = model(&files, "Outer");
        assert!(outer.contains("TypeToken<Inner>"));
    }
    for files in generate(AUTHORED, "raw") {
        let choice = model(&files, "Choice");
        assert!(choice.contains("VariantRaw"));
        assert!(!choice.contains("VariantAlpha"));
        assert!(!choice.contains("ambiguous inferred tagged union"));
    }
}

#[test]
fn composed_and_recursive_wire_shapes_emit_in_both_languages() {
    for files in generate(COMPOSED, "firstMatch") {
        let composed = model(&files, "ComposedTagged");
        assert!(composed.contains("VariantAlphaAlt"));
        assert!(composed.contains("conflicting inferred discriminator in payload"));
        let inline = model(&files, "InlineTagged");
        assert!(inline.contains("VariantInlineTaggedMember1"));
        assert!(inline.contains("\"scope\""));
    }
    for files in generate(RECURSIVE, "strict") {
        let envelope = model(&files, "Envelope");
        assert!(envelope.contains("\"children\""));
        assert!(envelope.contains("IdentityHashMap"));
        assert!(envelope.contains("TypeToken<"));
    }
}

#[test]
fn tagged_raw_does_not_replace_legacy_fixed_or_explicit_codecs() {
    for (fixture, root) in [(FIXED, "Setup"), (EXPLICIT, "Example")] {
        for (first, raw) in generate(fixture, "firstMatch")
            .iter()
            .zip(generate(fixture, "raw"))
        {
            assert_eq!(model(first, root), model(&raw, root));
        }
    }
}

fn model<'a>(files: &'a [FileInfo], name: &str) -> &'a str {
    files
        .iter()
        .find(|file| {
            file.filename
                .split_once('.')
                .is_some_and(|(stem, _)| stem == name)
        })
        .unwrap()
        .content
        .as_str()
}
