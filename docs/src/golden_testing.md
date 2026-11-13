# Golden Testing

Golden tests are the primary correctness mechanism for openapi-nexus. They ensure that generated output is byte-for-byte reproducible and that it compiles in the target language.

## How It Works

There are two layers:

### Layer 1: Snapshot comparison (Rust)

Each generator has a test file that:
1. Reads an OpenAPI fixture from `tests/fixtures/valid/`
2. Runs it through the full pipeline (parse, lower, generate)
3. Compares each generated file byte-for-byte against a `.golden` file

```
tests/
├── fixtures/valid/                  Input OpenAPI specs
├── golden/
│   ├── typescript/typescript-fetch/  Expected TypeScript output per fixture
│   ├── go/go-http/                   Expected Go output per fixture
│   ├── rust/rust-reqwest/            Expected Rust (reqwest) output
│   ├── rust/rust-ureq/               Expected Rust (ureq) output
│   ├── rust/rust-aioduct/            Expected Rust (aioduct) output
│   ├── python/python-httpx/          Expected Python (httpx) output
│   ├── python/python-requests/       Expected Python (requests) output
│   ├── java/java-okhttp/             Expected Java output
│   └── kotlin/kotlin-okhttp/         Expected Kotlin output
```

Each golden directory contains files with a `.golden` suffix:

```
tests/golden/go/go-http/petstore/
├── README.md.golden
├── go.mod.golden
├── apis/pets_api.go.golden
├── models/pet.go.golden
├── runtime/auth.go.golden
├── runtime/client.go.golden
└── runtime/errors.go.golden
```

### Layer 2: Compile check

CI materializes each golden directory into a temp folder (stripping the `.golden` suffix) and runs the target language's compiler:

| Language | Command | Marker file |
|----------|---------|-------------|
| TypeScript | `vp check && vp pack && vp check` or `tsc --noEmit` (see below) | `tsconfig.json.golden` |
| Go | `go build ./...` | `go.mod.golden` |
| Rust | `cargo check` | `Cargo.toml.golden` |
| Python | `pyright` | `pyproject.toml.golden` |
| Java | `gradle compileJava` | `build.gradle.golden` |
| Kotlin | `gradle compileKotlin` | `build.gradle.kts.golden` |

This catches type errors that snapshot comparison alone cannot.

### TypeScript: `vp pack` vs `tsc --noEmit`

The TypeScript golden build script selects the toolchain per-test based on whether
`vite.config.ts.golden` exists in the test directory:

- **With `vite.config.ts`** — runs `vp install && vp check && vp pack && vp check`. This is the `toolchain = "vp"` path: installs deps, checks the original generated files for formatting and type-aware linting/types, then builds the library bundle. The second check verifies that formatting still passes after packing; generated `dist/` output is excluded from formatting.
- **Without `vite.config.ts`** — runs `tsc --noEmit`. This is the `toolchain = "tsc"` path: type-checks only.

Both paths are always exercised in CI. Both `vp` and `tsc` must be available.

`vp pack` is library mode (uses `pack.entry` from `vite.config.ts`, emits `.mjs` + `.d.mts`).
`vp build` is app mode (expects `index.html`). Generated packages are libraries, never apps.

## Running Golden Tests

```bash
# Run all snapshot tests
cargo test

# Run specific generator's golden tests
cargo test --test golden_tests_typescript_fetch
cargo test --test golden_tests_go_http
cargo test --test golden_tests_rust_reqwest
cargo test --test golden_tests_rust_ureq
cargo test --test golden_tests_rust_aioduct
cargo test --test golden_tests_python_httpx
cargo test --test golden_tests_python_requests
cargo test --test golden_tests_java_okhttp
cargo test --test golden_tests_kotlin_okhttp

# Run a single test by name
cargo test --test golden_tests_go_http -- minimal
```

## Updating Golden Files

When you intentionally change generator output:

```bash
# Update all
UPDATE_GOLDEN=1 cargo test

# Update one generator
UPDATE_GOLDEN=1 cargo test --test golden_tests_typescript_fetch
UPDATE_GOLDEN=1 cargo test --test golden_tests_rust_reqwest
```

After updating, verify the compile check passes:

```bash
just golden-typescript::build
just golden-go::build
just golden-rust::build
just golden-python::build
just golden-java::build
just golden-kotlin::build
just golden-build-all           # all languages
```

## TypeScript Client Runtime Tests

```bash
just golden-typescript-clients::test
```

This generates fresh preserve/camelCase clients, validates, and packs
them with Vite+, checks their packed declarations with strict TypeScript, and
runs ordinary Node tests against the real bundle. Full additional-property payload equality,
typed dictionary conversion, allOf and search-event composition, renamed-key
collisions, optional-field absence, and prototype-sensitive keys are covered.
The preserve client supplies type-contract coverage, not a second runtime
conversion path (it does not emit converters).

Generation, compilation, and execution are separate just recipes. Builds run in
temporary directories so Vite+ does not skip gitignored generated sources; no
generated manifests or source files receive compatibility patches or formatter
rewrites. Full `vp check` runs on the original generated output, then runs again
after packing. The consumer type check uses the generated package's installed
compiler rather than the global `tsc` selection.
The current global Vite+ CLI also needs TypeScript installed alongside the CLI
for declaration packing (`npm install -g typescript@7.0.2 vite-plus@1.6.0`). CI
reuses that bootstrap and its generator cache in the existing TypeScript/Vite+
golden-build job; no separate consumer job is added. `just ci` includes these checks too. See
`tests/generated-clients/typescript/README.md` for details.

The `tagged-allof-payloads` fixture has recurring golden compile coverage for
TypeScript, Go, all three Rust transports, Java and Kotlin. Native TypeScript
consumers also check both conversion directions, nested sibling fields and outer
requiredness. The TypeScript-specific `overlapping-allof` fixture checks recursive
object views, arrays/maps, null/optional values, opaque JSON and authored-name
collisions. These are not cross-language wire-round-trip guarantees: Kotlin's
existing allOf wire representation is unchanged. The retained recursive Python
fixture still exposes a separately tracked unresolved-symbol defect and is not
registered as a passing Python compile case; its render coverage and existing
Python golden/consumer suites remain distinct checks.

The `optional-tool-tags` fixture covers inferred internal tags whose constraint is
optional on one branch and a referenced string enum with multiple legal values on
another. Its TypeScript consumer checks omitted and explicit custom tags plus
both dated search tags in both JSON directions, including strict declaration and
type-guard checks. The shared IR preserves these facts for all OpenAPI versions;
the current runtime regression is intentionally scoped to the TypeScript target.

## Rust Client Runtime Tests

Generated Rust clients also have standalone consumer test projects under
`tests/generated-clients/rust/`. Unlike golden comparison, these tests call real
generated clients over local HTTP and check decoding, error metadata, response
dispatch, and strict non-nullable controls.

```bash
just golden-rust::test          # reqwest, ureq, and aioduct
just golden-rust::test-reqwest  # one backend
```

Preparation runs the public generator CLI against the fixtures. Each consumer
project has its own checked-in manifest, lockfile, and ordinary Rust test files;
generated clients are unmodified path dependencies in an ignored directory. One
Cargo invocation runs all cases for a backend, with a persistent dependency cache.

These projects are intentionally outside the generator workspace. `just test-all`
and generator coverage do not recursively build client crates. CI has a separate
Rust Client Tests matrix, and `just ci` includes these tests. They complement,
rather than replace, snapshot comparison and the all-golden compile checks.

See [the consumer test README](https://github.com/rust-codegen-group/openapi-nexus/tree/main/tests/generated-clients/rust)
for direct Cargo/IDE use and the retained coverage per fixture.

## JVM Client Runtime Tests

Java and Kotlin runtime checks are standalone Gradle consumer builds under
`tests/generated-clients/jvm/`. `just golden-jvm::test` prepares fresh generated
dependencies and runs normal JUnit tests, one Gradle invocation per backend.
Generated manifests are never patched. Each fixture has an isolated consumer
classpath, and both backends assert union precedence and complete payload
round-trips, not only wrapper identity or discriminator substrings.

There are no JVM build subprocesses or global directory locks in the Rust test
suite. The independent JVM Client Tests CI matrix and `just ci` run these checks.

## Native Python Union Consumers

`just golden-python-clients::test httpx` and
`just golden-python-clients::test requests` generate fresh clients for
`firstMatch`, `strict`, and `raw`, run strict pyright on the unchanged files, then
run ordinary `unittest` consumers. The same contracts cover dataclasses, nested
containers, aliases, finite values, recursion, JSON/multipart transport encoding,
tagged payloads, nulls, and lazy error decoding. In-process HTTP transports avoid
network IO without bypassing each library's request encoder. Native tests do not
launch generation, type checkers, or other runners.

The same `just golden-python::build-httpx` / `build-requests` entrypoints run all
golden checks, fresh public-declaration checks and native consumers, both locally
and in CI. `golden-build-all` and `just ci` inherit this complete validation; CI
does not append a Python-specific test step. Dependencies are installed once in
a temporary environment created with the active Python (locally 3.15.0, CI `3.x`),
and the same checker and interpreter are used throughout the backend's checks.
The generated SDK's minimum and explicit pyright target remain 3.12, a static contract,
not a claim that the native tests execute on the minimum runtime. See the
[Python configuration](python_config.md) for selection-policy semantics.

## Validation Contracts and Toolchains

- Missing fixture files fail their parser tests. Nested fixture paths are relative
  to the fixture tree exactly once; an empty discovery produces a failing test,
  not a green placeholder. Known malformed upstream documents are executed as
  explicit expected parse failures in `test_fixture_rejections.txt`. Missing
  files or read errors cannot satisfy those rejection expectations.
- Every golden directory must have its build manifest. Native validators reject
  empty selections and missing manifests instead of reporting `0/0 passed`.
- Rust golden builds retain their cache under `target/golden-build/rust-<backend>`
  or the explicitly configured `CARGO_TARGET_DIR`. CI caches the same directory.
- Python prepares dependencies and resolves pyright once per backend, sharing
  that environment across goldens, declaration checks and runtime consumers.
  `PYTHON` selects the interpreter used to create the temporary environment;
  the active `pyright` is used when available, otherwise npm resolves it once.
- Development and ordinary CI validation select current Rust `stable` (locally
  validated with Rust 1.100.0). MSRV checks explicitly select Rust 1.90.0; they do
  not substitute for stable validation.
- Local TypeScript validation uses TypeScript 7.0.2 and Vite+ 1.6.0 on Node
  26.13.1, resolving Vite 8.3.7, Rolldown 1.2.18 and Oxfmt 0.78.0. The native
  validators use the active `tsc` and `vp` on `PATH`. To select
  that compiler explicitly, `npm exec --package=typescript@7.0.2 -- just
  golden-typescript::build-tsc` supplies it without changing generated packages.
- Current local JVM validation uses Homebrew JDK 27.0.1 with Gradle 9.8.2, respecting
  `JAVA_HOME` or the active JVM. CI validates on JDK 27. Generated libraries may
  still declare a Java 17 compile target, independently of the consumer test JVM.

Fixture discovery is tested in-process through the same build-support module
used by `build.rs`. Cargo compiles and executes the registered parser tests
normally; no test harness launches a compiler, build tool, or validation script.
`just script-check` lints the native build entrypoints. Tool orchestration belongs
to just/CI, which directly run the golden builds and ordinary consumer projects.

`just ci` includes generator tests and coverage, all nine generators' native
golden builds, and TypeScript/Python/Rust/JVM consumer tests. Golden comparison is already included
in `test-all`, so the full recipe does not repeat it as another standalone step.

## Extra File Detection

The test harness detects when a generator produces files that have no corresponding `.golden` file. This prevents regressions where new output silently goes untested. If a generator adds a new file, the test will fail with a clear message listing the unmatched files and instructing you to run `UPDATE_GOLDEN=1`.

## Fixture Generators

The `fixture-generators/` crates generate OpenAPI specs from Rust code using utoipa annotations. This ensures fixtures are type-checked and valid:

```bash
cargo run --bin fixture-generator-petstore-spec-generator
cargo run --bin fixture-generator-enum-repr-spec-generator
cargo run --bin fixture-generator-additional-properties-spec-generator
```

Output goes to `tests/fixtures/valid/`.
