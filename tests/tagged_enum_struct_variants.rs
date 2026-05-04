//! Tests that internally/adjacently tagged enums emit struct variants
//! (inlined fields) while externally tagged and untagged enums keep
//! tuple variants. This is required for utoipa::ToSchema and
//! schemars::JsonSchema derive compatibility.

use openapi_nexus::generators::rust::reqwest::RustReqwestCodeGenerator;
use openapi_nexus::test_utils::{generate_files, read_fixture};

fn generate_enum_repr(config: toml::value::Table) -> std::collections::HashMap<String, String> {
    let generator = RustReqwestCodeGenerator::new(config);
    let spec = read_fixture("valid/enum-repr/enum-repr.yaml");
    generate_files(&generator, &spec).expect("generation should succeed")
}

fn generate_discriminated_union_internally_tagged(
    config: toml::value::Table,
) -> std::collections::HashMap<String, String> {
    let generator = RustReqwestCodeGenerator::new(config);
    let spec = read_fixture("valid/type-aliases/discriminated-union-internally-tagged.yaml");
    generate_files(&generator, &spec).expect("generation should succeed")
}

fn generate_discriminated_union_with_refs(
    config: toml::value::Table,
) -> std::collections::HashMap<String, String> {
    let generator = RustReqwestCodeGenerator::new(config);
    let spec = read_fixture("valid/type-aliases/discriminated-union-with-refs.yaml");
    generate_files(&generator, &spec).expect("generation should succeed")
}

fn generate_discriminated_union_multiple(
    config: toml::value::Table,
) -> std::collections::HashMap<String, String> {
    let generator = RustReqwestCodeGenerator::new(config);
    let spec = read_fixture("valid/type-aliases/discriminated-union-multiple.yaml");
    generate_files(&generator, &spec).expect("generation should succeed")
}

// ---------------------------------------------------------------------------
// Internally tagged: struct variants with inlined fields
// ---------------------------------------------------------------------------

#[test]
fn internally_tagged_enum_uses_struct_variants() {
    let files = generate_enum_repr(toml::value::Table::new());
    let content = files
        .get("src/models/internally_tagged_enum.rs")
        .expect("internally_tagged_enum.rs should exist");

    assert!(
        content.contains("#[serde(tag = \"ty\")]"),
        "should have internal tag attribute, got:\n{content}"
    );

    assert!(
        content.contains("InternallyTaggedEnumVariantA {"),
        "variant A should be a struct variant (opening brace), got:\n{content}"
    );
    assert!(
        content.contains("InternallyTaggedEnumVariantB {"),
        "variant B should be a struct variant (opening brace), got:\n{content}"
    );

    assert!(
        !content.contains("InternallyTaggedEnumVariantA("),
        "variant A should NOT be a tuple variant, got:\n{content}"
    );
    assert!(
        !content.contains("InternallyTaggedEnumVariantB("),
        "variant B should NOT be a tuple variant, got:\n{content}"
    );
}

#[test]
fn internally_tagged_struct_variant_fields_inlined() {
    let files = generate_enum_repr(toml::value::Table::new());
    let content = files
        .get("src/models/internally_tagged_enum.rs")
        .expect("internally_tagged_enum.rs should exist");

    assert!(
        content.contains("field1: String"),
        "VariantA's field1 should be inlined, got:\n{content}"
    );
    assert!(
        content.contains("field2: i32"),
        "VariantA's field2 should be inlined, got:\n{content}"
    );
    assert!(
        content.contains("field3: bool"),
        "VariantB's field3 should be inlined, got:\n{content}"
    );
    assert!(
        content.contains("field4: f64"),
        "VariantB's field4 should be inlined, got:\n{content}"
    );
}

#[test]
fn internally_tagged_fields_have_no_pub() {
    let files = generate_enum_repr(toml::value::Table::new());
    let content = files
        .get("src/models/internally_tagged_enum.rs")
        .expect("internally_tagged_enum.rs should exist");

    assert!(
        !content.contains("pub field"),
        "enum variant fields must not have pub visibility, got:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// Adjacently tagged: struct variants with inlined fields
// ---------------------------------------------------------------------------

#[test]
fn adjacently_tagged_enum_uses_struct_variants() {
    let files = generate_enum_repr(toml::value::Table::new());
    let content = files
        .get("src/models/adjacently_tagged_enum.rs")
        .expect("adjacently_tagged_enum.rs should exist");

    assert!(
        content.contains("#[serde(tag = \"ty\", content = \"data\")]"),
        "should have adjacent tag attribute, got:\n{content}"
    );

    assert!(
        content.contains("AdjacentlyTaggedEnumVariantA {"),
        "variant A should be a struct variant, got:\n{content}"
    );
    assert!(
        content.contains("AdjacentlyTaggedEnumVariantB {"),
        "variant B should be a struct variant, got:\n{content}"
    );

    assert!(
        !content.contains("AdjacentlyTaggedEnumVariantA("),
        "variant A should NOT be a tuple variant, got:\n{content}"
    );
}

#[test]
fn adjacently_tagged_struct_variant_fields_inlined() {
    let files = generate_enum_repr(toml::value::Table::new());
    let content = files
        .get("src/models/adjacently_tagged_enum.rs")
        .expect("adjacently_tagged_enum.rs should exist");

    assert!(
        content.contains("field1: String"),
        "VariantA's field1 should be inlined, got:\n{content}"
    );
    assert!(
        content.contains("field2: i32"),
        "VariantA's field2 should be inlined, got:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// Externally tagged: tuple variants (unchanged)
// ---------------------------------------------------------------------------

#[test]
fn externally_tagged_enum_uses_tuple_variants() {
    let files = generate_enum_repr(toml::value::Table::new());
    let content = files
        .get("src/models/externally_tagged_enum.rs")
        .expect("externally_tagged_enum.rs should exist");

    assert!(
        content.contains("ExternallyTaggedEnumVariantA(super::VariantA)"),
        "variant A should be a tuple variant wrapping super::VariantA, got:\n{content}"
    );
    assert!(
        content.contains("ExternallyTaggedEnumVariantB(super::VariantB)"),
        "variant B should be a tuple variant wrapping super::VariantB, got:\n{content}"
    );

    assert!(
        !content.contains("ExternallyTaggedEnumVariantA {"),
        "variant A should NOT be a struct variant, got:\n{content}"
    );
}

#[test]
fn externally_tagged_enum_has_no_serde_tag() {
    let files = generate_enum_repr(toml::value::Table::new());
    let content = files
        .get("src/models/externally_tagged_enum.rs")
        .expect("externally_tagged_enum.rs should exist");

    assert!(
        !content.contains("#[serde(tag"),
        "externally tagged enum should have no serde tag attribute, got:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// Untagged: tuple variants (unchanged)
// ---------------------------------------------------------------------------

#[test]
fn untagged_enum_uses_tuple_variants() {
    let files = generate_enum_repr(toml::value::Table::new());
    let content = files
        .get("src/models/untagged_enum.rs")
        .expect("untagged_enum.rs should exist");

    assert!(
        content.contains("#[serde(untagged)]"),
        "should have untagged attribute, got:\n{content}"
    );
    assert!(
        content.contains("VariantA(super::VariantA)"),
        "untagged variant should be a tuple variant, got:\n{content}"
    );
    assert!(
        content.contains("VariantB(super::VariantB)"),
        "untagged variant should be a tuple variant, got:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// Discriminated union (internally tagged with refs): struct variants
// ---------------------------------------------------------------------------

#[test]
fn discriminated_union_internally_tagged_uses_struct_variants() {
    let files = generate_discriminated_union_internally_tagged(toml::value::Table::new());
    let content = files
        .get("src/models/resource.rs")
        .expect("resource.rs should exist");

    assert!(
        content.contains("#[serde(tag = \"kind\")]"),
        "should have internal tag attribute, got:\n{content}"
    );

    assert!(
        content.contains("ResourceUnspecified {"),
        "ResourceUnspecified should be a struct variant, got:\n{content}"
    );
    assert!(
        content.contains("ResourceQuick {"),
        "ResourceQuick should be a struct variant, got:\n{content}"
    );
    assert!(
        content.contains("ResourceCustom {"),
        "ResourceCustom should be a struct variant, got:\n{content}"
    );

    assert!(
        !content.contains("ResourceUnspecified("),
        "ResourceUnspecified should NOT be a tuple variant, got:\n{content}"
    );
}

#[test]
fn discriminated_union_struct_variant_has_serde_rename_on_fields() {
    let files = generate_discriminated_union_internally_tagged(toml::value::Table::new());
    let content = files
        .get("src/models/resource.rs")
        .expect("resource.rs should exist");

    assert!(
        content.contains("#[serde(rename = \"quickField\")]"),
        "camelCase fields should have serde rename, got:\n{content}"
    );
    assert!(
        content.contains("quick_field: String"),
        "field should use snake_case name, got:\n{content}"
    );
}

#[test]
fn discriminated_union_with_refs_uses_struct_variants() {
    let files = generate_discriminated_union_with_refs(toml::value::Table::new());
    let content = files
        .get("src/models/container_image.rs")
        .expect("container_image.rs should exist");

    assert!(
        content.contains("ContainerImageManaged {"),
        "ContainerImageManaged should be a struct variant, got:\n{content}"
    );
    assert!(
        content.contains("ContainerImageCustom {"),
        "ContainerImageCustom should be a struct variant, got:\n{content}"
    );
}

#[test]
fn discriminated_union_multiple_uses_struct_variants() {
    let files = generate_discriminated_union_multiple(toml::value::Table::new());

    let container = files
        .get("src/models/container_kind.rs")
        .expect("container_kind.rs should exist");
    assert!(
        container.contains("#[serde(tag = \"kind\")]"),
        "ContainerKind should be internally tagged, got:\n{container}"
    );
    assert!(
        !container.contains("(super::"),
        "ContainerKind should not have tuple variants, got:\n{container}"
    );

    let volume = files
        .get("src/models/volume_kind.rs")
        .expect("volume_kind.rs should exist");
    assert!(
        volume.contains("#[serde(tag = \"kind\")]"),
        "VolumeKind should be internally tagged, got:\n{volume}"
    );
    assert!(
        !volume.contains("(super::"),
        "VolumeKind should not have tuple variants, got:\n{volume}"
    );
}

// ---------------------------------------------------------------------------
// Extra derives applied to tagged union enums with struct variants
// ---------------------------------------------------------------------------

fn build_config(toml_str: &str) -> toml::value::Table {
    toml::from_str::<toml::value::Table>(toml_str).unwrap()
}

#[test]
fn extra_derives_on_internally_tagged_enum() {
    let config = build_config(
        r#"
        [extra_derives.unions]
        derives = ["PartialEq"]
        "#,
    );
    let files = generate_enum_repr(config);
    let content = files
        .get("src/models/internally_tagged_enum.rs")
        .expect("internally_tagged_enum.rs should exist");

    assert!(
        content.contains("#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]"),
        "internally tagged enum should have extra derive, got:\n{content}"
    );
}

#[test]
fn extra_derives_on_adjacently_tagged_enum() {
    let config = build_config(
        r#"
        [extra_derives.unions]
        derives = ["PartialEq"]
        "#,
    );
    let files = generate_enum_repr(config);
    let content = files
        .get("src/models/adjacently_tagged_enum.rs")
        .expect("adjacently_tagged_enum.rs should exist");

    assert!(
        content.contains("#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]"),
        "adjacently tagged enum should have extra derive, got:\n{content}"
    );
}

#[test]
fn extra_derives_on_externally_tagged_enum() {
    let config = build_config(
        r#"
        [extra_derives.unions]
        derives = ["PartialEq"]
        "#,
    );
    let files = generate_enum_repr(config);
    let content = files
        .get("src/models/externally_tagged_enum.rs")
        .expect("externally_tagged_enum.rs should exist");

    assert!(
        content.contains("#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]"),
        "externally tagged enum should also get unions extra derive, got:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// Standalone variant structs are still generated alongside inlined variants
// ---------------------------------------------------------------------------

#[test]
fn variant_structs_still_generated_for_internally_tagged() {
    let files = generate_enum_repr(toml::value::Table::new());

    assert!(
        files.contains_key("src/models/variant_a.rs"),
        "variant_a.rs should still be generated as standalone struct"
    );
    assert!(
        files.contains_key("src/models/variant_b.rs"),
        "variant_b.rs should still be generated as standalone struct"
    );
}
