//! Source contracts through the public generator boundary.

use openapi_nexus::generators::typescript::fetch::TypeScriptFetchCodeGenerator;
use openapi_nexus::test_utils::{generate_files, read_fixture};

#[test]
fn typescript_nullable_matrices_keep_array_precedence() {
    let generator = TypeScriptFetchCodeGenerator::new(toml::value::Table::new());
    let files = generate_files(&generator, &read_fixture("valid/nested-nullable.yaml"))
        .expect("nullable matrix fixture generates");
    assert!(
        files["models/MaybeMatrix.ts"]
            .contains("export type MaybeMatrix = readonly string[][] | null;"),
        "nullable matrix alias has incorrect array syntax: {}",
        files["models/MaybeMatrix.ts"]
    );
    assert!(
        files["models/MatrixContainer.ts"].contains("readonly matrix: readonly string[][] | null;"),
        "nullable matrix field has incorrect array syntax: {}",
        files["models/MatrixContainer.ts"]
    );
}
