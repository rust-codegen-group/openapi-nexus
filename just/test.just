# Testing commands

set working-directory := '..'

# Run all tests
@test-all:
    cargo test

# Run TypeScript generator tests
@test-typescript:
    cargo test -p openapi-nexus-typescript-fetch

# Run Go generator tests
@test-go:
    cargo test -p openapi-nexus-go-http

# Run specific test
@test-specific TEST_NAME:
    cargo test -p openapi-nexus-typescript-fetch {{ TEST_NAME }}
