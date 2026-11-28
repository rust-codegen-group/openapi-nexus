//! Execute whole-package snapshot cases registered by the build script.

use std::path::Path;

use openapi_nexus::codegen::traits::{CodeGenerator, FileWriter};
use openapi_nexus::test_utils::run_golden_test;
use toml::Table;

pub fn run<G: CodeGenerator + FileWriter>(
    backend: &str,
    make_generator: impl Fn(Table) -> G,
    cases: &[(&str, &str, bool, &str)],
) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let (language, _) = backend
        .split_once('-')
        .expect("registered backend has a language");
    let golden_dir = Path::new("tests/golden").join(language).join(backend);
    let update_hint = format!(
        "UPDATE_GOLDEN=1 cargo test --test golden_tests_{}",
        backend.replace('-', "_")
    );
    for &(name, fixture, use_vp, settings) in cases {
        let (mut config, overrides) = if let Some(overrides) =
            settings.strip_prefix("@rust-union-policies\n")
        {
            let profile = root.join("tests/generated-clients/rust/union-policies.toml");
            let text = std::fs::read_to_string(&profile)
                .unwrap_or_else(|error| panic!("could not read {}: {error}", profile.display()));
            let table = text
                .parse::<Table>()
                .expect("valid Rust union policy profile");
            (
                table[backend]
                    .as_table()
                    .expect("Rust union policy profile for backend")
                    .clone(),
                overrides,
            )
        } else {
            (Table::new(), settings)
        };
        let overrides = overrides.parse::<Table>().unwrap_or_else(|error| {
            panic!("invalid generator settings for {backend}/{name}: {error}")
        });
        assert!(
            !overrides.contains_key("toolchain"),
            "declare the toolchain in the mode column: {backend}/{name}"
        );
        config.extend(overrides);
        if use_vp {
            config.insert("toolchain".into(), "vp".into());
        }
        run_golden_test(
            &make_generator(config),
            &golden_dir,
            name,
            fixture,
            &update_hint,
        );
    }
}
