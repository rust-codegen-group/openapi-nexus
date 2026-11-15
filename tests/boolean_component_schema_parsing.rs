use openapi_nexus::ParsedSpec;
use openapi_nexus::ir::{
    lower::lower,
    types::{IrPrimitive, IrSchemaKind, IrTypeExpr},
};
use openapi_nexus::parser::{
    parse_content_json, parse_content_json_v31, parse_content_yaml, parse_content_yaml_v31,
};
use serde_json::{Value, json};

const FIXTURE: &str = include_str!("fixtures/valid/boolean-component-schemas.yaml");

fn serialized_spec(parsed: &ParsedSpec) -> Value {
    match parsed {
        ParsedSpec::V30(spec) => serde_json::to_value(spec),
        ParsedSpec::V31(spec) => serde_json::to_value(spec),
        ParsedSpec::V32(spec) => serde_json::to_value(spec),
    }
    .expect("spec serializes")
}

fn fixture(version: &str) -> Value {
    let mut document: Value = serde_norway::from_str(FIXTURE).expect("fixture is valid YAML");
    document["openapi"] = json!(version);
    document
}

fn parse_both_formats(document: &Value) -> [ParsedSpec; 2] {
    [
        parse_content_json(&serde_json::to_string(document).unwrap()).expect("JSON spec parses"),
        parse_content_yaml(&serde_norway::to_string(document).unwrap()).expect("YAML spec parses"),
    ]
}

#[test]
fn boolean_components_round_trip_without_changing_schema_shape() {
    for version in ["3.1.0", "3.2.0"] {
        let document = fixture(version);
        for parsed in parse_both_formats(&document) {
            let serialized = serialized_spec(&parsed);
            assert_eq!(serialized["openapi"], json!(version));
            assert_eq!(serialized["components"], document["components"]);

            // Both output encodings must continue to accept and preserve false,
            // rather than replacing either boolean with an empty object schema.
            let json = serde_json::to_string(&serialized).unwrap();
            let yaml = serde_norway::to_string(&serialized).unwrap();
            assert_eq!(
                serialized_spec(&parse_content_json(&json).unwrap())["components"],
                document["components"]
            );
            assert_eq!(
                serialized_spec(&parse_content_yaml(&yaml).unwrap())["components"],
                document["components"]
            );
        }
    }
}

#[test]
fn version_specific_v31_entrypoints_preserve_boolean_components() {
    let expected = fixture("3.1.0");
    let yaml = parse_content_yaml_v31(FIXTURE).expect("explicit v31 YAML parser accepts booleans");
    let json = parse_content_json_v31(&expected.to_string())
        .expect("explicit v31 JSON parser accepts booleans");
    assert_eq!(
        serde_json::to_value(yaml).unwrap()["components"],
        expected["components"]
    );
    assert_eq!(
        serde_json::to_value(json).unwrap()["components"],
        expected["components"]
    );
}

macro_rules! assert_schema_resolution {
    ($version:ident, $spec:expr) => {{
        use openapi_nexus::spec::$version::spec::{
            BooleanSchema, ErrorRef, FromRef, ObjectOrReference, ObjectSchema, Schema,
            SchemaType, SchemaTypeSet,
        };

        let spec = $spec;
        let schemas = &spec.components.as_ref().unwrap().schemas;
        for (name, value) in [
            ("Anything", true),
            ("Nothing", false),
            ("AnythingAlias", true),
            ("NothingAlias", false),
            ("AliasChain", true),
        ] {
            let path = format!("#/components/schemas/{name}");
            let expected = Schema::Boolean(BooleanSchema(value));
            assert_eq!(schemas[name].resolve(spec).unwrap(), expected);
            assert_eq!(Schema::from_ref(spec, &path).unwrap(), expected);
            assert!(matches!(
                ObjectSchema::from_ref(spec, &path),
                Err(ErrorRef::BooleanSchema { path: error_path, value: error_value })
                    if error_path == path && error_value == value
            ));
        }
        assert!(schemas["Anything"].as_object().is_none());
        assert!(schemas["Nothing"].as_object().is_none());
        assert!(matches!(
            schemas["AnythingAlias"].as_object(),
            Some(ObjectOrReference::Ref { ref_path, .. })
                if ref_path == "#/components/schemas/Anything"
        ));
        let flag = ObjectSchema::from_ref(spec, "#/components/schemas/FlagAlias").unwrap();
        assert_eq!(flag.schema_type, Some(SchemaTypeSet::Single(SchemaType::Boolean)));
        assert_eq!(
            schemas["FlagAlias"].resolve(spec).unwrap(),
            schemas["Flag"]
        );
        assert_eq!(
            ObjectSchema::from_ref(spec, "#/components/schemas/Empty").unwrap(),
            ObjectSchema::default()
        );
        assert!(matches!(
            Schema::from_ref(spec, "#/components/schemas/Missing"),
            Err(ErrorRef::Unresolvable { .. })
        ));
        assert!(matches!(
            Schema::from_ref(spec, "#/components/responses/ResponseA"),
            Err(ErrorRef::MismatchedType { .. })
        ));
    }};
}

#[test]
fn schema_references_resolve_boolean_targets_without_fabricating_objects() {
    for version in ["3.1.0", "3.2.0"] {
        for parsed in parse_both_formats(&fixture(version)) {
            match parsed {
                ParsedSpec::V31(spec) => assert_schema_resolution!(oas31, &spec),
                ParsedSpec::V32(spec) => assert_schema_resolution!(oas32, &spec),
                ParsedSpec::V30(_) => unreachable!(),
            }
        }
    }
}

#[test]
fn true_components_lower_to_any_but_boolean_types_and_named_aliases_stay_typed() {
    for version in ["3.1.0", "3.2.0"] {
        let mut document = fixture(version);
        let schemas = document["components"]["schemas"].as_object_mut().unwrap();
        schemas.remove("Nothing");
        schemas.remove("NothingAlias");
        for parsed in parse_both_formats(&document) {
            let ir = lower(parsed).expect("true boolean component lowers");
            assert_eq!(ir.schemas.len(), 6);
            for name in ["Anything", "Empty"] {
                assert!(matches!(
                    ir.schemas[name].kind,
                    IrSchemaKind::Alias(IrTypeExpr::Any)
                ));
            }
            assert!(matches!(
                ir.schemas["Flag"].kind,
                IrSchemaKind::Alias(IrTypeExpr::Primitive(IrPrimitive::Boolean))
            ));
            for (name, target) in [
                ("AnythingAlias", "Anything"),
                ("AliasChain", "AnythingAlias"),
                ("FlagAlias", "Flag"),
            ] {
                assert!(matches!(
                    &ir.schemas[name].kind,
                    IrSchemaKind::Alias(IrTypeExpr::Named(actual)) if actual == target
                ));
            }
            assert!(ir.schemas.values().all(|schema| schema.is_component));
        }
    }
}

fn fixture_with_unused_false(version: &str) -> Value {
    let mut document: Value =
        serde_norway::from_str(include_str!("fixtures/valid/unused-boolean-component.yaml"))
            .unwrap();
    document["openapi"] = json!(version);
    document
}

fn assert_false_reference_rejected(document: &Value) {
    for parsed in parse_both_formats(document) {
        let error =
            lower(parsed).expect_err("a referenced false component needs an uninhabited type");
        assert_eq!(
            error.to_string(),
            "lowering error: boolean component schema `Nothing` is false (rejects every value); the IR cannot represent an uninhabited type"
        );
    }
}

#[test]
fn unused_false_components_do_not_block_operations() {
    for version in ["3.1.0", "3.2.0"] {
        for parsed in parse_both_formats(&fixture_with_unused_false(version)) {
            let ir = lower(parsed).expect("unused false components do not prevent a usable client");
            assert_eq!(ir.operations.len(), 1);
            assert_eq!(ir.operations[0].path, "/values");
            assert!(!ir.schemas.contains_key("Nothing"));
        }
    }
}

#[test]
fn retained_models_cannot_reference_false_components() {
    let reference = json!({"$ref": "#/components/schemas/Nothing"});
    for version in ["3.1.0", "3.2.0"] {
        for model in [
            reference.clone(),
            json!({"type": "object", "properties": {"value": reference}}),
            json!({"type": ["array", "null"], "items": reference}),
            json!({"type": "object", "additionalProperties": reference}),
            json!({"allOf": [reference]}),
            json!({"anyOf": [reference, {"type": "string"}]}),
            json!({"oneOf": [reference, {"type": "string"}]}),
            json!({
                "oneOf": [reference],
                "discriminator": {
                    "propertyName": "kind",
                    "mapping": {"empty": "#/components/schemas/Nothing"}
                }
            }),
            json!({
                "oneOf": [{
                    "type": "object",
                    "properties": {"payload": {"type": "string"}},
                    "required": ["payload"],
                    "additionalProperties": reference
                }]
            }),
            json!({
                "oneOf": [{
                    "type": "object",
                    "properties": {
                        "kind": {"type": "string", "enum": ["a"], "allOf": [reference]},
                        "data": {"type": "string"}
                    },
                    "required": ["kind", "data"]
                }]
            }),
            json!({
                "type": "object",
                "properties": {"value": {
                    "anyOf": [{"type": "null"}, reference],
                    "enum": [null, 1]
                }}
            }),
            json!({
                "type": "object",
                "properties": {"value": {
                    "type": ["string", "array"],
                    "items": reference
                }}
            }),
        ] {
            let mut document = fixture_with_unused_false(version);
            document["components"]["schemas"]["ModelA"] = model;
            assert_false_reference_rejected(&document);
        }
        let mut document = fixture_with_unused_false(version);
        document["components"]["schemas"]["AliasA"] =
            json!({"$ref": "#/components/schemas/AliasB"});
        document["components"]["schemas"]["AliasB"] = reference.clone();
        assert_false_reference_rejected(&document);
    }
}

#[test]
fn retained_operations_cannot_reference_false_components() {
    let reference = json!({"$ref": "#/components/schemas/Nothing"});
    for version in ["3.1.0", "3.2.0"] {
        for operation in [
            json!({
                "parameters": [{"name": "value", "in": "query", "schema": reference}],
                "responses": {"200": {"description": "A value"}}
            }),
            json!({
                "requestBody": {"content": {"application/json": {"schema": reference}}},
                "responses": {"200": {"description": "A value"}}
            }),
            json!({"responses": {"200": {
                "description": "A value",
                "content": {"application/json": {"schema": reference}}
            }}}),
            json!({"responses": {"200": {
                "description": "A value",
                "headers": {"X-Value": {"schema": reference}}
            }}}),
            json!({"responses": {"200": {
                "description": "A value",
                "content": {"application/json": {"schema": {
                    "type": "object",
                    "properties": {"values": {"type": "array", "items": reference}}
                }}}
            }}}),
        ] {
            let mut document = fixture_with_unused_false(version);
            document["paths"]["/values"]["get"] = operation;
            assert_false_reference_rejected(&document);
        }
        let mut document = fixture_with_unused_false(version);
        document["webhooks"] = json!({"changed": {"post": {"responses": {"200": {
            "description": "A value",
            "content": {"application/json": {"schema": reference}}
        }}}}});
        assert_false_reference_rejected(&document);
    }
}

#[test]
fn consumed_discriminator_constraints_cannot_hide_false_references() {
    let tagged = json!({"oneOf": [{"allOf": [
        {
            "type": "object",
            "properties": {"kind": {"type": "string", "enum": ["a"]}},
            "required": ["kind"]
        },
        {"type": "object", "properties": {"kind": {"$ref": "#/components/schemas/Nothing"}}}
    ]}]});
    for version in ["3.1.0", "3.2.0"] {
        for component in [true, false] {
            let mut document = fixture_with_unused_false(version);
            let response_schema = if component {
                document["components"]["schemas"]["Wrapper"] = tagged.clone();
                json!({"$ref": "#/components/schemas/Wrapper"})
            } else {
                tagged.clone()
            };
            document["paths"]["/values"]["get"]["responses"]["200"]["content"] = json!({
                "application/json": {"schema": response_schema}
            });
            assert_false_reference_rejected(&document);
        }
    }
}

#[test]
fn streaming_items_cannot_reference_false_components() {
    let mut document = fixture_with_unused_false("3.2.0");
    document["paths"]["/values"]["get"]["responses"]["200"]["content"] = json!({
        "text/event-stream": {"itemSchema": {"$ref": "#/components/schemas/Nothing"}}
    });
    assert_false_reference_rejected(&document);
}

#[test]
fn overwritten_parameters_do_not_keep_a_false_component_in_use() {
    for version in ["3.1.0", "3.2.0"] {
        let mut document = fixture_with_unused_false(version);
        document["paths"]["/values"]["parameters"] = json!([
            {"name": "value", "in": "query", "schema": {"$ref": "#/components/schemas/Nothing"}}
        ]);
        document["paths"]["/values"]["get"]["parameters"] = json!([
            {"name": "value", "in": "query", "schema": {"type": "string"}}
        ]);
        for parsed in parse_both_formats(&document) {
            let ir = lower(parsed).expect("only the retained parameter type needs an IR model");
            assert_eq!(ir.operations[0].parameters.len(), 1);
            assert_eq!(
                ir.operations[0].parameters[0].type_expr,
                IrTypeExpr::Primitive(IrPrimitive::String)
            );
            assert!(!ir.schemas.contains_key("Nothing"));
        }
    }
}

#[test]
fn omitted_false_component_names_remain_reserved_for_inline_promotion() {
    for version in ["3.1.0", "3.2.0"] {
        let mut document = fixture_with_unused_false(version);
        document["components"]["schemas"] = json!({"getValueResponse200": false});
        document["paths"]["/values"]["get"]["responses"]["200"]["content"] = json!({
            "application/json": {"schema": {
                "type": "object", "properties": {"value": {"type": "string"}}
            }}
        });
        for parsed in parse_both_formats(&document) {
            let ir = lower(parsed).expect("the omitted component keeps its authored identity");
            assert!(!ir.schemas.contains_key("getValueResponse200"));
            assert!(ir.schemas.contains_key("getValueResponse2002"));
        }
    }
}

#[test]
fn reference_shaped_example_and_extension_data_do_not_use_false_components() {
    for version in ["3.1.0", "3.2.0"] {
        let mut document = fixture_with_unused_false(version);
        document["components"]["schemas"]["Payload"] = json!({
            "type": "object",
            "properties": {"value": {
                "type": "string",
                "default": "#/components/schemas/Nothing",
                "example": {"$ref": "#/components/schemas/Nothing"}
            }},
            "x-data": {"$ref": "#/components/schemas/Nothing"}
        });
        for parsed in parse_both_formats(&document) {
            let ir = lower(parsed).expect("reference-shaped data is not a schema dependency");
            assert!(ir.schemas.contains_key("Payload"));
            assert!(!ir.schemas.contains_key("Nothing"));
        }
    }
}

#[test]
fn all_generators_preserve_output_when_an_unused_false_component_is_present() {
    use std::collections::HashMap;

    use openapi_nexus::generators::{
        go::http::GoHttpCodeGenerator,
        java::okhttp::JavaOkhttpCodeGenerator,
        kotlin::okhttp::KotlinOkhttpCodeGenerator,
        python::{httpx::PythonHttpxCodeGenerator, requests::PythonRequestsCodeGenerator},
        rust::{
            aioduct::RustAioductCodeGenerator, reqwest::RustReqwestCodeGenerator,
            ureq::RustUreqCodeGenerator,
        },
        typescript::fetch::TypeScriptFetchCodeGenerator,
    };
    use openapi_nexus::{CodeGenerator, FileCategory, IrSpec};

    fn rendered(
        generator: &dyn CodeGenerator,
        ir: &IrSpec,
    ) -> HashMap<(FileCategory, String), String> {
        generator
            .generate(ir)
            .unwrap()
            .into_iter()
            .map(|file| ((file.category, file.filename), file.content))
            .collect()
    }

    let generators: Vec<Box<dyn CodeGenerator>> = vec![
        Box::new(TypeScriptFetchCodeGenerator::new(Default::default())),
        Box::new(GoHttpCodeGenerator::new(Default::default())),
        Box::new(RustReqwestCodeGenerator::new(Default::default())),
        Box::new(RustUreqCodeGenerator::new(Default::default())),
        Box::new(RustAioductCodeGenerator::new(Default::default())),
        Box::new(PythonHttpxCodeGenerator::new(Default::default())),
        Box::new(PythonRequestsCodeGenerator::new(Default::default())),
        Box::new(JavaOkhttpCodeGenerator::new(Default::default())),
        Box::new(KotlinOkhttpCodeGenerator::new(Default::default())),
    ];
    for version in ["3.1.0", "3.2.0"] {
        let document = fixture_with_unused_false(version);
        let mut control = document.clone();
        control["components"]["schemas"] = json!({});
        for (parsed, expected) in parse_both_formats(&document)
            .into_iter()
            .zip(parse_both_formats(&control))
        {
            let ir = lower(parsed).unwrap();
            let expected = lower(expected).unwrap();
            for generator in &generators {
                let actual = rendered(generator.as_ref(), &ir);
                assert!(!actual.is_empty());
                assert_eq!(
                    actual,
                    rendered(generator.as_ref(), &expected),
                    "{version}: {}",
                    generator.generator_type()
                );
            }
        }
    }
}

#[test]
fn the_first_dependency_still_determines_false_reference_diagnostics() {
    for version in ["3.1.0", "3.2.0"] {
        for from_model in [true, false] {
            let mut document = fixture_with_unused_false(version);
            document["components"]["schemas"] = json!({"Alpha": false, "Zulu": false});
            if from_model {
                document["components"]["schemas"]["Wrapper"] = json!({"allOf": [
                    {"$ref": "#/components/schemas/Zulu"},
                    {"$ref": "#/components/schemas/Alpha"}
                ]});
            } else {
                document["paths"]["/values"]["get"]["parameters"] = json!([
                    {"name": "value", "in": "query", "schema": {"$ref": "#/components/schemas/Zulu"}}
                ]);
                document["paths"]["/values"]["get"]["requestBody"] = json!({
                    "content": {"application/json": {"schema": {"$ref": "#/components/schemas/Alpha"}}}
                });
            }
            for parsed in parse_both_formats(&document) {
                let error =
                    lower(parsed).expect_err("the first unsupported dependency is reported");
                assert_eq!(
                    error.to_string(),
                    "lowering error: boolean component schema `Zulu` is false (rejects every value); the IR cannot represent an uninhabited type"
                );
            }
        }
    }
}

#[test]
fn invalid_component_scalars_and_non_schema_booleans_remain_rejected() {
    for version in ["3.1.0", "3.2.0"] {
        for invalid in [Value::Null, json!(1), json!("true"), json!([])] {
            let mut document = fixture(version);
            document["components"]["schemas"] = json!({"Invalid": invalid});
            assert!(parse_content_json(&document.to_string()).is_err());
            assert!(parse_content_yaml(&serde_norway::to_string(&document).unwrap()).is_err());
        }
        for value in [true, false] {
            let mut document = fixture(version);
            document["components"]["responses"] = json!({"ResponseA": value});
            assert!(parse_content_json(&document.to_string()).is_err());
            assert!(parse_content_yaml(&serde_norway::to_string(&document).unwrap()).is_err());
        }
    }
}

#[test]
fn openapi_v30_does_not_gain_boolean_component_schemas() {
    for value in [true, false] {
        let mut document = fixture("3.0.3");
        document["components"]["schemas"] = json!({"Anything": value});
        assert!(parse_content_json(&document.to_string()).is_err());
        assert!(parse_content_yaml(&serde_norway::to_string(&document).unwrap()).is_err());
    }
}

#[test]
fn boolean_component_schema_true_parses_from_yaml() {
    let parsed = parse_content_yaml(
        r#"
openapi: 3.1.0
info: {title: Boolean component schema, version: '1'}
paths: {}
components:
  schemas:
    Anything: true
"#,
    )
    .expect("OpenAPI 3.1 accepts a boolean component schema");

    let spec = parsed.as_v31().expect("OpenAPI 3.1 spec");
    let serialized = serde_json::to_value(spec).expect("spec serializes");
    assert_eq!(serialized["components"]["schemas"]["Anything"], json!(true));
}
