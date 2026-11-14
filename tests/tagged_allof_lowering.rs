use std::collections::BTreeMap;

use openapi_nexus::IrSpec;
use openapi_nexus::ir::lower::lower;
use openapi_nexus::ir::types::{IrProperty, IrSchemaKind, IrTaggedVariant, IrTypeExpr};
use openapi_nexus::parser::parse_content_yaml;

const FIXTURE: &str = include_str!("fixtures/valid/tagged-allof-payloads.yaml");

fn spec(version: &str) -> IrSpec {
    lower(parse_content_yaml(&FIXTURE.replacen("3.1.0", version, 1)).unwrap()).unwrap()
}

fn variant<'a>(ir: &'a IrSpec, tag: &str) -> &'a IrTaggedVariant {
    let IrSchemaKind::TaggedUnion(union) = &ir.schemas["Variant"].kind else {
        panic!("Variant must retain internal tagging");
    };
    union
        .variants
        .iter()
        .find(|variant| variant.discriminator_value == tag)
        .unwrap()
}

fn properties<'a>(ir: &'a IrSpec, expr: &IrTypeExpr) -> BTreeMap<&'a str, &'a IrProperty> {
    if matches!(expr, IrTypeExpr::Any) {
        return BTreeMap::new();
    }
    let IrTypeExpr::Named(name) = expr else {
        panic!("expected a named payload, got {expr:?}");
    };
    assert!(
        ir.schemas[name].is_component,
        "promoted component payload {name}"
    );
    match &ir.schemas[name].kind {
        IrSchemaKind::Object(object) => object
            .properties
            .iter()
            .map(|(name, prop)| (name.as_str(), prop))
            .collect(),
        IrSchemaKind::Intersection(intersection) => intersection
            .members
            .iter()
            .flat_map(|member| properties(ir, member))
            .collect(),
        other => panic!("expected object composition, got {other:?}"),
    }
}

#[test]
fn tagged_allof_keeps_inline_payload_metadata_and_common_options() {
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let ir = spec(version);
        let fields = properties(&ir, &variant(&ir, "variant_a").content_type);
        assert!(
            fields.contains_key("label"),
            "{version}: inline label was lost"
        );
        assert!(fields["label"].required);
        assert_eq!(
            fields["label"].description.as_deref(),
            Some("Variant label")
        );
        assert_eq!(
            fields["label"].validation.as_ref().unwrap().min_length,
            Some(1)
        );
        assert!(fields["arguments"].required);
        assert!(!fields["strict_mode"].required);
        assert!(fields.contains_key("details"));
        assert!(fields.contains_key("deferred"));
        assert!(fields.contains_key("allowed_sources"));
        assert!(fields.contains_key("output_shape"));
        assert!(
            !fields.contains_key("type"),
            "tag is owned by the union, not its payload"
        );
    }
}

#[test]
fn tagged_allof_keeps_recursive_members_and_nested_config_payloads() {
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let ir = spec(version);
        let members = properties(&ir, &variant(&ir, "variant_b").content_type);
        assert!(
            members.contains_key("members"),
            "{version}: nested members were lost"
        );
        assert_eq!(
            members["members"].type_expr,
            IrTypeExpr::Array(Box::new(IrTypeExpr::Named("Variant".into())))
        );
        let configuration = properties(&ir, &variant(&ir, "variant_c").content_type);
        assert_eq!(
            configuration
                .get("configuration")
                .map(|prop| &prop.type_expr),
            Some(&IrTypeExpr::Named("NestedConfig".into()))
        );
    }
}

#[test]
fn tag_object_can_also_carry_payload_fields() {
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let ir = spec(version);
        let fields = properties(&ir, &variant(&ir, "variant_d").content_type);
        assert!(
            fields.contains_key("input_shape"),
            "{version}: fields beside the tag were lost"
        );
        assert!(fields["input_shape"].required);
        assert!(fields.contains_key("deferred"));
        assert!(!fields.contains_key("type"));
    }
}

#[test]
fn tagged_allof_does_not_need_a_reference_to_retain_its_payload() {
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let ir = spec(version);
        let fields = properties(&ir, &variant(&ir, "variant_e").content_type);
        assert!(
            fields.contains_key("inline_label"),
            "{version}: inline-only payload was lost"
        );
        assert!(!fields.contains_key("type"));
    }
}

#[test]
fn tagged_allof_retains_every_reference_not_only_the_last_one() {
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let ir = spec(version);
        let fields = properties(&ir, &variant(&ir, "variant_f").content_type);
        assert!(
            fields.contains_key("deferred"),
            "{version}: first reference was lost"
        );
        assert!(fields.contains_key("display_name"));
        assert!(fields["display_name"].required);
    }
}

#[test]
fn tagged_allof_retains_properties_authored_alongside_allof() {
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let ir = spec(version);
        let fields = properties(&ir, &variant(&ir, "variant_g").content_type);
        assert!(
            fields.contains_key("outer_label"),
            "{version}: outer properties were lost"
        );
        assert!(fields["outer_label"].required);
        assert!(fields.contains_key("deferred"));
    }
}

#[test]
fn nested_composition_keeps_its_own_properties_and_required_fields() {
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let ir = spec(version);
        let fields = properties(&ir, &variant(&ir, "variant_k").content_type);
        assert!(
            fields.contains_key("outer_label"),
            "{version}: nested sibling field was lost"
        );
        assert!(fields["outer_label"].required);
        assert!(fields.contains_key("inner_label"));
    }
}

#[test]
fn outer_requiredness_strengthens_only_the_variant_payload() {
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let ir = spec(version);
        let fields = properties(&ir, &variant(&ir, "variant_i").content_type);
        assert!(
            fields["label"].required,
            "{version}: outer requiredness was lost"
        );
        let original = properties(&ir, &IrTypeExpr::Named("OptionalLabel".into()));
        assert!(
            !original["label"].required,
            "requiredness must not mutate the referenced component"
        );
    }
}

#[test]
fn repeated_required_only_members_do_not_hide_outer_requiredness() {
    let mut source: serde_norway::Value = serde_norway::from_str(FIXTURE).unwrap();
    let variants = source["components"]["schemas"]["Variant"]["oneOf"]
        .as_sequence_mut()
        .unwrap();
    let required = variants
        .iter_mut()
        .find(|variant| variant["required"][0].as_str() == Some("label"))
        .unwrap();
    required["allOf"]
        .as_sequence_mut()
        .unwrap()
        .push(serde_norway::from_str("{required: [label]}").unwrap());
    let source = serde_norway::to_string(&source).unwrap();
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let ir = lower(parse_content_yaml(&source.replacen("3.1.0", version, 1)).unwrap()).unwrap();
        let fields = properties(&ir, &variant(&ir, "variant_i").content_type);
        assert!(
            fields["label"].required,
            "{version}: a repeated constraint must not erase requiredness"
        );
        assert!(!properties(&ir, &IrTypeExpr::Named("OptionalLabel".into()))["label"].required);
    }
}

#[test]
fn ordinary_reference_plus_tag_keeps_its_existing_payload_identity() {
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let ir = spec(version);
        assert_eq!(
            variant(&ir, "variant_h").content_type,
            IrTypeExpr::Named("SharedFields".into())
        );
    }
}

#[test]
fn payload_promotion_does_not_overwrite_authored_schema_names() {
    let source = FIXTURE.replace(
        "    Variant:\n",
        "    VariantFunction: {type: string}\n    VariantFunctionAllOf2: {type: integer}\n    Variant:\n",
    );
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let ir = lower(parse_content_yaml(&source.replacen("3.1.0", version, 1)).unwrap()).unwrap();
        assert!(matches!(
            ir.schemas["VariantFunction"].kind,
            IrSchemaKind::Alias(_)
        ));
        assert!(matches!(
            ir.schemas["VariantFunctionAllOf2"].kind,
            IrSchemaKind::Alias(_)
        ));
        assert_ne!(
            variant(&ir, "variant_a").content_type,
            IrTypeExpr::Named("VariantFunction".into())
        );
        assert!(properties(&ir, &variant(&ir, "variant_a").content_type).contains_key("label"));
    }
}

#[test]
fn allof_member_order_does_not_change_retained_payload_fields() {
    let mut source: serde_norway::Value = serde_norway::from_str(FIXTURE).unwrap();
    for member in source["components"]["schemas"]["Variant"]["oneOf"]
        .as_sequence_mut()
        .unwrap()
    {
        if let Some(allof) = member
            .get_mut("allOf")
            .and_then(serde_norway::Value::as_sequence_mut)
        {
            allof.reverse();
        }
    }
    let source = serde_norway::to_string(&source).unwrap();
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let before = spec(version);
        let after =
            lower(parse_content_yaml(&source.replacen("3.1.0", version, 1)).unwrap()).unwrap();
        for tag in [
            "variant_a",
            "variant_b",
            "variant_c",
            "variant_d",
            "variant_e",
            "variant_f",
            "variant_g",
            "variant_h",
        ] {
            let before = properties(&before, &variant(&before, tag).content_type);
            let after = properties(&after, &variant(&after, tag).content_type);
            assert_eq!(
                before.keys().collect::<Vec<_>>(),
                after.keys().collect::<Vec<_>>(),
                "{version}: {tag}"
            );
            for (name, field) in before {
                assert_eq!(
                    field.type_expr, after[name].type_expr,
                    "{version}: {tag}.{name}"
                );
                assert_eq!(field.required, after[name].required);
            }
        }
    }
}

#[test]
fn every_registered_generator_can_render_the_retained_payload_graph() {
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
        typescript::fetch::TypeScriptFetchCodeGenerator,
    };
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
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let ir = spec(version);
        for generator in &generators {
            let files = generator.generate(&ir).unwrap_or_else(|error| {
                panic!("{version}: {}: {error}", generator.generator_type())
            });
            assert!(!files.is_empty(), "{}", generator.generator_type());
        }
    }
}
