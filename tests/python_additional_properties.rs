use openapi_nexus::codegen::traits::code_generator::CodeGenerator;
use openapi_nexus::generators::python::{
    httpx::PythonHttpxCodeGenerator, requests::PythonRequestsCodeGenerator,
};
use openapi_nexus::ir::{lower::lower, types::IrSchemaKind};
use openapi_nexus::parser::parse_content_yaml;

#[test]
fn explicit_extensions_retain_value_facts_and_usable_model_converters() {
    let source = "openapi: 3.1.0\ninfo: {title: Python dictionaries, version: '1'}\npaths: {}\ncomponents:\n  schemas:\n    OpaqueMap: {type: object, additionalProperties: true}\n    MixedMap:\n      type: object\n      properties:\n        known_field: {type: string}\n      additionalProperties: {type: string}\n    TypedMap: {type: object, additionalProperties: {type: string}}\n";
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let ir = lower(parse_content_yaml(&source.replacen("3.1.0", version, 1)).unwrap()).unwrap();
        for name in ["OpaqueMap", "MixedMap", "TypedMap"] {
            let IrSchemaKind::Object(object) = &ir.schemas[name].kind else {
                panic!("explicit additional properties retain object facts");
            };
            assert!(object.additional_properties.is_some());
        }
        for generator in [
            Box::new(PythonHttpxCodeGenerator::new(Default::default())) as Box<dyn CodeGenerator>,
            Box::new(PythonRequestsCodeGenerator::new(Default::default())),
        ] {
            let files = generator.generate(&ir).unwrap();
            for name in ["opaque_map.py", "mixed_map.py", "typed_map.py"] {
                let file = files.iter().find(|file| file.filename == name).unwrap();
                assert!(file.content.contains("additional_properties:"), "{name}");
                assert!(file.content.contains("def from_dict("), "{name}");
                assert!(file.content.contains("def to_dict("), "{name}");
            }
        }
    }
}
