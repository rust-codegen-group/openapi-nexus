# Testing commands

# Run all tests
@test-all:
    cargo test

# Run TypeScript generator tests
@test-typescript:
    cargo test -p openapi-nexus-typescript

# Run Go generator tests
@test-go:
    cargo test -p openapi-nexus-go

# Run specific test
@test-specific TEST_NAME:
    cargo test -p openapi-nexus-typescript {{ TEST_NAME }}

# Update golden test files for all generators
@golden-update: golden-update-ts golden-update-go

# Update golden test files for TypeScript only
@golden-update-ts:
    UPDATE_GOLDEN=1 cargo test --test golden_tests_typescript_fetch -- --nocapture

# Update golden test files for Go only
@golden-update-go:
    UPDATE_GOLDEN=1 cargo test --test golden_tests_go_http -- --nocapture

# Run golden tests (verify they pass)
@golden-check:
    cargo test --test golden_tests_typescript_fetch
    cargo test --test golden_tests_go_http
