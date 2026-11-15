use indexmap::IndexMap;
use serde_json::json;

use super::SchemaDependencies;
use crate::ir::types::{
    IrEnum, IrEnumValue, IrEnumValueType, IrHeader, IrInfo, IrIntersection, IrObject, IrOperation,
    IrParameter, IrPrimitive, IrProperty, IrRequestBody, IrRequestBodyEncoding, IrResponse,
    IrSchema, IrSchemaKind, IrSpec, IrTaggedUnion, IrTaggedVariant, IrTypeExpr, IrUnion,
    ParameterLocation, TaggingStyle,
};

#[test]
fn native_dependencies_are_direct_ordered_facts_not_reference_validation() {
    let source = json!({
        "allOf": [
            {"$ref": "#/components/schemas/Zulu"},
            {"$ref": "#/components/schemas/Zulu"},
            {"$ref": "external.yaml#/components/schemas/External"},
            {"$ref": "#/components/responses/ResponseA"}
        ],
        "anyOf": [{"$ref": "#/components/schemas/Alpha"}],
        "oneOf": [{"$ref": "#/components/schemas/Beta"}],
        "properties": {
            "a": {"$ref": "#/components/schemas/SelfRef"},
            "b": {"$ref": "#/components/schemas/Alpha"},
            "c": {"allOf": [{"$ref": "#/components/schemas/Nested"}]},
            "d": {"$ref": "#/components/schemas/Escaped~1Name"},
            "e": {"$ref": "#/components/schemas/"}
        },
        "items": {"$ref": "#/components/schemas/Items"},
        "additionalProperties": {"$ref": "#/components/schemas/Additional"},
        "prefixItems": [{"$ref": "#/components/schemas/TupleOnly"}],
        "discriminator": {
            "propertyName": "kind",
            "mapping": {"a": "#/components/schemas/MappingOnly"}
        },
        "example": {"$ref": "#/components/schemas/ExampleData"},
        "default": {"$ref": "#/components/schemas/DefaultData"},
        "x-data": {"$ref": "#/components/schemas/ExtensionData"}
    });
    let expected = [
        "Zulu",
        "Alpha",
        "Beta",
        "SelfRef",
        "Nested",
        "Escaped~1Name",
        "",
        "Items",
        "Additional",
    ];
    let v31: crate::spec::oas31::spec::ObjectSchema =
        serde_json::from_value(source.clone()).unwrap();
    let v32: crate::spec::oas32::spec::ObjectSchema = serde_json::from_value(source).unwrap();

    // No component catalog exists here: unknown or recursive targets are facts,
    // not an instruction to resolve them or reject them during collection.
    assert_eq!(
        v31.schema_dependencies().into_iter().collect::<Vec<_>>(),
        expected
    );
    assert_eq!(
        v32.schema_dependencies().into_iter().collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn native_literal_schemas_and_reference_shaped_data_have_no_dependencies() {
    let source = json!({
        "items": true,
        "additionalProperties": false,
        "properties": {"value": {
            "type": "string",
            "enum": ["#/components/schemas/EnumData"],
            "default": "#/components/schemas/DefaultData",
            "example": {"$ref": "#/components/schemas/ExampleData"}
        }}
    });
    let v31: crate::spec::oas31::spec::ObjectSchema =
        serde_json::from_value(source.clone()).unwrap();
    let v32: crate::spec::oas32::spec::ObjectSchema = serde_json::from_value(source).unwrap();
    assert!(v31.schema_dependencies().is_empty());
    assert!(v32.schema_dependencies().is_empty());
}

fn empty_spec() -> IrSpec {
    IrSpec {
        info: IrInfo {
            title: "Dependency facts".into(),
            description: None,
            version: "1".into(),
            terms_of_service: None,
            contact: None,
            license: None,
        },
        servers: Vec::new(),
        schemas: IndexMap::new(),
        operations: Vec::new(),
        security_schemes: IndexMap::new(),
        security: Vec::new(),
    }
}

fn named(name: &str) -> IrTypeExpr {
    IrTypeExpr::Named(name.into())
}

fn schema(name: &str, kind: IrSchemaKind) -> IrSchema {
    IrSchema {
        name: name.into(),
        description: None,
        deprecated: false,
        kind,
        is_component: true,
    }
}

#[test]
fn ir_dependencies_cover_all_model_kinds_and_type_wrappers() {
    let nested = IrTypeExpr::Array(Box::new(IrTypeExpr::Map(Box::new(IrTypeExpr::Nullable(
        Box::new(IrTypeExpr::ScalarEnum {
            representation: Box::new(IrTypeExpr::Union(vec![
                named("Zulu"),
                named("Alpha"),
                named("Zulu"),
                IrTypeExpr::Primitive(IrPrimitive::String),
                IrTypeExpr::StringLiteral("#/components/schemas/LiteralData".into()),
                IrTypeExpr::StringEnum(vec!["#/components/schemas/EnumData".into()]),
                IrTypeExpr::Any,
            ])),
            representation_is_constraint: true,
            representation_nullable: true,
            values: vec![json!("#/components/schemas/ScalarData")],
        }),
    )))));
    let models = [
        schema(
            "ObjectDeclaration",
            IrSchemaKind::Object(IrObject {
                properties: IndexMap::from([(
                    "value".into(),
                    IrProperty {
                        name: "value".into(),
                        type_expr: nested,
                        required: true,
                        nullable: false,
                        description: None,
                        default_value: Some(json!({"$ref": "#/components/schemas/DefaultData"})),
                        format: None,
                        validation: None,
                    },
                )]),
                additional_properties: Some(named("Additional")),
            }),
        ),
        schema(
            "AliasDeclaration",
            IrSchemaKind::Alias(named("AliasTarget")),
        ),
        schema(
            "TaggedDeclaration",
            IrSchemaKind::TaggedUnion(IrTaggedUnion {
                discriminator_field: "kind".into(),
                tagging: TaggingStyle::Adjacent {
                    content_field: "payload".into(),
                },
                variants: vec![IrTaggedVariant {
                    discriminator_value: "VariantA".into(),
                    discriminator_values: vec!["#/components/schemas/TagData".into()],
                    discriminator_optional: false,
                    content_type: named("VariantTarget"),
                    description: None,
                    additional_properties: Some(named("VariantAdditional")),
                }],
            }),
        ),
        schema(
            "UnionDeclaration",
            IrSchemaKind::Union(IrUnion {
                members: vec![
                    named("UnionTarget"),
                    named("#/components/schemas/Verbatim"),
                    named("Alpha"),
                ],
                nullable: false,
            }),
        ),
        schema(
            "IntersectionDeclaration",
            IrSchemaKind::Intersection(IrIntersection {
                members: vec![named("IntersectionTarget")],
            }),
        ),
        schema(
            "EnumDeclaration",
            IrSchemaKind::Enum(IrEnum {
                value_type: IrEnumValueType::String,
                values: vec![IrEnumValue {
                    value: json!("#/components/schemas/EnumData"),
                    description: None,
                }],
            }),
        ),
    ];
    let mut ir = empty_spec();
    for model in models {
        ir.schemas.insert(model.name.clone(), model);
    }
    assert_eq!(
        ir.schema_dependencies().into_iter().collect::<Vec<_>>(),
        [
            "Zulu",
            "Alpha",
            "Additional",
            "AliasTarget",
            "VariantTarget",
            "VariantAdditional",
            "UnionTarget",
            "#/components/schemas/Verbatim",
            "IntersectionTarget"
        ]
    );
}

#[test]
fn ir_dependencies_include_all_retained_operation_type_positions() {
    let mut ir = empty_spec();
    ir.operations.push(IrOperation {
        operation_id: "getValue".into(),
        tags: vec!["MetadataTag".into()],
        method: "GET".into(),
        path: "/values".into(),
        summary: None,
        description: None,
        deprecated: false,
        parameters: vec![IrParameter {
            name: "value".into(),
            location: ParameterLocation::Query,
            type_expr: named("Parameter"),
            required: false,
            description: None,
            default_value: Some(json!({"$ref": "#/components/schemas/DefaultData"})),
        }],
        request_body: Some(IrRequestBody {
            required: false,
            description: None,
            content: IndexMap::from([
                ("application/json".into(), named("RequestA")),
                ("text/plain".into(), named("RequestB")),
            ]),
            encoding: IndexMap::from([(
                "application/json".into(),
                IndexMap::from([(
                    "payload".into(),
                    IrRequestBodyEncoding {
                        content_type: Some("application/json".into()),
                    },
                )]),
            )]),
        }),
        responses: vec![IrResponse {
            status: "200".into(),
            description: "A value".into(),
            content: IndexMap::from([
                ("application/json".into(), named("ResponseA")),
                ("text/plain".into(), named("ResponseB")),
            ]),
            item_content: IndexMap::from([("text/event-stream".into(), named("StreamItem"))]),
            headers: IndexMap::from([(
                "X-Value".into(),
                IrHeader {
                    description: None,
                    type_expr: IrTypeExpr::Nullable(Box::new(named("Header"))),
                    required: false,
                },
            )]),
        }],
        security: Vec::new(),
    });
    assert_eq!(
        ir.schema_dependencies().into_iter().collect::<Vec<_>>(),
        [
            "Parameter",
            "RequestA",
            "RequestB",
            "ResponseA",
            "ResponseB",
            "StreamItem",
            "Header"
        ]
    );
}
