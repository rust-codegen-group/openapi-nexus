//! Compile the shared golden registry into ordinary named Rust tests.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use sigil_stitch::error::SigilStitchError;
use sigil_stitch::prelude::{CodeBlock, sigil_quote};
use sigil_stitch::spec::{annotation_spec::AnnotationSpec, file_spec::FileSpec, fun_spec::FunSpec};

pub const REGISTRY: &str = "tests/golden_cases.tsv";
const BACKENDS: [&str; 9] = [
    "go-http",
    "java-okhttp",
    "kotlin-okhttp",
    "python-httpx",
    "python-requests",
    "rust-aioduct",
    "rust-reqwest",
    "rust-ureq",
    "typescript-fetch",
];

type Suites<'a> = BTreeMap<&'static str, BTreeMap<String, Vec<GoldenCase<'a>>>>;

struct GoldenCase<'a> {
    name: &'a str,
    fixture: &'a str,
    use_vp: bool,
    config: String,
}

struct Catalog<'a> {
    suites: Suites<'a>,
    fragments: BTreeMap<&'a str, &'a str>,
}

fn parse_catalog(text: &str) -> Result<Catalog<'_>, String> {
    let mut suites: Suites<'_> = BTreeMap::new();
    let mut projects = BTreeSet::new();
    let mut fragments = BTreeMap::new();
    let mut fragment_paths = BTreeSet::new();
    for (index, line) in text.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let error = |problem| format!("{REGISTRY}:{}: {problem}", index + 1);
        let mut columns = line.split('\t');
        let Some((scopes, mode, name, test, fixture, config)) = (|| {
            Some((
                columns.next()?,
                columns.next()?,
                columns.next()?,
                columns.next()?,
                columns.next()?,
                columns.next()?,
            ))
        })() else {
            return Err(error("expected six tab-separated fields"));
        };
        if columns.next().is_some() {
            return Err(error("expected six tab-separated fields"));
        }
        let targets: BTreeSet<_> = scopes.split(',').collect();
        if scopes != "all"
            && (targets.len() != scopes.split(',').count()
                || !targets.iter().all(|backend| BACKENDS.contains(backend)))
        {
            return Err(error("invalid or repeated backend selector"));
        }
        if mode == "fragment" {
            if scopes == "all"
                || !safe_relative(name)
                || !name.ends_with(".golden")
                || !valid_identifier(test)
                || fixture != "-"
                || config != "-"
                || !fragment_paths.insert(name)
                || fragments.insert(test, name).is_some()
            {
                return Err(error("invalid or duplicate fragment"));
            }
            continue;
        }
        if !matches!(mode, "native" | "tsc" | "vp")
            || !safe_relative(name)
            || name.contains(['/', '.'])
            || !safe_relative(fixture)
            || !fixture.starts_with("valid/")
            || !matches!(fixture.rsplit('.').next(), Some("yaml" | "yml" | "json"))
            || config.is_empty()
        {
            return Err(error("invalid project case"));
        }
        let test = if test == "-" {
            format!("test_{}_golden", name.replace('-', "_"))
        } else {
            test.into()
        };
        if !valid_identifier(&test) {
            return Err(error("invalid Rust test name"));
        }
        let config = if config == "-" {
            String::new()
        } else {
            config.replace("\\n", "\n")
        };
        for backend in BACKENDS
            .into_iter()
            .filter(|backend| scopes == "all" || targets.contains(backend))
        {
            if mode != "native" && backend != "typescript-fetch" {
                return Err(error("toolchain mode is only for TypeScript"));
            }
            if config.starts_with('@')
                && !(backend.starts_with("rust-") && config.starts_with("@rust-union-policies\n"))
            {
                return Err(error("invalid golden configuration profile"));
            }
            if !projects.insert((backend, name)) {
                return Err(format!(
                    "{REGISTRY}:{}: duplicate project {backend}/{name}",
                    index + 1
                ));
            }
            suites
                .entry(backend)
                .or_default()
                .entry(test.clone())
                .or_default()
                .push(GoldenCase {
                    name,
                    fixture,
                    use_vp: mode == "vp",
                    config: config.clone(),
                });
        }
    }
    for backend in BACKENDS {
        if !suites.contains_key(backend) {
            return Err(format!("{REGISTRY}: no projects registered for {backend}"));
        }
    }
    Ok(Catalog { suites, fragments })
}

fn safe_relative(path: &str) -> bool {
    path.split('/').all(|part| {
        !matches!(part, "" | "." | "..")
            && part
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    })
}

fn valid_identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z' | b'_'))
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

fn test_function(
    name: &str,
    backend: &str,
    cases: &[GoldenCase<'_>],
) -> Result<FunSpec, SigilStitchError> {
    let body = sigil_quote!(RustLang {
        support::golden_cases::run($S(backend), GoldenGenerator::new, &[
            $for(case in cases; separator = ", ", trailing = true) {
                ($S(case.name), $S(case.fixture), $if(case.use_vp) { true } $else { false }, $S(&case.config))
            }
        ]);
    })?;
    FunSpec::builder(name)
        .annotate(AnnotationSpec::new("test"))
        .annotate(AnnotationSpec::new("tracing_test::traced_test"))
        .body(body)
        .build()
}

fn fragment_macro(fragments: BTreeMap<&str, &str>) -> Result<CodeBlock, SigilStitchError> {
    // There is no macro declaration spec; quote the macro and its literal arms.
    sigil_quote!(RustLang {
        macro_rules! golden_fragment_path {
            $for((key, path) in fragments; separator = "\n") {
                ($N(key)) => { $S(format!("tests/golden/{path}")) };
            }
        }
    })
}

pub fn write_tests(manifest_dir: &Path, out_dir: &Path) {
    let registry = fs::read_to_string(manifest_dir.join(REGISTRY)).expect("read golden registry");
    let Catalog { suites, fragments } =
        parse_catalog(&registry).unwrap_or_else(|error| panic!("{error}"));
    for (backend, tests) in suites {
        let suite = backend.replace('-', "_");
        let filename = format!("golden_cases_{suite}.rs");
        let mut file = FileSpec::builder(&filename);
        for (name, cases) in tests {
            let function = test_function(&name, backend, &cases)
                .unwrap_or_else(|error| panic!("build registered test {backend}/{name}: {error}"));
            file = file.add_function(function);
        }
        write_file(
            file.build().expect("build registered golden suite"),
            out_dir,
            &filename,
        );
    }
    let filename = "golden_fragments.rs";
    let file = FileSpec::builder(filename)
        .add_code(fragment_macro(fragments).expect("build golden fragment path macro"))
        .build()
        .expect("build golden fragment file");
    write_file(file, out_dir, filename);
}

fn write_file(file: FileSpec, out_dir: &Path, filename: &str) {
    let source = file
        .render(100)
        .unwrap_or_else(|error| panic!("render {filename}: {error}"));
    fs::write(out_dir.join(filename), source)
        .unwrap_or_else(|error| panic!("write {filename}: {error}"));
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use sigil_stitch::assert_rendered;
    use sigil_stitch::lang::rust::Rust;

    use super::{fragment_macro, parse_catalog};

    #[test]
    fn fragment_paths_render_as_literal_macro_arms() {
        let block = fragment_macro(BTreeMap::from([("choice", "typescript/choice.ts.golden")]))
            .expect("build fragment macro");
        assert_rendered!(
            Rust::new(),
            width = 100,
            block,
            concat!(
                "macro_rules!golden_fragment_path {\n",
                "    (choice) => {\"tests/golden/typescript/choice.ts.golden\"};\n",
                "}\n",
            ),
        );
    }

    #[test]
    fn overlapping_backend_scopes_cannot_register_the_same_project() {
        let registry = "all\tnative\tminimal\t-\tvalid/minimal.yaml\t-\ngo-http\tnative\tminimal\tdifferent_test\tvalid/petstore.yaml\t-\n";
        let error = parse_catalog(registry).err().expect("duplicate must fail");
        assert!(
            error.contains("duplicate project go-http/minimal"),
            "{error}"
        );
    }

    #[test]
    fn native_toolchains_cannot_misclassify_a_project() {
        let registry = "go-http\tvp\tminimal\t-\tvalid/minimal.yaml\t-\n";
        let error = parse_catalog(registry)
            .err()
            .expect("invalid mode must fail");
        assert!(
            error.contains("toolchain mode is only for TypeScript"),
            "{error}"
        );
    }
}
