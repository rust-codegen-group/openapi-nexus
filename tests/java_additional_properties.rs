use openapi_nexus::codegen::traits::code_generator::CodeGenerator;
use openapi_nexus::generators::java::okhttp::JavaOkhttpCodeGenerator;
use openapi_nexus::ir::{lower::lower, types::IrSchemaKind};
use openapi_nexus::parser::parse_content_yaml;

#[test]
fn explicit_additional_values_retain_ir_facts_and_get_a_flat_gson_codec() {
    let source = "openapi: 3.1.0\ninfo: {title: Extensions, version: '1'}\npaths: {}\ncomponents:\n  schemas:\n    Pure: {type: object, additionalProperties: {type: string}}\n    Mixed:\n      type: object\n      properties:\n        known_field: {type: string}\n      additionalProperties: true\n";
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let ir = lower(parse_content_yaml(&source.replacen("3.1.0", version, 1)).unwrap()).unwrap();
        for name in ["Pure", "Mixed"] {
            let IrSchemaKind::Object(object) = &ir.schemas[name].kind else {
                panic!("explicit additional properties retain object facts");
            };
            assert!(object.additional_properties.is_some());
        }
        let files = JavaOkhttpCodeGenerator::new(Default::default())
            .generate(&ir)
            .unwrap();
        for name in ["Pure.java", "Mixed.java"] {
            let model = files.iter().find(|file| file.filename == name).unwrap();
            assert!(model.content.contains("getAdditionalProperties()"));
            assert!(model.content.contains("private transient java.util.Map"));
            assert!(
                model
                    .content
                    .contains("@com.google.gson.annotations.JsonAdapter")
            );
        }
        assert!(
            files
                .iter()
                .any(|file| file.filename == "AdditionalPropertiesAdapterFactory.java")
        );
    }
}
