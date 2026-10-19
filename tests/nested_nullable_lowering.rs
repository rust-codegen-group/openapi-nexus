//! Nested schema semantics through the public parser/lowering boundary.

use openapi_nexus::ir::types::{IrPrimitive, IrSchemaKind, IrTypeExpr};

fn response_type(version: &str, schema: &str) -> IrTypeExpr {
    let schema = schema
        .lines()
        .map(|line| format!("                {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let source = format!(
        "openapi: {version}\ninfo:\n  title: Nested Nullable\n  version: '1'\npaths:\n  /values:\n    get:\n      operationId: getValues\n      responses:\n        '400':\n          description: Error\n          content:\n            application/json:\n              schema:\n{schema}\n"
    );
    let parsed = openapi_nexus::parser::parse_content_yaml(&source).expect("schema parses");
    let ir = openapi_nexus::ir::lower::lower(parsed).expect("schema lowers");
    ir.operations[0].responses[0].content["application/json"].clone()
}

#[test]
fn oas30_map_values_keep_nullable_arrays() {
    let actual = response_type(
        "3.0.3",
        "type: object\nadditionalProperties:\n  type: array\n  nullable: true\n  items:\n    type: string",
    );
    assert_eq!(
        actual,
        IrTypeExpr::Map(Box::new(IrTypeExpr::Nullable(Box::new(IrTypeExpr::Array(
            Box::new(IrTypeExpr::Primitive(IrPrimitive::String)),
        )))))
    );
}

fn assert_nullable_map_values(version: &str) {
    assert_eq!(
        response_type(
            version,
            "type: object\nadditionalProperties:\n  type: [array, 'null']\n  items:\n    type: string",
        ),
        IrTypeExpr::Map(Box::new(IrTypeExpr::Nullable(Box::new(IrTypeExpr::Array(
            Box::new(IrTypeExpr::Primitive(IrPrimitive::String)),
        )))))
    );
}

#[test]
fn oas31_map_values_keep_nullable_arrays() {
    assert_nullable_map_values("3.1.0");
}

#[test]
fn oas32_map_values_keep_nullable_arrays() {
    assert_nullable_map_values("3.2.0");
}

#[test]
fn nested_nullability_stays_on_the_declared_node() {
    let string = IrTypeExpr::Primitive(IrPrimitive::String);
    let cases = [
        (
            "type: array\nitems:\n  type: string\n  nullable: true",
            "type: array\nitems:\n  type: [string, 'null']",
            IrTypeExpr::Array(Box::new(IrTypeExpr::Nullable(Box::new(string.clone())))),
        ),
        (
            "type: array\nitems:\n  type: object\n  nullable: true\n  additionalProperties:\n    type: string",
            "type: array\nitems:\n  type: [object, 'null']\n  additionalProperties:\n    type: string",
            IrTypeExpr::Array(Box::new(IrTypeExpr::Nullable(Box::new(IrTypeExpr::Map(
                Box::new(string.clone()),
            ))))),
        ),
        (
            "type: array\nnullable: true\nitems:\n  type: object\n  additionalProperties:\n    type: string",
            "type: [array, 'null']\nitems:\n  type: object\n  additionalProperties:\n    type: string",
            IrTypeExpr::Nullable(Box::new(IrTypeExpr::Array(Box::new(IrTypeExpr::Map(
                Box::new(string.clone()),
            ))))),
        ),
        (
            "type: object\nnullable: true\nadditionalProperties:\n  type: array\n  items:\n    type: string",
            "type: [object, 'null']\nadditionalProperties:\n  type: array\n  items:\n    type: string",
            IrTypeExpr::Nullable(Box::new(IrTypeExpr::Map(Box::new(IrTypeExpr::Array(
                Box::new(string.clone()),
            ))))),
        ),
        (
            "type: object\nadditionalProperties:\n  type: array\n  items:\n    type: string",
            "type: object\nadditionalProperties:\n  type: array\n  items:\n    type: string",
            IrTypeExpr::Map(Box::new(IrTypeExpr::Array(Box::new(string.clone())))),
        ),
    ];
    for (oas30, oas31, expected) in cases {
        for (version, schema) in [("3.0.3", oas30), ("3.1.0", oas31), ("3.2.0", oas31)] {
            assert_eq!(
                response_type(version, schema),
                expected,
                "{version}: {schema}"
            );
        }
    }
}

#[test]
fn promoted_array_items_keep_container_and_item_nullability() {
    for (version, schema) in [
        (
            "3.0.3",
            "type: array\nnullable: true\nitems:\n  type: object\n  nullable: true\n  properties:\n    message:\n      type: string",
        ),
        (
            "3.1.0",
            "type: [array, 'null']\nitems:\n  type: [object, 'null']\n  properties:\n    message:\n      type: string",
        ),
        (
            "3.2.0",
            "type: [array, 'null']\nitems:\n  type: [object, 'null']\n  properties:\n    message:\n      type: string",
        ),
    ] {
        let actual = response_type(version, schema);
        let IrTypeExpr::Nullable(array) = actual else {
            panic!("{version}: nullable container lost: {actual:?}")
        };
        let IrTypeExpr::Array(item) = *array else {
            panic!("expected array")
        };
        assert!(
            matches!(*item, IrTypeExpr::Nullable(ref inner) if matches!(**inner, IrTypeExpr::Named(_))),
            "{version}: nullable promoted item lost: {item:?}"
        );
    }
}

#[test]
fn named_nullable_array_aliases_keep_the_container_wrapper() {
    for (version, declaration) in [
        ("3.0.3", "type: array\n      nullable: true"),
        ("3.1.0", "type: [array, 'null']"),
        ("3.2.0", "type: [array, 'null']"),
    ] {
        let yaml = format!(
            "openapi: {version}\ninfo:\n  title: Nullable Alias\n  version: '1'\npaths: {{}}\ncomponents:\n  schemas:\n    MaybeValues:\n      {declaration}\n      items:\n        type: string\n"
        );
        let parsed = openapi_nexus::parser::parse_content_yaml(&yaml).expect("alias schema parses");
        let ir = openapi_nexus::ir::lower::lower(parsed).expect("alias schema lowers");
        let IrSchemaKind::Alias(alias) = &ir.schemas["MaybeValues"].kind else {
            panic!("expected array alias")
        };
        assert_eq!(
            alias,
            &IrTypeExpr::Nullable(Box::new(IrTypeExpr::Array(Box::new(
                IrTypeExpr::Primitive(IrPrimitive::String)
            )))),
            "{version}"
        );
    }
}

#[test]
fn nested_type_sets_preserve_all_non_null_members() {
    for version in ["3.1.0", "3.2.0"] {
        let actual = response_type(
            version,
            "type: array\nitems:\n  type: [string, integer, 'null']",
        );
        assert_eq!(
            actual,
            IrTypeExpr::Array(Box::new(IrTypeExpr::Nullable(Box::new(IrTypeExpr::Union(
                vec![
                    IrTypeExpr::Primitive(IrPrimitive::String),
                    IrTypeExpr::Primitive(IrPrimitive::Integer),
                ]
            ))))),
            "{version}"
        );
    }
}

#[test]
fn nested_nullable_union_wrappers_match_top_level_semantics() {
    for version in ["3.1.0", "3.2.0"] {
        for keyword in ["oneOf", "anyOf"] {
            for (container, expected) in [
                (
                    "type: array\nitems:",
                    IrTypeExpr::Array(Box::new(IrTypeExpr::Nullable(Box::new(
                        IrTypeExpr::Primitive(IrPrimitive::String),
                    )))),
                ),
                (
                    "type: object\nadditionalProperties:",
                    IrTypeExpr::Map(Box::new(IrTypeExpr::Nullable(Box::new(
                        IrTypeExpr::Primitive(IrPrimitive::String),
                    )))),
                ),
            ] {
                let schema =
                    format!("{container}\n  {keyword}:\n    - type: 'null'\n    - type: string");
                assert_eq!(
                    response_type(version, &schema),
                    expected,
                    "{version}: {keyword}: {container}"
                );
            }
        }
    }
}

#[test]
fn named_map_values_keep_nullable_union_wrappers() {
    for version in ["3.1.0", "3.2.0"] {
        for keyword in ["oneOf", "anyOf"] {
            for members in [
                "[{type: 'null'}, {type: string}]",
                "[{type: string}, {type: 'null'}]",
            ] {
                let yaml = format!(
                    "openapi: {version}\ninfo:\n  title: Named Nullable Map\n  version: '1'\npaths: {{}}\ncomponents:\n  schemas:\n    NullableStrings:\n      type: object\n      additionalProperties:\n        {keyword}: {members}\n"
                );
                let parsed =
                    openapi_nexus::parser::parse_content_yaml(&yaml).expect("named map parses");
                let ir = openapi_nexus::ir::lower::lower(parsed).expect("named map lowers");
                let IrSchemaKind::Object(map) = &ir.schemas["NullableStrings"].kind else {
                    panic!("named map should keep its object schema");
                };
                assert_eq!(
                    map.additional_properties,
                    Some(IrTypeExpr::Nullable(Box::new(IrTypeExpr::Primitive(
                        IrPrimitive::String
                    )))),
                    "{version}: {keyword}: {members}"
                );
            }
        }
    }
}
