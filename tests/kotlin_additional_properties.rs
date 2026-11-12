use openapi_nexus::codegen::traits::code_generator::CodeGenerator;
use openapi_nexus::generators::kotlin::okhttp::KotlinOkhttpCodeGenerator;
use openapi_nexus::ir::{lower::lower, types::IrSchemaKind};
use openapi_nexus::parser::parse_content_yaml;

#[test]
fn mixed_extension_facts_get_storage_without_replacing_pure_dictionary_codecs() {
    let source = "openapi: 3.1.0\ninfo: {title: Extensions, version: '1'}\npaths: {}\ncomponents:\n  schemas:\n    Mixed:\n      type: object\n      required: [known_field]\n      properties:\n        known_field: {type: string}\n      additionalProperties: {type: integer}\n    Pure: {type: object, additionalProperties: {type: string}}\n";
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let ir = lower(parse_content_yaml(&source.replacen("3.1.0", version, 1)).unwrap()).unwrap();
        let IrSchemaKind::Object(object) = &ir.schemas["Mixed"].kind else {
            panic!("mixed object facts retained");
        };
        assert!(object.additional_properties.is_some());
        let files = KotlinOkhttpCodeGenerator::new(Default::default())
            .generate(&ir)
            .unwrap();
        let mixed = &files
            .iter()
            .find(|file| file.filename == "Mixed.kt")
            .unwrap()
            .content;
        assert!(mixed.contains("var additionalProperties:"));
        assert!(mixed.contains("@field:kotlin.jvm.Transient"));
        assert!(mixed.contains("constructor(knownField:"));
        let pure = &files
            .iter()
            .find(|file| file.filename == "Pure.kt")
            .unwrap()
            .content;
        assert!(pure.contains("val additionalProperties:"));
        assert!(!pure.contains("AdditionalPropertiesAdapterFactory"));
    }
}
