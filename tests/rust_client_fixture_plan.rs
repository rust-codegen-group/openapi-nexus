use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use toml::Table;

#[test]
fn rust_consumer_dependencies_match_the_fixture_plan() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/generated-clients/rust");
    let plan = fs::read_to_string(root.join("fixtures.txt")).unwrap();

    for backend in ["reqwest", "ureq", "aioduct"] {
        let mut planned = BTreeSet::new();
        for row in plan.lines().map(str::trim) {
            if row.is_empty() || row.starts_with('#') {
                continue;
            }
            let mut fields = row.split_whitespace();
            let scope = fields.next().unwrap();
            let output = fields.next().expect("fixture row must name its output");
            assert!(
                matches!(scope, "all" | "reqwest" | "ureq" | "aioduct"),
                "unknown fixture backend: {scope}"
            );
            if scope == "all" || scope == backend {
                assert!(
                    planned.insert(format!("generated/{output}")),
                    "duplicate fixture {output} for {backend}"
                );
            }
        }
        assert!(!planned.is_empty(), "no fixtures selected for {backend}");

        let cargo = fs::read_to_string(root.join(backend).join("Cargo.toml")).unwrap();
        let manifest = cargo.parse::<Table>().unwrap();
        let dependencies: BTreeSet<_> = ["dependencies", "dev-dependencies", "build-dependencies"]
            .into_iter()
            .filter_map(|section| manifest.get(section)?.as_table())
            .flat_map(|section| section.values())
            .filter_map(|entry| entry.as_table()?.get("path")?.as_str())
            .filter(|path| path.starts_with("generated/"))
            .map(str::to_owned)
            .collect();

        let missing: Vec<_> = dependencies.difference(&planned).collect();
        let unused: Vec<_> = planned.difference(&dependencies).collect();
        assert!(
            missing.is_empty() && unused.is_empty(),
            "{backend}: Cargo dependencies missing a fixture: {missing:?}; fixtures missing a Cargo dependency: {unused:?}"
        );
    }
}
