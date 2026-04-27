//! Integration tests for extra_derives config in rust-reqwest generator.

use openapi_nexus_rust_reqwest::RustReqwestCodeGenerator;
use openapi_nexus_test_utils::{generate_files, read_fixture};

fn build_config(toml_str: &str) -> toml::value::Table {
    toml::from_str::<toml::value::Table>(toml_str).unwrap()
}

fn generate_petstore(config: toml::value::Table) -> std::collections::HashMap<String, String> {
    let generator = RustReqwestCodeGenerator::new(config);
    let spec = read_fixture("valid/petstore.yaml");
    generate_files(&generator, &spec).expect("generation should succeed")
}

fn generate_union(config: toml::value::Table) -> std::collections::HashMap<String, String> {
    let generator = RustReqwestCodeGenerator::new(config);
    let spec = read_fixture("valid/type-aliases/union-with-interfaces.yaml");
    generate_files(&generator, &spec).expect("generation should succeed")
}

#[test]
fn no_extra_derives_default_output() {
    let files = generate_petstore(toml::value::Table::new());

    let pet = files.get("src/models/pet.rs").expect("pet.rs should exist");
    assert!(
        pet.contains("#[derive(Debug, Clone, Serialize, Deserialize)]"),
        "struct should have default derives, got:\n{pet}"
    );
    assert!(
        !pet.contains("PartialEq"),
        "struct should not have PartialEq by default"
    );

    let api = files.get("src/apis/pet.rs").expect("pet api should exist");
    assert!(
        api.contains("#[derive(Debug)]"),
        "response struct should have only Debug derive"
    );
}

#[test]
fn extra_derives_on_structs() {
    let files = generate_petstore(build_config(
        r#"
        [extra_derives.structs]
        derives = ["PartialEq", "Hash"]
        "#,
    ));

    let pet = files.get("src/models/pet.rs").expect("pet.rs should exist");
    assert!(
        pet.contains("#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Hash)]"),
        "struct should have extra derives appended, got:\n{pet}"
    );

    let api = files.get("src/apis/pet.rs").expect("pet api should exist");
    assert!(
        api.contains("#[derive(Debug)]"),
        "response struct should not be affected by structs config"
    );
    assert!(
        !api.contains("PartialEq"),
        "response struct should not have PartialEq from structs config"
    );
}

#[test]
fn extra_derives_on_enums() {
    let files = generate_petstore(build_config(
        r#"
        [extra_derives.enums]
        derives = ["Hash"]
        "#,
    ));

    let status = files
        .get("src/models/pet_status.rs")
        .expect("pet_status.rs should exist");
    assert!(
        status.contains("#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]"),
        "string enum should have Hash appended, got:\n{status}"
    );

    let pet = files.get("src/models/pet.rs").expect("pet.rs should exist");
    assert!(
        pet.contains("#[derive(Debug, Clone, Serialize, Deserialize)]"),
        "struct should be unaffected by enums config"
    );
}

#[test]
fn extra_derives_on_unions() {
    let files = generate_union(build_config(
        r#"
        [extra_derives.unions]
        derives = ["PartialEq"]
        "#,
    ));

    let has_union_derive = files.values().any(|content| {
        content.contains("#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]")
            && content.contains("#[serde(untagged)]")
    });
    assert!(has_union_derive, "union should have PartialEq appended");

    let struct_files: Vec<_> = files
        .iter()
        .filter(|(k, v)| {
            k.starts_with("src/models/") && *k != "src/models/mod.rs" && v.contains("pub struct")
        })
        .collect();
    for (path, content) in &struct_files {
        assert!(
            !content.contains("PartialEq"),
            "struct in {path} should not be affected by unions config"
        );
    }
}

#[test]
fn extra_derives_on_response_structs() {
    let files = generate_petstore(build_config(
        r#"
        [extra_derives.response_structs]
        derives = ["Clone", "PartialEq"]
        "#,
    ));

    let api = files.get("src/apis/pet.rs").expect("pet api should exist");
    assert!(
        api.contains("#[derive(Debug, Clone, PartialEq)]"),
        "response struct should have extra derives, got:\n{api}"
    );

    let pet = files.get("src/models/pet.rs").expect("pet.rs should exist");
    assert!(
        pet.contains("#[derive(Debug, Clone, Serialize, Deserialize)]"),
        "model struct should be unaffected by response_structs config"
    );
}

#[test]
fn extra_derives_cargo_toml_deps() {
    let files = generate_petstore(build_config(
        r#"
        [extra_derives.structs]
        derives = ["utoipa::ToSchema"]
        [extra_derives.structs.dependencies]
        utoipa = '{ version = "5", features = ["openapi_extensions"] }'
        "#,
    ));

    let cargo = files.get("Cargo.toml").expect("Cargo.toml should exist");
    assert!(
        cargo.contains("reqwest"),
        "Cargo.toml should still have reqwest dep"
    );
    assert!(
        cargo.contains(r#"utoipa = { version = "5", features = ["openapi_extensions"] }"#),
        "Cargo.toml should contain extra dependency, got:\n{cargo}"
    );
}

#[test]
fn extra_derives_all_kinds_together() {
    let files = generate_petstore(build_config(
        r#"
        [extra_derives.structs]
        derives = ["PartialEq"]
        [extra_derives.structs.dependencies]
        fake-crate = '"1.0"'
        [extra_derives.enums]
        derives = ["Hash"]
        [extra_derives.response_structs]
        derives = ["Clone"]
        "#,
    ));

    let pet = files.get("src/models/pet.rs").expect("pet.rs should exist");
    assert!(
        pet.contains("#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]"),
        "struct should have PartialEq, got:\n{pet}"
    );

    let status = files
        .get("src/models/pet_status.rs")
        .expect("pet_status.rs should exist");
    assert!(
        status.contains("Hash"),
        "enum should have Hash, got:\n{status}"
    );

    let api = files.get("src/apis/pet.rs").expect("pet api should exist");
    assert!(
        api.contains("#[derive(Debug, Clone)]"),
        "response struct should have Clone, got:\n{api}"
    );

    let cargo = files.get("Cargo.toml").expect("Cargo.toml should exist");
    assert!(
        cargo.contains(r#"fake-crate = "1.0""#),
        "Cargo.toml should have extra dep, got:\n{cargo}"
    );
}

#[test]
fn extra_derives_empty_derives_list() {
    let with_empty = generate_petstore(build_config(
        r#"
        [extra_derives.structs]
        derives = []
        "#,
    ));
    let without = generate_petstore(toml::value::Table::new());

    let pet_with = with_empty
        .get("src/models/pet.rs")
        .expect("pet.rs should exist");
    let pet_without = without
        .get("src/models/pet.rs")
        .expect("pet.rs should exist");
    assert_eq!(
        pet_with, pet_without,
        "empty derives list should produce identical output"
    );
}
