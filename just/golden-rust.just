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
