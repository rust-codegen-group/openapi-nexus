# Testing commands

set working-directory := '..'

# Run all tests
@test-all:
    cargo test

# Run TypeScript generator tests
@test-typescript:
    cargo test --test golden_tests_typescript_fetch

# Run Go generator tests
@test-go:
    cargo test --test golden_tests_go_http

# Run specific test by name
@test-specific TEST_NAME:
    cargo test {{ TEST_NAME }}
