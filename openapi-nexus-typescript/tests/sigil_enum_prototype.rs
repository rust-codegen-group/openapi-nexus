//! Prototype: render an IR `Enum` schema through `emit_enum_file` and print the
//! result. Not a golden test — we're evaluating whether `sigil_quote!` is worth
//! adopting for enum emission.
//!
//! Run with:
//!   cargo test -p openapi-nexus-typescript --test sigil_enum_prototype -- --nocapture

use std::fs;
use std::path::Path;

use openapi_nexus_typescript::sigil_emit::emit_enum_file;

fn read_fixture(rel: &str) -> String {
    for base in ["tests/fixtures", "../tests/fixtures", "../../tests/fixtures"] {
        let p = Path::new(base).join(rel);
        if p.exists() {
            return fs::read_to_string(p).unwrap();
        }
    }
    panic!("fixture not found: {rel}");
}

#[test]
fn foo_type_enum_renders_as_union_alias() {
    let yaml = read_fixture("valid/interface-with-enum-reference.yaml");
    let parsed = openapi_nexus_parser::parse_content_yaml(&yaml).unwrap();
    let ir = openapi_nexus_ir::lower::lower(parsed).unwrap();

    let foo = ir.schemas.get("FooType").expect("FooType schema in IR");
    let file = emit_enum_file(foo).expect("prototype supports string Enum");
    let rendered = file.render(100).expect("renders");

    println!("--- rendered FooType.ts ---\n{rendered}\n--- end ---");

    assert!(
        rendered.contains("export type FooType = 'foo' | 'bar' | 'baz';"),
        "expected union alias, got:\n{rendered}"
    );
}
