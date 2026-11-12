use openapi_nexus::codegen::traits::code_generator::CodeGenerator;
use openapi_nexus::generators::go::http::GoHttpCodeGenerator;
use openapi_nexus::ir::{
    lower::lower,
    types::{IrSchemaKind, IrTypeExpr},
};
use openapi_nexus::parser::parse_content_yaml;

#[test]
fn object_map_composition_retains_members_without_anonymous_map_embedding() {
    let source = include_str!("fixtures/valid/go-allof-composition.yaml");
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let ir = lower(parse_content_yaml(&source.replacen("3.1.0", version, 1)).unwrap()).unwrap();
        let IrSchemaKind::Intersection(intersection) = &ir.schemas["OpenAllOf"].kind else {
            panic!("allOf retained")
        };
        assert!(matches!(&intersection.members[1], IrTypeExpr::Map(_)));
        let files = GoHttpCodeGenerator::new(Default::default())
            .generate(&ir)
            .unwrap();
        let model = &files
            .iter()
            .find(|f| f.filename == "open_all_of.go")
            .unwrap()
            .content;
        assert!(
            model.contains("AdditionalProperties map[string]any"),
            "{model}"
        );
        assert!(model.contains("MarshalJSON()"), "{model}");
        assert!(model.contains("UnmarshalJSON("), "{model}");
        assert!(!model.contains("\n\tmap["), "{model}");
    }
}

#[test]
fn raw_policy_and_incompatible_shapes_keep_json_without_changing_ordinary_models() {
    let source = include_str!("fixtures/valid/go-allof-composition.yaml");
    let ir = lower(parse_content_yaml(source).unwrap()).unwrap();
    let composed = GoHttpCodeGenerator::new(Default::default())
        .generate(&ir)
        .unwrap();
    let raw = GoHttpCodeGenerator::new(toml::from_str("all_of_strategy = 'raw'").unwrap())
        .generate(&ir)
        .unwrap();
    let content = |files: &[openapi_nexus::codegen::traits::file_writer::FileInfo], name: &str| {
        files
            .iter()
            .find(|file| file.filename == name)
            .unwrap()
            .content
            .clone()
    };
    assert_eq!(content(&composed, "base.go"), content(&raw, "base.go"));
    assert!(content(&raw, "open_all_of.go").contains("type OpenAllOf = json.RawMessage"));
    for name in ["scalar.go", "array_value.go", "cycle.go", "incompatible.go"] {
        assert!(content(&composed, name).contains("= json.RawMessage"));
    }
}

#[test]
fn repeated_composition_dags_share_metadata_instead_of_expanding_every_path() {
    let mut source = "openapi: 3.1.0\ninfo: {title: Repeated composition, version: '1'}\npaths: {}\ncomponents:\n  schemas:\n    Root:\n      type: object\n      properties:\n        wire_value: {type: string}\n".to_string();
    let mut previous = "Root".to_string();
    for depth in 0..22 {
        let name = format!("Level{depth}");
        source.push_str(&format!("    {name}:\n      allOf: [{{$ref: '#/components/schemas/{previous}'}}, {{$ref: '#/components/schemas/{previous}'}}]\n"));
        previous = name;
    }
    let ir = lower(parse_content_yaml(&source).unwrap()).unwrap();
    let files = GoHttpCodeGenerator::new(Default::default())
        .generate(&ir)
        .unwrap();
    let outer = &files
        .iter()
        .find(|file| file.filename == "level21.go")
        .unwrap()
        .content;
    assert!(outer.contains("Level20"));
    assert!(outer.contains("MarshalJSON()"));
    assert!(!outer.contains("= json.RawMessage"));
}

#[test]
fn nullable_compositions_use_raw_json_including_nested_and_aliased_members() {
    let source = include_str!("fixtures/valid/go-allof-nullable.yaml");
    for version in ["3.1.0", "3.2.0"] {
        let ir = lower(parse_content_yaml(&source.replacen("3.1.0", version, 1)).unwrap()).unwrap();
        let files = GoHttpCodeGenerator::new(Default::default())
            .generate(&ir)
            .unwrap();
        for name in [
            "inline_nullable.go",
            "nullable_map.go",
            "double_nullable.go",
            "nested_nullable.go",
            "alias_composition.go",
        ] {
            let model = &files
                .iter()
                .find(|file| file.filename == name)
                .unwrap()
                .content;
            assert!(
                model.contains("= json.RawMessage"),
                "{version} {name}: {model}"
            );
        }
    }
}
