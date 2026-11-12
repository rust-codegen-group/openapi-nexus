use openapi_nexus::codegen::traits::code_generator::CodeGenerator;
use openapi_nexus::generators::kotlin::okhttp::KotlinOkhttpCodeGenerator;
use openapi_nexus::ir::{lower::lower, types::IrSchemaKind};
use openapi_nexus::parser::parse_content_yaml;

#[test]
fn explicit_dictionary_models_generate_without_empty_data_classes() {
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let source = format!(
            "openapi: {version}\ninfo: {{title: Dictionary, version: '1'}}\npaths: {{}}\ncomponents:\n  schemas:\n    Labels:\n      type: object\n      additionalProperties: {{type: string}}\n"
        );
        let ir = lower(parse_content_yaml(&source).unwrap()).unwrap();
        let IrSchemaKind::Object(object) = &ir.schemas["Labels"].kind else {
            panic!("named dictionaries retain object facts");
        };
        assert!(object.properties.is_empty());
        assert!(object.additional_properties.is_some());
        let files = KotlinOkhttpCodeGenerator::new(Default::default())
            .generate(&ir)
            .expect("a valid named dictionary must generate");
        let labels = files
            .iter()
            .find(|file| file.filename == "Labels.kt")
            .unwrap();
        assert!(labels.content.contains("class Labels"));
        assert!(!labels.content.contains("data class Labels()"));
        let tokens = labels.content.split_whitespace().collect::<String>();
        assert!(tokens.contains("kotlin.collections.MutableMap<kotlin.String,kotlin.String>"));
    }
}

#[test]
fn dictionary_values_keep_semantic_model_container_and_raw_types() {
    let ir =
        lower(parse_content_yaml(include_str!("fixtures/valid/kotlin-dictionaries.yaml")).unwrap())
            .unwrap();
    let files = KotlinOkhttpCodeGenerator::new(Default::default())
        .generate(&ir)
        .unwrap();
    for (name, value) in [
        ("Values.kt", "com.example.sdk.models.NestedValue"),
        (
            "Lists.kt",
            "kotlin.collections.List<com.example.sdk.models.NestedValue>",
        ),
        (
            "NestedMaps.kt",
            "kotlin.collections.Map<kotlin.String, com.example.sdk.models.NestedValue>",
        ),
        ("NullableValues.kt", "com.example.sdk.models.NestedValue?"),
        ("JsonValues.kt", "JsonElement"),
    ] {
        let file = files.iter().find(|file| file.filename == name).unwrap();
        let tokens = file.content.split_whitespace().collect::<String>();
        let expected = format!("kotlin.collections.MutableMap<kotlin.String, {value}>");
        assert!(
            tokens.contains(&expected.split_whitespace().collect::<String>()),
            "{name}"
        );
    }
}

#[test]
fn dictionary_model_qualification_respects_the_configured_package() {
    let ir = lower(
        parse_content_yaml(include_str!(
            "fixtures/valid/kotlin-dictionary-helper-types.yaml"
        ))
        .unwrap(),
    )
    .unwrap();
    let config = toml::from_str("package_name = 'org.example.dictionary'").unwrap();
    let files = KotlinOkhttpCodeGenerator::new(config)
        .generate(&ir)
        .unwrap();
    for (name, model) in [
        ("TokenValues.kt", "TypeToken"),
        ("ElementValues.kt", "JsonElement"),
        ("NullValues.kt", "JsonNull"),
        ("GsonValues.kt", "Gson"),
    ] {
        let file = files.iter().find(|file| file.filename == name).unwrap();
        let tokens = file.content.split_whitespace().collect::<String>();
        assert!(
            tokens.contains(&format!(
                "kotlin.collections.MutableMap<kotlin.String,org.example.dictionary.models.{model}>"
            )),
            "{name} uses the authored model in the configured package"
        );
        assert!(!file.content.contains("com.example.sdk"));
    }
}
