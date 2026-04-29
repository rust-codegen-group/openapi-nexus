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
├── fixtures/valid/           # Input OpenAPI specs
├── golden/
│   ├── typescript/
│   │   └── typescript-fetch/ # Expected TS output per fixture
│   └── go/
│       └── go-http/          # Expected Go output per fixture
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

- TypeScript: `tsc --noEmit` (marker: `tsconfig.json.golden`)
- Go: `go build ./...` (marker: `go.mod.golden`)

This catches type errors that snapshot comparison alone cannot.

## Running Golden Tests

```bash
# Run all snapshot tests
cargo test

# Run only TypeScript golden tests
cargo test -p openapi-nexus-typescript-fetch --test golden_tests_typescript_fetch

# Run only Go golden tests
cargo test -p openapi-nexus-go-http --test golden_tests_go_http

# Run a single test by name
cargo test -p openapi-nexus-go-http --test golden_tests_go_http -- minimal
```

## Updating Golden Files

When you intentionally change generator output:

```bash
# Update all
UPDATE_GOLDEN=1 cargo test

# Update TypeScript only
UPDATE_GOLDEN=1 cargo test --test golden_tests_typescript_fetch

# Update Go only
UPDATE_GOLDEN=1 cargo test --test golden_tests_go_http
```

After updating, verify the compile check passes:

```bash
just golden::build-ts    # TypeScript compile check
just golden::build-go    # Go compile check
just golden::build-all   # Both
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
