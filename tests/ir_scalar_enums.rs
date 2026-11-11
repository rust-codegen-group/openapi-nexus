use openapi_nexus::ir::{lower::lower, types::IrSchemaKind};
use openapi_nexus::parser::parse_content_yaml;
use serde_json::json;

use openapi_nexus::codegen::traits::code_generator::CodeGenerator;
use openapi_nexus::generators::{
    go::http::GoHttpCodeGenerator,
    java::okhttp::JavaOkhttpCodeGenerator,
    kotlin::okhttp::KotlinOkhttpCodeGenerator,
    python::{httpx::PythonHttpxCodeGenerator, requests::PythonRequestsCodeGenerator},
    rust::{
        aioduct::RustAioductCodeGenerator, reqwest::RustReqwestCodeGenerator,
        ureq::RustUreqCodeGenerator,
    },
};

#[test]
fn untyped_scalar_enum_values_do_not_inherit_a_synthetic_type_constraint() {
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let source = format!(
            "openapi: {version}\ninfo: {{title: Untyped finite values, version: '1'}}\npaths: {{}}\ncomponents:\n  schemas:\n    Payload:\n      type: object\n      properties:\n        revision: {{enum: [1, 2]}}\n        typed_revision: {{type: integer, enum: [1, 2]}}\n        flags: {{type: array, items: {{enum: [true]}}}}\n        ratios: {{type: object, additionalProperties: {{enum: [1.5]}}}}\n"
        );
        let ir = lower(parse_content_yaml(&source).unwrap()).unwrap();
        let IrSchemaKind::Object(object) = &ir.schemas["Payload"].kind else {
            panic!("payload is an object");
        };
        for (name, path) in [
            ("revision", "/ScalarEnum"),
            ("flags", "/Array/ScalarEnum"),
            ("ratios", "/Map/ScalarEnum"),
        ] {
            let expr = serde_json::to_value(&object.properties[name].type_expr).unwrap();
            let metadata = expr.pointer(path).unwrap();
            assert_eq!(
                metadata["representation_is_constraint"],
                json!(false),
                "{version} {name}"
            );
            assert_eq!(
                metadata["representation"],
                json!({"Primitive": "String"}),
                "{version} {name}"
            );
        }
        let typed = serde_json::to_value(&object.properties["typed_revision"].type_expr).unwrap();
        assert_eq!(
            typed["ScalarEnum"]["representation_is_constraint"],
            json!(true),
            "{version}"
        );
    }
}

#[test]
fn inline_scalar_enum_facts_survive_each_supported_lowerer_and_container() {
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let nullable_type = if version == "3.0.3" {
            "type: integer, nullable: true"
        } else {
            "type: [integer, 'null']"
        };
        let source = format!(
            "openapi: {version}\ninfo: {{title: Scalar facts, version: '1'}}\npaths: {{}}\ncomponents:\n  schemas:\n    Payload:\n      type: object\n      properties:\n        revision: {{type: integer, enum: [1, 2]}}\n        nullable_revision: {{{nullable_type}, enum: [1, 2]}}\n        values:\n          type: array\n          items: {{type: number, enum: [1.5, 2.5]}}\n        flags:\n          type: object\n          additionalProperties: {{type: boolean, enum: [true]}}\n"
        );
        let ir = lower(parse_content_yaml(&source).unwrap()).unwrap();
        let IrSchemaKind::Object(object) = &ir.schemas["Payload"].kind else {
            panic!("payload is an object")
        };
        let revision = serde_json::to_value(&object.properties["revision"].type_expr).unwrap();
        let values = serde_json::to_value(&object.properties["values"].type_expr).unwrap();
        let flags = serde_json::to_value(&object.properties["flags"].type_expr).unwrap();
        let nullable = &object.properties["nullable_revision"];
        assert!(
            nullable.nullable,
            "{version}: property nullability remains structural"
        );
        let nullable = serde_json::to_value(&nullable.type_expr).unwrap();
        assert_eq!(nullable["ScalarEnum"]["values"], json!([1, 2]), "{version}");
        assert_eq!(
            nullable["ScalarEnum"]["representation_nullable"],
            json!(true),
            "{version}"
        );
        assert_eq!(revision["ScalarEnum"]["values"], json!([1, 2]), "{version}");
        assert_eq!(
            revision["ScalarEnum"]["representation_nullable"],
            json!(false),
            "{version}"
        );
        assert_eq!(
            values["Array"]["ScalarEnum"]["values"],
            json!([1.5, 2.5]),
            "{version}"
        );
        assert_eq!(
            flags["Map"]["ScalarEnum"]["values"],
            json!([true]),
            "{version}"
        );
    }
}

#[test]
fn nullable_wrapper_enums_retain_constraints_at_each_lowering_boundary() {
    for version in ["3.1.0", "3.2.0"] {
        let source = format!(
            r#"openapi: {version}
info: {{title: Nullable wrapper facts, version: '1'}}
paths: {{}}
components:
  schemas:
    Payload:
      type: object
      properties:
        revision:
          anyOf: [{{type: integer}}, {{type: 'null'}}]
          enum: [1]
        nested_revision:
          anyOf: [{{type: integer, enum: [1]}}, {{type: 'null'}}]
          enum: [1, null]
        values:
          type: array
          items:
            anyOf: [{{type: integer}}, {{type: 'null'}}]
            enum: [1, null]
        flags:
          type: object
          additionalProperties:
            anyOf: [{{type: integer}}, {{type: 'null'}}]
            enum: [1, null]
        rows:
          type: array
          items:
            type: array
            items:
              anyOf: [{{type: integer}}, {{type: 'null'}}]
              enum: [1, null]
"#
        );
        let ir = lower(parse_content_yaml(&source).unwrap()).unwrap();
        let IrSchemaKind::Object(object) = &ir.schemas["Payload"].kind else {
            panic!("payload is an object")
        };
        assert!(object.properties["revision"].nullable, "{version}");
        let nested = serde_json::to_value(&object.properties["nested_revision"].type_expr).unwrap();
        assert!(object.properties["nested_revision"].nullable, "{version}");
        assert_eq!(
            nested["ScalarEnum"]["representation_nullable"],
            json!(true),
            "{version}"
        );
        assert_eq!(
            nested["ScalarEnum"]["representation"]["ScalarEnum"]["representation_nullable"],
            json!(false),
            "{version}"
        );
        for (name, path, expected) in [
            ("revision", "/ScalarEnum/values", json!([1])),
            ("values", "/Array/ScalarEnum/values", json!([1, null])),
            ("flags", "/Map/ScalarEnum/values", json!([1, null])),
            ("rows", "/Array/Array/ScalarEnum/values", json!([1, null])),
        ] {
            let expr = serde_json::to_value(&object.properties[name].type_expr).unwrap();
            assert_eq!(expr.pointer(path), Some(&expected), "{version}: {name}");
        }
    }
}

#[test]
fn scalar_enum_metadata_changes_only_rust_union_dispatch_and_not_public_representations() {
    let source = r#"openapi: 3.1.0
info: {title: Scalar representations, version: '1'}
paths:
  /parts:
    post:
      operationId: sendParts
      parameters:
        - name: enabled
          in: query
          schema: {type: boolean, $BOOL default: true}
        - name: limit
          in: query
          schema: {type: [integer, 'null'], $INT format: int64}
        - name: numbers
          in: query
          required: true
          schema:
            type: array
            items: {type: [integer, 'null'], $INT format: int32}
        - name: status
          in: query
          schema:
            anyOf: [{$ref: '#/components/schemas/Status'}, {type: 'null'}]
            $WRAPPER
      requestBody:
        content:
          multipart/form-data:
            schema:
              type: object
              properties:
                revision: {type: integer, $INT format: int32}
                enabled: {type: boolean, $BOOL default: true}
                status:
                  anyOf: [{$ref: '#/components/schemas/Status'}, {type: 'null'}]
                  $WRAPPER
                values:
                  type: array
                  items: {type: number, $NUMBER format: double}
      responses:
        '200':
          description: Values
          headers:
            X-Revision:
              schema: {type: integer, $INT format: int32}
          content:
            application/json:
              schema:
                type: array
                items: {type: number, $NUMBER format: double}
components:
  schemas:
    Status:
      type: integer
      enum: [1, 2]
    Choice:
      anyOf:
        - {type: boolean, $BOOL default: true}
        - {type: integer, $INT format: int32}
    Payload:
      type: object
      properties:
        revision: {type: integer, $INT format: int64}
        optional_revision: {type: [integer, 'null'], $INT format: int64}
        wrapped_revision:
          anyOf: [{type: integer}, {type: 'null'}]
          $WRAPPER
        nested_revision:
          anyOf: [{type: integer, $INT format: int32}, {type: 'null'}]
          $WRAPPER
        status:
          anyOf: [{$ref: '#/components/schemas/Status'}, {type: 'null'}]
          $WRAPPER
        statuses:
          type: array
          items:
            anyOf: [{$ref: '#/components/schemas/Status'}, {type: 'null'}]
            $WRAPPER
        status_map:
          type: object
          additionalProperties:
            anyOf: [{$ref: '#/components/schemas/Status'}, {type: 'null'}]
            $WRAPPER
        enabled: {type: boolean, $BOOL default: true}
        flags:
          type: object
          additionalProperties: {type: boolean, $BOOL default: true}
        timestamps:
          type: array
          items: {type: string, enum: ['2026-01-01T00:00:00Z'], format: date-time}
"#;
    let decorated = source
        .replace("$INT", "enum: [1, 2],")
        .replace("$NUMBER", "enum: [1.5, 2.5],")
        .replace("$WRAPPER", "enum: [1, null]")
        .replace("$BOOL", "enum: [true],");
    let legacy = source
        .replace("$INT", "")
        .replace("$NUMBER", "")
        .replace("$WRAPPER", "")
        .replace("$BOOL", "")
        .replace("enum: ['2026-01-01T00:00:00Z'], ", "");
    let decorated = lower(parse_content_yaml(&decorated).unwrap()).unwrap();
    let legacy = lower(parse_content_yaml(&legacy).unwrap()).unwrap();
    let generators: Vec<Box<dyn CodeGenerator>> = vec![
        Box::new(GoHttpCodeGenerator::new(Default::default())),
        Box::new(JavaOkhttpCodeGenerator::new(Default::default())),
        Box::new(KotlinOkhttpCodeGenerator::new(Default::default())),
        Box::new(RustAioductCodeGenerator::new(Default::default())),
        Box::new(RustReqwestCodeGenerator::new(Default::default())),
        Box::new(RustUreqCodeGenerator::new(Default::default())),
    ];
    for generator in generators {
        let files = |ir| {
            generator
                .generate(ir)
                .unwrap()
                .into_iter()
                .map(|file| {
                    (
                        format!("{:?}/{}", file.category, file.filename),
                        file.content,
                    )
                })
                .collect::<std::collections::BTreeMap<_, _>>()
        };
        let actual = files(&decorated);
        let expected = files(&legacy);
        for (filename, expected) in &expected {
            if generator.generator_type().language() == openapi_nexus::codegen::Language::Rust
                && filename == "Models/choice.rs"
            {
                let actual = &actual[filename];
                assert_ne!(actual, expected, "Rust dispatch consumes finite facts");
                assert_eq!(
                    actual.split("\nimpl").next(),
                    expected.split("\nimpl").next(),
                    "typed variants and payloads do not change"
                );
                assert!(actual.contains("Some(1_i64)"));
                assert!(actual.contains("Some(true)"));
                continue;
            }
            assert_eq!(
                actual.get(filename),
                Some(expected),
                "{} {filename}",
                generator.generator_type()
            );
        }
        assert_eq!(
            actual.len(),
            expected.len(),
            "{} file count",
            generator.generator_type()
        );
    }
    // Python now consumes these facts for union dispatch, while keeping ordinary
    // model declarations and the other codecs on their existing representations.
    // Retain exact comparison outside the new matcher, not a blanket exclusion.
    let python: Vec<Box<dyn CodeGenerator>> = vec![
        Box::new(PythonHttpxCodeGenerator::new(Default::default())),
        Box::new(PythonRequestsCodeGenerator::new(Default::default())),
    ];
    for generator in python {
        let files = |ir| {
            generator
                .generate(ir)
                .unwrap()
                .into_iter()
                .map(|file| ((file.category, file.filename), file.content))
                .collect::<std::collections::HashMap<_, _>>()
        };
        let actual = files(&decorated);
        let expected = files(&legacy);
        assert_eq!(actual.len(), expected.len());
        for (key, legacy_source) in expected {
            let source = actual.get(&key).unwrap();
            if key.1 == "_union.py" {
                assert_ne!(
                    source,
                    &legacy_source,
                    "{} uses retained union constraints",
                    generator.generator_type()
                );
                assert!(source.contains("_union_support.equal(value, True)"));
                assert!(source.contains("_union_support.equal(value, 1)"));
            } else {
                assert_eq!(
                    source,
                    &legacy_source,
                    "{} {}",
                    generator.generator_type(),
                    key.1
                );
            }
        }
    }
}
