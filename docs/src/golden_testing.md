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
