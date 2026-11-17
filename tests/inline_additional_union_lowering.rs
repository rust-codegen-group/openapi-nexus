//! Inline dictionary values retain the same union facts as named equivalents.

use openapi_nexus::ir::types::{IrPrimitive, IrSchemaKind, IrSpec, IrTypeExpr};
use openapi_nexus::{ir, parser};

fn union_members<'a>(spec: &'a IrSpec, expr: &'a IrTypeExpr) -> &'a [IrTypeExpr] {
    match expr {
        IrTypeExpr::Union(members) => members,
        IrTypeExpr::Named(name) => match &spec.schemas[name].kind {
            IrSchemaKind::Union(union) if !union.nullable => &union.members,
            other => panic!("expected a non-null union, got {other:?}"),
        },
        other => panic!("inline placement must not erase alternatives: {other:?}"),
    }
}

fn fixture_for_version(version: &str) -> String {
    let mut fixture: serde_norway::Value =
        serde_norway::from_str(include_str!("fixtures/valid/inline-additional-union.yaml"))
            .unwrap();
    if version == "3.0.3" {
        // The shared generated-client fixture also exercises 3.1 type sets.
        // Version-specific regression tests below cover their 3.0 equivalents.
        fixture["components"]["schemas"]
            .as_mapping_mut()
            .unwrap()
            .retain(|name, _| {
                matches!(
                    name.as_str(),
                    Some(
                        "Inline"
                            | "Referenced"
                            | "Choice"
                            | "InlineOneOf"
                            | "Mixed"
                            | "NestedMap"
                            | "NestedArray"
                    )
                )
            });
    }
    fixture["openapi"] = serde_norway::Value::String(version.to_owned());
    serde_norway::to_string(&fixture).unwrap()
}

#[test]
fn inline_additional_unions_match_referenced_alternatives_in_every_version() {
    for version in ["3.1.0", "3.2.0", "3.0.3"] {
        for keyword in ["anyOf", "oneOf"] {
            let source = fixture_for_version(version).replace("anyOf", keyword);
            let spec = ir::lower::lower(parser::parse_content_yaml(&source).unwrap()).unwrap();
            let IrSchemaKind::Union(choice) = &spec.schemas["Choice"].kind else {
                panic!("{version} {keyword}: named control must retain its union");
            };
            assert_eq!(
                choice.members,
                vec![
                    IrTypeExpr::Primitive(IrPrimitive::Integer),
                    IrTypeExpr::Primitive(IrPrimitive::String),
                ],
                "{version} {keyword}: named alternatives"
            );
            assert!(!choice.nullable);
            let IrSchemaKind::Object(referenced) = &spec.schemas["Referenced"].kind else {
                panic!("{version} {keyword}: named dictionary must remain an object");
            };
            assert_eq!(
                referenced.additional_properties,
                Some(IrTypeExpr::Named("Choice".to_owned())),
                "{version} {keyword}: reference control"
            );
            let IrSchemaKind::Object(inline) = &spec.schemas["Inline"].kind else {
                panic!("{version} {keyword}: inline dictionary must remain an object");
            };
            assert_eq!(
                union_members(&spec, inline.additional_properties.as_ref().unwrap()),
                choice.members,
                "{version} {keyword}: inline placement must not erase alternatives"
            );
        }
    }
}

fn schema_source(version: &str, schemas: &str) -> String {
    let schemas = schemas
        .lines()
        .map(|line| format!("    {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "openapi: {version}\ninfo: {{title: Inline Union, version: '1'}}\npaths: {{}}\ncomponents:\n  schemas:\n{schemas}\n"
    )
}

fn lower_schemas(version: &str, schemas: &str) -> IrSpec {
    ir::lower::lower(parser::parse_content_yaml(&schema_source(version, schemas)).unwrap()).unwrap()
}

fn additional<'a>(spec: &'a IrSpec, name: &str) -> &'a IrTypeExpr {
    let IrSchemaKind::Object(object) = &spec.schemas[name].kind else {
        panic!("{name} must remain an object");
    };
    object.additional_properties.as_ref().unwrap()
}

#[test]
fn mixed_and_nested_dictionaries_keep_union_facts_and_container_shapes() {
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let source = fixture_for_version(version);
        let spec = ir::lower::lower(parser::parse_content_yaml(&source).unwrap()).unwrap();
        let expected = union_members(&spec, additional(&spec, "Inline"));
        for name in ["InlineOneOf", "Mixed"] {
            assert_eq!(union_members(&spec, additional(&spec, name)), expected);
        }
        let IrTypeExpr::Map(values) = additional(&spec, "NestedMap") else {
            panic!("{version}: nested dictionary shape lost");
        };
        assert_eq!(union_members(&spec, values), expected);
        let IrTypeExpr::Array(items) = additional(&spec, "NestedArray") else {
            panic!("{version}: array shape lost");
        };
        let IrTypeExpr::Map(values) = items.as_ref() else {
            panic!("{version}: array item dictionary shape lost");
        };
        assert_eq!(union_members(&spec, values), expected);
    }
}

#[test]
fn container_promotion_keeps_each_nullable_node_and_empty_array_items() {
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let (array_type, map_type) = if version == "3.0.3" {
            (
                "type: array\n    nullable: true",
                "type: object\n      nullable: true",
            )
        } else {
            ("type: [array, 'null']", "type: [object, 'null']")
        };
        let spec = lower_schemas(
            version,
            &format!(
                "Values:\n  type: object\n  additionalProperties:\n    {array_type}\n    items:\n      {map_type}\n      additionalProperties:\n        anyOf:\n          - type: array\n            items: {{}}\n          - type: string"
            ),
        );
        let IrTypeExpr::Nullable(array) = additional(&spec, "Values") else {
            panic!("{version}: nullable array lost");
        };
        let IrTypeExpr::Array(items) = array.as_ref() else {
            panic!("{version}: array lost");
        };
        let IrTypeExpr::Nullable(map) = items.as_ref() else {
            panic!("{version}: nullable item map lost");
        };
        let IrTypeExpr::Map(values) = map.as_ref() else {
            panic!("{version}: item map lost");
        };
        assert_eq!(
            union_members(&spec, values),
            [
                IrTypeExpr::Array(Box::new(IrTypeExpr::Any)),
                IrTypeExpr::Primitive(IrPrimitive::String),
            ]
        );
    }
}

#[test]
fn union_members_keep_inline_objects_recursive_references_and_unique_names() {
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let spec = lower_schemas(
            version,
            r#"Values:
  type: object
  additionalProperties:
    anyOf:
      - type: object
        properties:
          count: {type: integer}
      - $ref: '#/components/schemas/Values'
ValuesAdditionalProperty:
  type: string
"#,
        );
        let IrTypeExpr::Named(name) = additional(&spec, "Values") else {
            panic!("{version}: inline union must be promoted");
        };
        assert_ne!(name, "ValuesAdditionalProperty");
        assert!(spec.schemas[name].is_component);
        let members = union_members(&spec, additional(&spec, "Values"));
        assert_eq!(members[1], IrTypeExpr::Named("Values".to_owned()));
        let IrTypeExpr::Named(object_name) = &members[0] else {
            panic!("{version}: inline object member must be promoted");
        };
        let IrSchemaKind::Object(object) = &spec.schemas[object_name].kind else {
            panic!("{version}: object member lost");
        };
        assert_eq!(
            object.properties["count"].type_expr,
            IrTypeExpr::Primitive(IrPrimitive::Integer)
        );
        assert!(matches!(
            spec.schemas["ValuesAdditionalProperty"].kind,
            IrSchemaKind::Alias(IrTypeExpr::Primitive(IrPrimitive::String))
        ));
        assert_eq!(
            spec.schemas.len(),
            4,
            "{version}: references must not expand"
        );
    }
}

#[test]
fn inline_multi_member_nullable_unions_keep_null_as_a_union_fact() {
    for version in ["3.1.0", "3.2.0"] {
        for keyword in ["oneOf", "anyOf"] {
            let spec = lower_schemas(
                version,
                &format!(
                    "Values:\n  type: object\n  additionalProperties:\n    {keyword}:\n      - type: integer\n      - type: 'null'\n      - type: string"
                ),
            );
            let IrTypeExpr::Named(name) = additional(&spec, "Values") else {
                panic!("{version} {keyword}: multi-member union lost");
            };
            let IrSchemaKind::Union(union) = &spec.schemas[name].kind else {
                panic!("{version} {keyword}: union kind lost");
            };
            assert!(union.nullable);
            assert_eq!(
                union.members,
                [
                    IrTypeExpr::Primitive(IrPrimitive::Integer),
                    IrTypeExpr::Primitive(IrPrimitive::String),
                ]
            );
        }
    }
}

#[test]
fn external_tagged_wrapper_extensions_keep_inline_unions() {
    let fixture = include_str!("fixtures/valid/open-tagged-additional-properties.yaml");
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let source = fixture.replace("3.1.0", version).replace(
            "additionalProperties: true",
            "additionalProperties: {anyOf: [{type: integer}, {type: string}]}",
        );
        let spec = ir::lower::lower(parser::parse_content_yaml(&source).unwrap()).unwrap();
        let IrSchemaKind::TaggedUnion(union) = &spec.schemas["ExternalEvent"].kind else {
            panic!("{version}: external tagged union lost");
        };
        for variant in &union.variants {
            assert_eq!(
                union_members(&spec, variant.additional_properties.as_ref().unwrap()),
                [
                    IrTypeExpr::Primitive(IrPrimitive::Integer),
                    IrTypeExpr::Primitive(IrPrimitive::String),
                ]
            );
        }
    }
}

#[test]
fn properties_and_array_aliases_keep_deep_inline_union_context() {
    let container = "type: array\nitems:\n  type: array\n  items:\n    type: object\n    additionalProperties:\n      anyOf:\n        - type: integer\n        - type: string";
    let indented = |spaces| {
        container
            .lines()
            .map(|line| format!("{}{line}", " ".repeat(spaces)))
            .collect::<Vec<_>>()
            .join("\n")
    };
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let spec = lower_schemas(
            version,
            &format!(
                "Holder:\n  type: object\n  properties:\n    values:\n{}\n    inferred:\n      additionalProperties:\n        oneOf: [{{type: integer}}, {{type: string}}]\nAlias:\n{}",
                indented(6),
                indented(2),
            ),
        );
        let IrSchemaKind::Object(holder) = &spec.schemas["Holder"].kind else {
            panic!("{version}: holder lost");
        };
        let IrSchemaKind::Alias(alias) = &spec.schemas["Alias"].kind else {
            panic!("{version}: array alias lost");
        };
        let expected = [
            IrTypeExpr::Primitive(IrPrimitive::Integer),
            IrTypeExpr::Primitive(IrPrimitive::String),
        ];
        for expr in [&holder.properties["values"].type_expr, alias] {
            let IrTypeExpr::Array(outer) = expr else {
                panic!("{version}: outer array lost");
            };
            let IrTypeExpr::Array(inner) = outer.as_ref() else {
                panic!("{version}: inner array lost");
            };
            let IrTypeExpr::Map(values) = inner.as_ref() else {
                panic!("{version}: dictionary item lost");
            };
            assert_eq!(union_members(&spec, values), expected);
        }
        let IrTypeExpr::Map(values) = &holder.properties["inferred"].type_expr else {
            panic!("{version}: inferred dictionary lost");
        };
        assert_eq!(union_members(&spec, values), expected);
    }
}

#[test]
fn container_promotion_does_not_override_existing_string_enum_precedence() {
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let spec = lower_schemas(
            version,
            "Holder:\n  type: object\n  properties:\n    value:\n      type: object\n      enum: [fixed]\n      additionalProperties:\n        anyOf:\n          - type: integer\n          - type: string",
        );
        let IrSchemaKind::Object(holder) = &spec.schemas["Holder"].kind else {
            panic!("{version}: holder lost");
        };
        assert_eq!(
            holder.properties["value"].type_expr,
            IrTypeExpr::StringLiteral("fixed".to_owned())
        );
        assert_eq!(spec.schemas.len(), 1);
    }
}

#[test]
fn promoted_array_values_keep_the_original_finite_constraint() {
    let schemas = r#"Inline:
  type: object
  additionalProperties:
    type: [array, 'null']
    enum: [null]
    items:
      anyOf: [{type: integer}, {type: string}]
Referenced:
  type: object
  additionalProperties:
    type: [array, 'null']
    enum: [null]
    items: {$ref: '#/components/schemas/Choice'}
Choice:
  anyOf: [{type: integer}, {type: string}]
"#;
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let schemas = if version == "3.0.3" {
            schemas.replace("type: [array, 'null']", "type: array\n    nullable: true")
        } else {
            schemas.to_owned()
        };
        let spec = lower_schemas(version, &schemas);
        for name in ["Inline", "Referenced"] {
            let IrTypeExpr::ScalarEnum {
                representation,
                representation_is_constraint,
                values,
                ..
            } = additional(&spec, name)
            else {
                panic!("{version} {name}: array promotion lost the finite constraint");
            };
            assert!(*representation_is_constraint);
            assert_eq!(values, &[serde_json::Value::Null]);
            let IrTypeExpr::Nullable(array) = representation.as_ref() else {
                panic!("{version} {name}: nullable array lost");
            };
            let IrTypeExpr::Array(items) = array.as_ref() else {
                panic!("{version} {name}: array shape lost");
            };
            assert_eq!(
                union_members(&spec, items),
                [
                    IrTypeExpr::Primitive(IrPrimitive::Integer),
                    IrTypeExpr::Primitive(IrPrimitive::String),
                ]
            );
        }
    }
}

#[test]
fn mixed_type_sets_retain_top_level_composition_constraints() {
    for version in ["3.1.0", "3.2.0"] {
        for (keyword, expected_kind) in [
            ("oneOf", "union"),
            ("anyOf", "union"),
            ("allOf", "intersection"),
        ] {
            let spec = lower_schemas(
                version,
                &format!(
                    r#"Values:
  type: [object, string]
  {keyword}:
    - type: object
      properties:
        marker: {{type: string}}
    - type: string
Holder:
  type: object
  properties:
    value:
      type: [object, string]
      {keyword}:
        - type: object
          properties:
            marker: {{type: string}}
        - type: string
"#
                ),
            );
            match (&spec.schemas["Values"].kind, expected_kind) {
                (IrSchemaKind::Union(union), "union") => {
                    assert_eq!(union.members.len(), 2, "{version} {keyword}");
                }
                (IrSchemaKind::Intersection(intersection), "intersection") => {
                    assert_eq!(intersection.members.len(), 2, "{version} {keyword}");
                }
                (kind, _) => {
                    panic!("{version} {keyword}: composition constraint was lost in {kind:?}")
                }
            }

            let IrSchemaKind::Object(holder) = &spec.schemas["Holder"].kind else {
                panic!("{version} {keyword}: holder must remain an object");
            };
            let IrTypeExpr::Named(value_name) = &holder.properties["value"].type_expr else {
                panic!(
                    "{version} {keyword}: inline composition should be promoted, got {:?}",
                    holder.properties["value"].type_expr
                );
            };
            match (&spec.schemas[value_name].kind, expected_kind) {
                (IrSchemaKind::Union(union), "union") => {
                    assert_eq!(union.members.len(), 2, "{version} {keyword} property");
                }
                (IrSchemaKind::Intersection(intersection), "intersection") => {
                    assert_eq!(
                        intersection.members.len(),
                        2,
                        "{version} {keyword} property"
                    );
                }
                (kind, _) => {
                    panic!("{version} {keyword}: property composition was lost in {kind:?}")
                }
            }
        }
    }
}

#[test]
fn mixed_type_sets_keep_each_container_branch_and_inline_union() {
    for version in ["3.1.0", "3.2.0"] {
        for types in [
            "[object, string]",
            "[array, string]",
            "[array, object, 'null']",
            "[object, array, string, 'null']",
        ] {
            let schemas = format!(
                r#"Inline:
  type: object
  additionalProperties:
    type: {types}
    additionalProperties:
      anyOf: [{{type: integer}}, {{type: string}}]
    items:
      oneOf: [{{type: integer}}, {{type: string}}]
Referenced:
  type: object
  additionalProperties:
    type: {types}
    additionalProperties: {{$ref: '#/components/schemas/Choice'}}
    items: {{$ref: '#/components/schemas/Choice'}}
Choice:
  anyOf: [{{type: integer}}, {{type: string}}]
"#
            );
            let spec = lower_schemas(version, &schemas);
            let expected = union_members(&spec, &IrTypeExpr::Named("Choice".to_owned())).to_vec();
            for name in ["Inline", "Referenced"] {
                let mut expr = additional(&spec, name);
                if types.contains("null") {
                    let IrTypeExpr::Nullable(inner) = expr else {
                        panic!("{version} {types} {name}: null branch lost");
                    };
                    expr = inner;
                }
                let IrTypeExpr::Union(branches) = expr else {
                    panic!("{version} {types} {name}: mixed type set narrowed to {expr:?}");
                };
                assert_eq!(
                    branches.len(),
                    types.split(',').count() - usize::from(types.contains("null"))
                );
                for branch in branches {
                    match branch {
                        IrTypeExpr::Map(values) | IrTypeExpr::Array(values) => {
                            assert_eq!(
                                union_members(&spec, values),
                                expected,
                                "{version} {types} {name}"
                            );
                        }
                        IrTypeExpr::Primitive(IrPrimitive::String) => {
                            assert!(types.contains("string"))
                        }
                        other => panic!("{version} {types} {name}: unexpected branch {other:?}"),
                    }
                }
                assert_eq!(
                    branches
                        .iter()
                        .filter(|branch| matches!(branch, IrTypeExpr::Map(_)))
                        .count(),
                    usize::from(types.contains("object"))
                );
                assert_eq!(
                    branches
                        .iter()
                        .filter(|branch| matches!(branch, IrTypeExpr::Array(_)))
                        .count(),
                    usize::from(types.contains("array"))
                );
                assert_eq!(
                    branches
                        .iter()
                        .filter(|branch| matches!(
                            branch,
                            IrTypeExpr::Primitive(IrPrimitive::String)
                        ))
                        .count(),
                    usize::from(types.contains("string"))
                );
            }
        }
    }
}

#[test]
fn promoted_union_members_reject_external_references_at_every_container_depth() {
    let external = "https://example.invalid/choice";
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        for member in [
            format!("$ref: '{external}'"),
            format!("type: array\nitems: {{$ref: '{external}'}}"),
            format!("type: object\nadditionalProperties: {{$ref: '{external}'}}"),
            format!(
                "type: array\nitems:\n  type: object\n  additionalProperties: {{$ref: '{external}'}}"
            ),
            format!(
                "type: object\nproperties:\n  field:\n    type: array\n    items: {{$ref: '{external}'}}"
            ),
            format!("allOf:\n  - {{$ref: '{external}'}}"),
        ] {
            let member = member
                .lines()
                .map(|line| format!("        {line}"))
                .collect::<Vec<_>>()
                .join("\n");
            for keyword in ["anyOf", "oneOf"] {
                let schemas = format!(
                    "Values:\n  type: object\n  additionalProperties:\n    {keyword}:\n      -\n{member}\n      - type: string"
                );
                let source = schema_source(version, &schemas);
                let result = ir::lower::lower(parser::parse_content_yaml(&source).unwrap());
                assert!(
                    matches!(result, Err(ir::lower::LowerError::ExternalReference { ref reference }) if reference == external),
                    "{version} {keyword}: external member escaped validation: {result:?}"
                );
            }
        }
    }
}

#[test]
fn named_aliases_cannot_hide_external_union_members() {
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let source = schema_source(
            version,
            r#"Values:
  type: object
  additionalProperties:
    anyOf:
      - {$ref: '#/components/schemas/ExternalAlias'}
      - {type: string}
ExternalAlias:
  $ref: 'https://example.invalid/choice'
"#,
        );
        let result = ir::lower::lower(parser::parse_content_yaml(&source).unwrap());
        assert!(
            matches!(result, Err(ir::lower::LowerError::ExternalReference { .. })),
            "{version}: alias hid an external reference: {result:?}"
        );
    }
}

#[test]
fn promoted_helper_names_do_not_collide_after_model_normalization() {
    use openapi_nexus::codegen::traits::code_generator::CodeGenerator;
    use openapi_nexus::codegen::traits::file_writer::FileCategory;
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
    use std::collections::HashSet;
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        for authored in [
            "fooAdditionalProperty",
            "foo_additional_property",
            "foo-additional-property",
        ] {
            let schemas = format!(
                "Foo:\n  type: object\n  additionalProperties:\n    anyOf: [{{type: integer}}, {{type: string}}]\n{authored}:\n  type: object\n  required: [marker]\n  properties:\n    marker: {{type: string}}"
            );
            let spec = lower_schemas(version, &schemas);
            let IrSchemaKind::Object(object) = &spec.schemas[authored].kind else {
                panic!("{version}: authored object was replaced");
            };
            assert!(object.properties["marker"].required);
            assert_eq!(
                union_members(&spec, additional(&spec, "Foo")),
                [
                    IrTypeExpr::Primitive(IrPrimitive::Integer),
                    IrTypeExpr::Primitive(IrPrimitive::String),
                ]
            );
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
            for generator in generators {
                let mut paths = HashSet::new();
                for file in generator.generate(&spec).unwrap() {
                    if file.category != FileCategory::None {
                        let path = (file.category, file.filename);
                        assert!(
                            paths.insert(path.clone()),
                            "{version} {authored}: {} produced duplicate artifact {path:?}",
                            generator.generator_type()
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn mixed_type_set_object_branches_retain_fixed_properties_and_other_types() {
    for version in ["3.1.0", "3.2.0"] {
        let spec = lower_schemas(
            version,
            r#"Values:
  type: object
  additionalProperties:
    type: [object, string]
    properties:
      label: {type: string}
    required: [label]
    additionalProperties:
      anyOf: [{type: integer}, {type: string}]
"#,
        );
        let value = additional(&spec, "Values");
        let IrTypeExpr::Union(members) = value else {
            panic!("{version}: mixed object/string value was narrowed to {value:?}");
        };
        assert!(members.contains(&IrTypeExpr::Primitive(IrPrimitive::String)));
        let Some(IrTypeExpr::Named(object_name)) = members.iter().find(|member| {
            matches!(member, IrTypeExpr::Named(name) if matches!(spec.schemas[name].kind, IrSchemaKind::Object(_)))
        }) else {
            panic!("{version}: object branch was not promoted as a named object: {members:?}");
        };
        let IrSchemaKind::Object(object) = &spec.schemas[object_name].kind else {
            unreachable!();
        };
        assert!(object.properties.contains_key("label"));
        assert!(object.properties["label"].required);
        assert!(
            union_members(&spec, object.additional_properties.as_ref().unwrap())
                .contains(&IrTypeExpr::Primitive(IrPrimitive::Integer))
        );
        assert!(
            union_members(&spec, object.additional_properties.as_ref().unwrap())
                .contains(&IrTypeExpr::Primitive(IrPrimitive::String))
        );
    }
}

#[test]
fn mixed_type_set_properties_preserve_array_and_nullable_branches() {
    for version in ["3.1.0", "3.2.0"] {
        let spec = lower_schemas(
            version,
            r#"Values:
  type: object
  additionalProperties:
    type: [object, array, string, 'null']
    properties:
      label: {type: string}
    required: [label]
    additionalProperties:
      anyOf: [{type: integer}, {type: string}]
    items:
      oneOf: [{type: integer}, {type: string}]
"#,
        );
        let IrTypeExpr::Nullable(inner) = additional(&spec, "Values") else {
            panic!("{version}: mixed type-set lost nullable container branch");
        };
        let IrTypeExpr::Union(members) = inner.as_ref() else {
            panic!("{version}: mixed type-set narrowed after nullable wrapping: {inner:?}");
        };
        assert_eq!(
            members.len(),
            3,
            "{version}: expected object, array, and string branches"
        );
        assert!(
            members
                .iter()
                .any(|member| matches!(member, IrTypeExpr::Primitive(IrPrimitive::String)))
        );
        assert!(members.iter().any(|member| matches!(member, IrTypeExpr::Named(name) if matches!(spec.schemas[name].kind, IrSchemaKind::Object(_)))));
        let Some(IrTypeExpr::Array(items)) = members
            .iter()
            .find(|member| matches!(member, IrTypeExpr::Array(_)))
        else {
            panic!("{version}: array branch missing: {members:?}");
        };
        assert_eq!(
            union_members(&spec, items),
            [
                IrTypeExpr::Primitive(IrPrimitive::Integer),
                IrTypeExpr::Primitive(IrPrimitive::String),
            ]
        );
    }
}

#[test]
fn named_mixed_components_and_inline_values_share_the_same_branches() {
    for version in ["3.1.0", "3.2.0"] {
        let spec = lower_schemas(
            version,
            r#"Inline:
  type: object
  additionalProperties:
    type: [object, string]
    properties:
      label: {type: string}
    required: [label]
    additionalProperties:
      anyOf: [{type: integer}, {type: string}]
Referenced:
  type: object
  additionalProperties:
    $ref: '#/components/schemas/MixedValue'
MixedValue:
  type: [object, string]
  properties:
    label: {type: string}
  required: [label]
  additionalProperties:
    anyOf: [{type: integer}, {type: string}]
"#,
        );
        let IrSchemaKind::Alias(IrTypeExpr::Union(named_members)) =
            &spec.schemas["MixedValue"].kind
        else {
            panic!(
                "{version}: named mixed component lost its union: {:?}",
                spec.schemas["MixedValue"].kind
            );
        };
        assert!(named_members.contains(&IrTypeExpr::Primitive(IrPrimitive::String)));
        let Some(IrTypeExpr::Named(object_name)) = named_members.iter().find(|member| {
            matches!(member, IrTypeExpr::Named(name) if matches!(spec.schemas[name].kind, IrSchemaKind::Object(_)))
        }) else {
            panic!("{version}: named object branch missing: {named_members:?}");
        };
        let IrSchemaKind::Object(object) = &spec.schemas[object_name].kind else {
            unreachable!();
        };
        assert!(object.properties.contains_key("label"));
        let IrSchemaKind::Object(referenced) = &spec.schemas["Referenced"].kind else {
            panic!("{version}: referenced owner lost object shape");
        };
        assert_eq!(
            referenced.additional_properties,
            Some(IrTypeExpr::Named("MixedValue".to_owned()))
        );
    }
}

#[test]
fn ignored_inline_siblings_do_not_change_inline_and_named_acceptance() {
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let source = schema_source(
            version,
            r#"Inline:
  type: object
  additionalProperties:
    type: string
    enum: [kept]
    properties:
      unused:
        $ref: 'https://example.invalid/ignored'
NamedValue:
  type: string
  enum: [kept]
  properties:
    unused:
      $ref: 'https://example.invalid/ignored'
Referenced:
  type: object
  additionalProperties:
    $ref: '#/components/schemas/NamedValue'
"#,
        );
        let spec = ir::lower::lower(parser::parse_content_yaml(&source).unwrap()).unwrap_or_else(
            |error| panic!("{version}: ignored sibling changed acceptance: {error}"),
        );
        let IrSchemaKind::Object(inline) = &spec.schemas["Inline"].kind else {
            panic!("{version}: inline owner lost object shape");
        };
        match inline.additional_properties.as_ref().unwrap() {
            IrTypeExpr::Named(name) => {
                assert!(matches!(spec.schemas[name].kind, IrSchemaKind::Enum(_)));
            }
            IrTypeExpr::StringLiteral(value) => assert_eq!(value, "kept"),
            other => {
                panic!("{version}: ignored inline siblings changed the consumed value: {other:?}")
            }
        }
        let IrSchemaKind::Object(referenced) = &spec.schemas["Referenced"].kind else {
            panic!("{version}: referenced owner lost object shape");
        };
        assert_eq!(
            referenced.additional_properties,
            Some(IrTypeExpr::Named("NamedValue".to_owned())),
            "{version}: named control changed shape"
        );
    }
}

#[test]
fn kotlin_union_adapter_names_avoid_authored_model_symbols() {
    use openapi_nexus::codegen::traits::code_generator::CodeGenerator;
    use openapi_nexus::generators::kotlin::okhttp::KotlinOkhttpCodeGenerator;

    let spec = lower_schemas(
        "3.1.0",
        r#"Values:
  type: object
  additionalProperties:
    anyOf: [{type: integer}, {type: string}]
ValuesAdditionalPropertyJsonAdapter:
  type: object
  properties:
    marker: {type: string}
"#,
    );
    let files = KotlinOkhttpCodeGenerator::new(Default::default())
        .generate(&spec)
        .unwrap();
    let union_file = files
        .iter()
        .find(|file| file.filename == "ValuesAdditionalProperty.kt")
        .expect("promoted union model is emitted");
    assert!(
        union_file
            .content
            .contains("@JsonAdapter(ValuesAdditionalPropertyJsonAdapter2::class")
    );
    assert!(
        union_file
            .content
            .contains("private class ValuesAdditionalPropertyJsonAdapter2")
    );
    let authored_file = files
        .iter()
        .find(|file| file.filename == "ValuesAdditionalPropertyJsonAdapter.kt")
        .expect("authored model is emitted");
    assert!(
        authored_file
            .content
            .contains("data class ValuesAdditionalPropertyJsonAdapter")
    );
}

#[test]
fn simple_dictionary_values_keep_existing_empty_boolean_and_reference_behavior() {
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        for (declaration, expected) in [
            ("additionalProperties: {}", Some(IrTypeExpr::Any)),
            ("additionalProperties: true", Some(IrTypeExpr::Any)),
            ("additionalProperties: false", None),
            (
                "additionalProperties: {type: integer}",
                Some(IrTypeExpr::Primitive(IrPrimitive::Integer)),
            ),
        ] {
            let spec = lower_schemas(
                version,
                &format!("Values:\n  type: object\n  {declaration}"),
            );
            let IrSchemaKind::Object(object) = &spec.schemas["Values"].kind else {
                panic!("{version}: dictionary lost");
            };
            assert_eq!(object.additional_properties, expected);
            assert_eq!(
                spec.schemas.len(),
                1,
                "{version}: simple values must not promote"
            );
        }
        let source = format!(
            "openapi: {version}\ninfo: {{title: Inline Union, version: '1'}}\npaths: {{}}\ncomponents:\n  schemas:\n    Values:\n      type: object\n      additionalProperties: {{$ref: 'https://example.invalid/schema'}}\n"
        );
        assert!(matches!(
            ir::lower::lower(parser::parse_content_yaml(&source).unwrap()),
            Err(ir::lower::LowerError::ExternalReference { .. })
        ));
    }
}
