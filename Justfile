# List available recipes
default:
    @just --list --list-submodules

mod golden-typescript 'just/golden-typescript.just'
mod golden-go 'just/golden-go.just'
mod golden-rust 'just/golden-rust.just'
mod golden-python 'just/golden-python.just'

# ---------- Build ----------

# Build the project
build:
    cargo build

# Build release
build-release:
    cargo build --release

# Check (compile without running)
check:
    cargo check

# Check all targets
check-all:
    cargo check --all-targets --all-features

# Check MSRV (1.90)
msrv:
    cargo +1.90.0 check

# ---------- Lint ----------

# Apply formatting
fmt:
    cargo fmt --all

# Check formatting
fmt-check:
    cargo fmt --all -- --check

# Run clippy with warnings as errors
clippy:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# Run all lint checks (fmt-check + clippy)
lint: fmt-check clippy

# ---------- Test ----------

# Run tests with nextest
test:
    cargo nextest run --workspace

# Run doctests only
test-doc:
    cargo test --doc --workspace

# Run all tests (nextest + doctests)
test-all: test test-doc

# Run a specific test by name
test-specific TEST_NAME:
    cargo test {{ TEST_NAME }}

# ---------- Coverage ----------

# Show coverage summary table
coverage:
    cargo llvm-cov clean --workspace
    cargo llvm-cov nextest --no-report
    cargo llvm-cov report --summary-only

# Generate HTML coverage report and open in browser
coverage-html:
    mkdir -p coverage/html
    cargo llvm-cov clean --workspace
    cargo llvm-cov nextest --html --output-dir coverage/html
    open coverage/html/index.html 2>/dev/null || xdg-open coverage/html/index.html 2>/dev/null || true

# Generate LCOV output for CI/editors
coverage-lcov:
    mkdir -p coverage
    cargo llvm-cov nextest --workspace --lcov --output-path coverage/lcov.info

# ---------- Docs ----------

# Build and open rustdoc
doc:
    cargo doc --no-deps --open

# Build rustdoc without opening (CI mode)
doc-check:
    RUSTDOCFLAGS="-Dwarnings" cargo doc --workspace --no-deps

# Build the mdbook
book:
    mdbook build docs

# Serve the mdbook with live reload
book-serve:
    mdbook serve docs --open

# ---------- Generate ----------

# Generate TypeScript from a spec file
generate-typescript INPUT OUTPUT:
    cargo run --bin openapi-nexus -- generate --input {{ INPUT }} --output {{ OUTPUT }} --generators typescript-fetch

# Generate Go from a spec file
generate-go INPUT OUTPUT:
    cargo run --bin openapi-nexus -- generate --input {{ INPUT }} --output {{ OUTPUT }} --generators go-http

# Generate both TypeScript and Go from a spec file
generate-all INPUT OUTPUT:
    cargo run --bin openapi-nexus -- generate --input {{ INPUT }} --output {{ OUTPUT }} --generators typescript-fetch,go-http

# Check TypeScript compilation in output directory
check-ts OUTPUT_DIR:
    cd {{ OUTPUT_DIR }} && npx tsc --noEmit

# ---------- Golden (top-level convenience) ----------

# Run all golden tests (verify generated output matches .golden files)
golden-check:
    cargo test --test golden_tests_typescript_fetch
    cargo test --test golden_tests_go_http
    cargo test --test golden_tests_rust_reqwest
    cargo test --test golden_tests_rust_ureq
    cargo test --test golden_tests_rust_aioduct
    cargo test --test golden_tests_python_httpx
    cargo test --test golden_tests_python_requests

# Update .golden files for all generators
golden-update: golden-typescript::update golden-go::update golden-rust::update golden-python::update

# Compile-check all goldens
golden-build-all: golden-typescript::build golden-go::build golden-rust::build golden-python::build

# ---------- CI (run everything locally) ----------

# Run the full CI pipeline locally
ci: fmt-check clippy doc-check book msrv test-all coverage-lcov golden-check
