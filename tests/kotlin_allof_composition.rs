use openapi_nexus::codegen::traits::code_generator::CodeGenerator;
use openapi_nexus::generators::kotlin::okhttp::KotlinOkhttpCodeGenerator;
use openapi_nexus::ir::lower::lower;
use openapi_nexus::parser::parse_content_yaml;

const FIXTURE: &str = include_str!("fixtures/valid/object-composition.yaml");

fn files(
    version: &str,
    config: toml::value::Table,
) -> Vec<openapi_nexus::codegen::traits::file_writer::FileInfo> {
    let source = FIXTURE.replacen("3.1.0", version, 1);
    let ir = lower(parse_content_yaml(&source).unwrap()).unwrap();
    KotlinOkhttpCodeGenerator::new(config)
        .generate(&ir)
        .unwrap()
}

fn content<'a>(
    files: &'a [openapi_nexus::codegen::traits::file_writer::FileInfo],
    name: &str,
) -> &'a str {
    &files
        .iter()
        .find(|file| file.filename == name)
        .unwrap()
        .content
}

#[test]
fn composed_canonical_models_preserve_member_entrypoints_across_openapi_versions() {
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let generated = files(version, Default::default());
        let alias = content(&generated, "MixedAlias.kt");
        assert!(
            alias.contains("@com.google.gson.annotations.JsonAdapter"),
            "{alias}"
        );
        assert!(
            alias.contains("val mixedMap: com.example.sdk.models.MixedMap"),
            "{alias}"
        );
        assert!(alias.contains("MixedAliasAllOfAdapterFactory"), "{alias}");

        let open = content(&generated, "OpenAllOf.kt");
        assert!(
            open.contains("val opaqueMap: com.example.sdk.models.OpaqueMap"),
            "{open}"
        );
        assert!(open.contains("val mapStringAny:"), "{open}");
        assert!(
            generated
                .iter()
                .any(|file| file.filename == "AllOfAdapterFactory.kt")
        );
    }
}

#[test]
fn raw_policy_changes_only_the_canonical_intersections() {
    let composed = files("3.1.0", Default::default());
    let raw = files("3.1.0", toml::from_str("all_of_strategy = 'raw'").unwrap());
    for name in [
        "MixedMap.kt",
        "OpaqueMap.kt",
        "NestedValue.kt",
        "AllOfAdapterFactory.kt",
    ] {
        assert_eq!(content(&composed, name), content(&raw, name), "{name}");
    }
    assert!(content(&raw, "MixedAlias.kt").contains("val value: com.google.gson.JsonElement"));
    assert!(content(&raw, "OpenAllOf.kt").contains("val value: com.google.gson.JsonElement"));
}

#[test]
fn repeated_composition_graphs_cache_and_deduplicate_object_facts() {
    let mut source = "openapi: 3.1.0\ninfo: {title: Repeated composition, version: '1'}\npaths: {}\ncomponents:\n  schemas:\n    Root:\n      type: object\n      properties:\n        wire_value: {type: string}\n".to_string();
    let mut previous = "Root".to_string();
    for depth in 0..22 {
        let name = format!("Level{depth}");
        source.push_str(&format!("    {name}:\n      allOf: [{{$ref: '#/components/schemas/{previous}'}}, {{$ref: '#/components/schemas/{previous}'}}]\n"));
        previous = name;
    }
    let ir = lower(parse_content_yaml(&source).unwrap()).unwrap();
    let generated = KotlinOkhttpCodeGenerator::new(Default::default())
        .generate(&ir)
        .unwrap();
    let model = content(&generated, "Level21.kt");
    assert!(model.contains("val level20:"), "{model}");
    assert!(!model.contains("val value:"), "{model}");
}
