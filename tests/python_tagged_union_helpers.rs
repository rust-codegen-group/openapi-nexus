//! Tests that Python tagged union types generate discriminator-aware
//! from_dict/to_dict helper functions for all tagging styles.

use openapi_nexus::generators::python::httpx::PythonHttpxCodeGenerator;
use openapi_nexus::test_utils::{generate_files, read_fixture};

fn generate_enum_repr(config: toml::value::Table) -> std::collections::HashMap<String, String> {
    let generator = PythonHttpxCodeGenerator::new(config);
    let spec = read_fixture("valid/enum-repr/enum-repr.yaml");
    generate_files(&generator, &spec).expect("generation should succeed")
}

fn generate_discriminated_union_internally_tagged(
    config: toml::value::Table,
) -> std::collections::HashMap<String, String> {
    let generator = PythonHttpxCodeGenerator::new(config);
    let spec = read_fixture("valid/type-aliases/discriminated-union-internally-tagged.yaml");
    generate_files(&generator, &spec).expect("generation should succeed")
}

fn generate_discriminated_union_with_refs(
    config: toml::value::Table,
) -> std::collections::HashMap<String, String> {
    let generator = PythonHttpxCodeGenerator::new(config);
    let spec = read_fixture("valid/type-aliases/discriminated-union-with-refs.yaml");
    generate_files(&generator, &spec).expect("generation should succeed")
}

// ---------------------------------------------------------------------------
// Internally tagged: from_dict dispatches on tag, to_dict injects tag
// ---------------------------------------------------------------------------

#[test]
fn internally_tagged_has_from_dict_helper() {
    let files = generate_enum_repr(toml::value::Table::new());
    let content = files
        .get("enum_representation_api/models/internally_tagged_enum.py")
        .expect("internally_tagged_enum.py should exist");

    assert!(
        content.contains("def internally_tagged_enum_from_dict(data: dict[str, object])"),
        "should have from_dict helper, got:\n{content}"
    );
    assert!(
        content.contains("_tag = data[\"ty\"]"),
        "from_dict should read the discriminator field, got:\n{content}"
    );
    assert!(
        content.contains("return VariantA.from_dict(data)"),
        "from_dict should dispatch to VariantA.from_dict, got:\n{content}"
    );
}

#[test]
fn internally_tagged_has_to_dict_helper() {
    let files = generate_enum_repr(toml::value::Table::new());
    let content = files
        .get("enum_representation_api/models/internally_tagged_enum.py")
        .expect("internally_tagged_enum.py should exist");

    assert!(
        content.contains("def internally_tagged_enum_to_dict(obj: InternallyTaggedEnum)"),
        "should have to_dict helper, got:\n{content}"
    );
    assert!(
        content.contains("result[\"ty\"] = \"INTERNALLY_TAGGED_ENUM_VARIANT_A\""),
        "to_dict should inject discriminator value, got:\n{content}"
    );
    assert!(
        content.contains("result = obj.to_dict()"),
        "to_dict should call obj.to_dict() first, got:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// Adjacently tagged: from_dict reads content field, to_dict wraps in envelope
// ---------------------------------------------------------------------------

#[test]
fn adjacently_tagged_from_dict_reads_content_field() {
    let files = generate_enum_repr(toml::value::Table::new());
    let content = files
        .get("enum_representation_api/models/adjacently_tagged_enum.py")
        .expect("adjacently_tagged_enum.py should exist");

    assert!(
        content.contains("def adjacently_tagged_enum_from_dict(data: dict[str, object])"),
        "should have from_dict helper, got:\n{content}"
    );
    assert!(
        content.contains("_tag = data[\"ty\"]"),
        "from_dict should read the tag field, got:\n{content}"
    );
    assert!(
        content.contains("_content = data[\"data\"]"),
        "from_dict should read the content field, got:\n{content}"
    );
    assert!(
        content.contains("return VariantA.from_dict(_content)"),
        "from_dict should pass content to variant's from_dict, got:\n{content}"
    );
}

#[test]
fn adjacently_tagged_to_dict_produces_envelope() {
    let files = generate_enum_repr(toml::value::Table::new());
    let content = files
        .get("enum_representation_api/models/adjacently_tagged_enum.py")
        .expect("adjacently_tagged_enum.py should exist");

    assert!(
        content.contains("def adjacently_tagged_enum_to_dict(obj: AdjacentlyTaggedEnum)"),
        "should have to_dict helper, got:\n{content}"
    );
    assert!(
        content.contains("{\"ty\": \"ADJACENTLY_TAGGED_ENUM_VARIANT_A\", \"data\": obj.to_dict()}"),
        "to_dict should produce tag+content envelope, got:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// Externally tagged: from_dict checks key presence, to_dict wraps in key
// ---------------------------------------------------------------------------

#[test]
fn externally_tagged_from_dict_checks_key() {
    let files = generate_enum_repr(toml::value::Table::new());
    let content = files
        .get("enum_representation_api/models/externally_tagged_enum.py")
        .expect("externally_tagged_enum.py should exist");

    assert!(
        content.contains("def externally_tagged_enum_from_dict(data: dict[str, object])"),
        "should have from_dict helper, got:\n{content}"
    );
    assert!(
        content.contains("\"EXTERNALLY_TAGGED_ENUM_VARIANT_A\" in data"),
        "from_dict should check key presence, got:\n{content}"
    );
    assert!(
        content.contains("VariantA.from_dict(data[\"EXTERNALLY_TAGGED_ENUM_VARIANT_A\"])"),
        "from_dict should pass inner value to variant's from_dict, got:\n{content}"
    );
}

#[test]
fn externally_tagged_to_dict_wraps_in_key() {
    let files = generate_enum_repr(toml::value::Table::new());
    let content = files
        .get("enum_representation_api/models/externally_tagged_enum.py")
        .expect("externally_tagged_enum.py should exist");

    assert!(
        content.contains("def externally_tagged_enum_to_dict(obj: ExternallyTaggedEnum)"),
        "should have to_dict helper, got:\n{content}"
    );
    assert!(
        content.contains("{\"EXTERNALLY_TAGGED_ENUM_VARIANT_A\": obj.to_dict()}"),
        "to_dict should wrap in variant key, got:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// Discriminated union fixture: internally tagged with serde rename on fields
// ---------------------------------------------------------------------------

#[test]
fn discriminated_union_resource_has_helpers() {
    let files = generate_discriminated_union_internally_tagged(toml::value::Table::new());
    let content = files
        .get("discriminated_union_internally_tagged_test/models/resource.py")
        .expect("resource.py should exist");

    assert!(
        content.contains("def resource_from_dict(data: dict[str, object]) -> Resource:"),
        "should have from_dict helper, got:\n{content}"
    );
    assert!(
        content.contains("_tag = data[\"kind\"]"),
        "from_dict should read 'kind' discriminator, got:\n{content}"
    );
    assert!(
        content.contains("_tag == \"RESOURCE_QUICK\""),
        "from_dict should check RESOURCE_QUICK, got:\n{content}"
    );
    assert!(
        content.contains("def resource_to_dict(obj: Resource) -> dict[str, object]:"),
        "should have to_dict helper, got:\n{content}"
    );
    assert!(
        content.contains("result[\"kind\"] = \"RESOURCE_QUICK\""),
        "to_dict should inject kind=RESOURCE_QUICK, got:\n{content}"
    );
}

#[test]
fn discriminated_union_with_refs_has_helpers() {
    let files = generate_discriminated_union_with_refs(toml::value::Table::new());
    let content = files
        .get("discriminated_union_with_references_test/models/container_image.py")
        .expect("container_image.py should exist");

    assert!(
        content.contains("def container_image_from_dict("),
        "should have from_dict helper, got:\n{content}"
    );
    assert!(
        content.contains("def container_image_to_dict("),
        "should have to_dict helper, got:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// Error handling
// ---------------------------------------------------------------------------

#[test]
fn from_dict_raises_on_unknown_discriminator() {
    let files = generate_discriminated_union_internally_tagged(toml::value::Table::new());
    let content = files
        .get("discriminated_union_internally_tagged_test/models/resource.py")
        .expect("resource.py should exist");

    assert!(
        content.contains("raise ValueError("),
        "from_dict should raise ValueError on unknown discriminator, got:\n{content}"
    );
}

#[test]
fn to_dict_raises_on_unknown_variant() {
    let files = generate_discriminated_union_internally_tagged(toml::value::Table::new());
    let content = files
        .get("discriminated_union_internally_tagged_test/models/resource.py")
        .expect("resource.py should exist");

    assert!(
        content.contains("raise ValueError(f\"Unknown variant for Resource:"),
        "to_dict should raise ValueError on unknown variant type, got:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// Variant dataclasses remain unchanged (no discriminator field injected)
// ---------------------------------------------------------------------------

#[test]
fn variant_dataclass_unchanged_no_discriminator_field() {
    let files = generate_enum_repr(toml::value::Table::new());
    let content = files
        .get("enum_representation_api/models/variant_a.py")
        .expect("variant_a.py should exist");

    assert!(
        !content.contains("ty:") && !content.contains("\"ty\""),
        "variant dataclass should NOT have discriminator field 'ty', got:\n{content}"
    );
    assert!(
        content.contains("field1: str"),
        "variant should still have its own fields, got:\n{content}"
    );
    assert!(
        content.contains("field2: int"),
        "variant should still have its own fields, got:\n{content}"
    );
}
