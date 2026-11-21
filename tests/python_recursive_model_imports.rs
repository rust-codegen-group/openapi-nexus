use openapi_nexus::codegen::traits::{CodeGenerator, FileCategory};
use openapi_nexus::generators::python::httpx::PythonHttpxCodeGenerator;
use openapi_nexus::generators::python::httpx::emit_models::python_type_name as httpx_python_type_name;
use openapi_nexus::generators::python::requests::PythonRequestsCodeGenerator;
use openapi_nexus::generators::python::requests::emit_models::python_type_name as requests_python_type_name;
use openapi_nexus::ir::types::{IrPrimitive, IrTypeExpr};

#[test]
fn python_type_name_helpers_remain_public() {
    let expr = IrTypeExpr::Primitive(IrPrimitive::String);

    let _ = httpx_python_type_name(&expr);
    let _ = requests_python_type_name(&expr);
}

#[test]
fn python_generators_keep_self_references_local() {
    let parsed = openapi_nexus::parser::parse_content_yaml(include_str!(
        "fixtures/valid/recursive-json/self-referential-object.yaml"
    ))
    .unwrap();
    let ir = openapi_nexus::ir::lower::lower(parsed).unwrap();
    let generators: [(&str, Box<dyn CodeGenerator>); 2] = [
        (
            "httpx",
            Box::new(PythonHttpxCodeGenerator::new(toml::value::Table::new())),
        ),
        (
            "requests",
            Box::new(PythonRequestsCodeGenerator::new(toml::value::Table::new())),
        ),
    ];

    for (backend, generator) in generators {
        let files = generator.generate(&ir).unwrap();
        let foo = files
            .iter()
            .find(|file| file.category == FileCategory::Models && file.filename == "foo.py")
            .unwrap();

        assert!(
            !foo.content.contains("from .foo import Foo"),
            "python-{backend} foo.py must not import its own class:\n{}",
            foo.content
        );
        assert!(
            foo.content.contains("from .bar import Bar"),
            "python-{backend} foo.py must retain imports for other models:\n{}",
            foo.content
        );
        assert!(
            foo.content.contains("children: list[Foo] | None"),
            "python-{backend} must retain the recursive field type:\n{}",
            foo.content
        );
        assert!(
            foo.content
                .contains("Foo.from_dict(cast(dict[str, object], item))")
                && foo
                    .content
                    .contains("for item in cast(list[object], data.get(\"children\"))"),
            "python-{backend} must retain recursive deserialization:\n{}",
            foo.content
        );
    }
}
