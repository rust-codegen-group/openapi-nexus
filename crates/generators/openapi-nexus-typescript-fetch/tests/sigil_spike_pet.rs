//! Phase 2 spike: render `petstore#Pet` through the sigil-stitch emit path and
//! assert it produces the shape documented in `docs/target-output-spec.md`.
//!
//! This is a smoke test, not a golden test. It deliberately pins exact substrings
//! so that sigil-stitch output regressions fail loudly.

use std::fs;
use std::path::Path;

use openapi_nexus_ir::types::IrSchemaKind;
use openapi_nexus_typescript_fetch::sigil_emit::emit_model_file;

fn read_fixture(rel: &str) -> String {
    for base in [
        "tests/fixtures",
        "../tests/fixtures",
        "../../tests/fixtures",
        "../../../tests/fixtures",
    ] {
        let p = Path::new(base).join(rel);
        if p.exists() {
            return fs::read_to_string(p).unwrap();
        }
    }
    panic!("fixture not found: {rel}");
}

#[test]
fn pet_model_renders_expected_shape() {
    let yaml = read_fixture("valid/petstore.yaml");
    let parsed = openapi_nexus_parser::parse_content_yaml(&yaml).unwrap();
    let ir = openapi_nexus_ir::lower::lower(parsed).unwrap();

    let pet = ir.schemas.get("Pet").expect("Pet schema in IR");
    assert!(
        matches!(pet.kind, IrSchemaKind::Object(_)),
        "Pet must lower to Object, got {:?}",
        pet.kind
    );

    let file = emit_model_file(pet).expect("spike supports Object");
    let rendered = file.render(100).expect("renders");

    println!("--- rendered Pet.ts ---\n{rendered}\n--- end ---");

    let required_fragments = [
        // type-only imports for ref'd models
        "import type { Category } from './Category'",
        "import type { PetStatus } from './PetStatus'",
        "import type { Tag } from './Tag'",
        // interface header
        "export interface Pet",
        // nullable optional ref
        "readonly category?: Category | null",
        // nullable optional primitive
        "readonly id?: number | null",
        // required primitive (no ? no | null)
        "readonly name: string",
        // required array of primitives, with element-level readonly
        "readonly photoUrls: readonly string[]",
        // nullable optional ref (enum)
        "readonly status?: PetStatus | null",
        // nullable optional array of refs, with element-level readonly
        "readonly tags?: readonly Tag[] | null",
    ];
    for frag in required_fragments {
        assert!(
            rendered.contains(frag),
            "expected fragment `{frag}` in rendered output:\n{rendered}"
        );
    }

    // `name` is required + non-null, so it must NOT have `?` or `| null`.
    let name_line = rendered
        .lines()
        .find(|l| l.contains("readonly name"))
        .expect("name line");
    assert!(
        !name_line.contains('?') && !name_line.contains("| null"),
        "required non-null field should be bare: `{name_line}`"
    );
}
