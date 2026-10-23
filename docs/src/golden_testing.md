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
| TypeScript | `vp check --no-fmt && vp pack` or `tsc --noEmit` (see below) | `tsconfig.json.golden` |
| Go | `go build ./...` | `go.mod.golden` |
| Rust | `cargo check` | `Cargo.toml.golden` |
| Python | `pyright` | `pyproject.toml.golden` |
| Java | `gradle compileJava` | `build.gradle.golden` |
| Kotlin | `gradle compileKotlin` | `build.gradle.kts.golden` |

This catches type errors that snapshot comparison alone cannot.

### TypeScript: `vp pack` vs `tsc --noEmit`

The TypeScript golden build script selects the toolchain per-test based on whether
`vite.config.ts.golden` exists in the test directory:

- **With `vite.config.ts`** — runs `vp install && vp check --no-fmt && vp pack`. This is the `toolchain = "vp"` path: installs deps, runs type-aware linting, then builds the library bundle.
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
  that environment across its fixtures. `PYTHON` selects the install interpreter;
  the active `pyright` is used when available, otherwise npm resolves it once.
- Development and ordinary CI validation select current Rust `stable` (locally
  validated with Rust 1.99.0). MSRV checks explicitly select Rust 1.90.0; they do
  not substitute for stable validation.
- Local TypeScript validation uses TypeScript 7.0.2 and Vite+ 1.3.0 on Node
  24.22.1. The native validators use the active `tsc` and `vp` on `PATH`; when
  `tsc` is not globally installed, `npm exec --package typescript -- just
  golden-typescript::build-tsc` supplies it without changing generated packages.
- Current local JVM validation uses Homebrew JDK 27 with Gradle 9.8.1, respecting
  `JAVA_HOME` or the active JVM. CI validates on JDK 27. Generated libraries may
  still declare a Java 17 compile target, independently of the consumer test JVM.

Fixture discovery is tested in-process through the same build-support module
used by `build.rs`. Cargo compiles and executes the registered parser tests
normally; no test harness launches a compiler, build tool, or validation script.
`just script-check` lints the native build entrypoints. Tool orchestration belongs
to just/CI, which directly run the golden builds and ordinary consumer projects.

`just ci` includes generator tests and coverage, all nine generators' native
golden builds, and Rust/JVM consumer tests. Golden comparison is already included
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
