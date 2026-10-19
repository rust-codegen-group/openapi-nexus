# Golden tests: Rust

set working-directory := '..'

# Update all Rust .golden files
update: update-reqwest update-ureq update-aioduct

# Update Rust reqwest .golden files
@update-reqwest:
    UPDATE_GOLDEN=1 cargo test --test golden_tests_rust_reqwest -- --nocapture

# Update Rust ureq .golden files
@update-ureq:
    UPDATE_GOLDEN=1 cargo test --test golden_tests_rust_ureq -- --nocapture

# Update Rust aioduct .golden files
@update-aioduct:
    UPDATE_GOLDEN=1 cargo test --test golden_tests_rust_aioduct -- --nocapture

# Compile-check all Rust goldens
build: build-reqwest build-ureq build-aioduct

# Compile-check Rust reqwest goldens
@build-reqwest:
    ./scripts/golden-build-rust.sh reqwest

# Compile-check Rust ureq goldens
@build-ureq:
    ./scripts/golden-build-rust.sh ureq

# Compile-check Rust aioduct goldens
@build-aioduct:
    ./scripts/golden-build-rust.sh aioduct

# Generate fresh clients and run their consumer test projects
test: test-reqwest test-ureq test-aioduct

# Prepare generated dependencies for direct Cargo/IDE use
@prepare BACKEND:
    bash ./scripts/generate-rust-client-fixtures.sh {{ BACKEND }}

@test-reqwest: (prepare "reqwest")
    cargo test --locked --manifest-path tests/generated-clients/rust/reqwest/Cargo.toml

@test-ureq: (prepare "ureq")
    cargo test --locked --manifest-path tests/generated-clients/rust/ureq/Cargo.toml

@test-aioduct: (prepare "aioduct")
    cargo test --locked --manifest-path tests/generated-clients/rust/aioduct/Cargo.toml
