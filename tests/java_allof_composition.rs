use openapi_nexus::codegen::traits::code_generator::CodeGenerator;
use openapi_nexus::generators::java::okhttp::JavaOkhttpCodeGenerator;
use openapi_nexus::ir::lower::lower;
use openapi_nexus::parser::parse_content_yaml;

#[test]
fn java_composition_uses_configured_factory_without_removing_member_entrypoints() {
    let source = include_str!("fixtures/valid/java-allof-composition.yaml");
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let ir = lower(parse_content_yaml(&source.replacen("3.1.0", version, 1)).unwrap()).unwrap();
        let files = JavaOkhttpCodeGenerator::new(Default::default())
            .generate(&ir)
            .unwrap();
        let model = &files
            .iter()
            .find(|file| file.filename == "MixedAlias.java")
            .unwrap()
            .content;
        assert!(model.contains("JsonAdapter"), "{model}");
        assert!(model.contains("getMixedMap()"), "{model}");
        assert!(model.contains("MixedAlias("), "{model}");
    }
}

#[test]
fn raw_policy_does_not_change_ordinary_model_output() {
    let ir = lower(
        parse_content_yaml(include_str!("fixtures/valid/java-allof-composition.yaml")).unwrap(),
    )
    .unwrap();
    let composed = JavaOkhttpCodeGenerator::new(Default::default())
        .generate(&ir)
        .unwrap();
    let raw = JavaOkhttpCodeGenerator::new(toml::from_str("all_of_strategy = 'raw'").unwrap())
        .generate(&ir)
        .unwrap();
    for name in [
        "MixedMap.java",
        "OpaqueMap.java",
        "NestedValue.java",
        "WrappedAlias.java",
    ] {
        let left = &composed
            .iter()
            .find(|file| file.filename == name)
            .unwrap()
            .content;
        let right = &raw
            .iter()
            .find(|file| file.filename == name)
            .unwrap()
            .content;
        assert_eq!(left, right, "{name}");
    }
    let model = &raw
        .iter()
        .find(|file| file.filename == "MixedAlias.java")
        .unwrap()
        .content;
    assert!(model.contains("getValue()"));
    assert!(model.contains("com.google.gson.JsonElement"));
}

#[test]
fn repeated_composition_graphs_cache_and_deduplicate_object_facts() {
    let mut source = "openapi: 3.1.0\ninfo: {title: Repeated Java composition, version: '1'}\npaths: {}\ncomponents:\n  schemas:\n    Root:\n      type: object\n      properties:\n        wire_value: {type: string}\n".to_string();
    let mut previous = "Root".to_string();
    for depth in 0..22 {
        let name = format!("Level{depth}");
        source.push_str(&format!("    {name}:\n      allOf: [{{$ref: '#/components/schemas/{previous}'}}, {{$ref: '#/components/schemas/{previous}'}}]\n"));
        previous = name;
    }
    let ir = lower(parse_content_yaml(&source).unwrap()).unwrap();
    let files = JavaOkhttpCodeGenerator::new(Default::default())
        .generate(&ir)
        .unwrap();
    let model = &files
        .iter()
        .find(|file| file.filename == "Level21.java")
        .unwrap()
        .content;
    assert!(model.contains("getLevel20()"));
    assert!(!model.contains("getValue()"));
}
