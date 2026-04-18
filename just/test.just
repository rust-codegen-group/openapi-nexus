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
