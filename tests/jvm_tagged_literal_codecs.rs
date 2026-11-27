use openapi_nexus::{
    codegen::traits::code_generator::CodeGenerator,
    generators::{
        java::okhttp::JavaOkhttpCodeGenerator, kotlin::okhttp::KotlinOkhttpCodeGenerator,
    },
    ir::lower::lower,
    parser::parse_content_yaml,
};

#[test]
fn native_jvm_adapters_keep_the_wire_lookup_and_literal_payload() {
    let input = include_str!("fixtures/valid/jvm-tagged-literal-codecs.yaml");
    let ir = lower(parse_content_yaml(input).unwrap()).unwrap();
    let java = JavaOkhttpCodeGenerator::new(toml::Table::new());
    let kotlin = KotlinOkhttpCodeGenerator::new(toml::Table::new());
    for (extension, files) in [
        ("java", java.generate(&ir).unwrap()),
        ("kt", kotlin.generate(&ir).unwrap()),
    ] {
        let model = |name| {
            let filename = format!("{name}.{extension}");
            &files
                .iter()
                .find(|file| file.filename == filename)
                .unwrap_or_else(|| panic!("missing {filename}"))
                .content
        };
        for name in ["InternalChoice", "AdjacentChoice"] {
            assert!(model(name).contains("isString"), "{name}.{extension}");
        }
        let mapped = model("MappedChoice");
        assert!(mapped.contains("isJsonPrimitive"), "{extension}");
        assert!(!mapped.contains("isString"), "{extension}");
        let external = model("ExternalChoice");
        assert!(
            external.contains("JsonPrimitive(\"format_v1\")"),
            "{extension}"
        );
        assert!(
            external.contains(".add(\"plain\", gson.toJsonTree("),
            "{extension}"
        );
    }
}
