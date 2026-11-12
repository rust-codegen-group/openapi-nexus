# Golden tests: Go

set working-directory := '..'

# Update Go .golden files
@update:
    UPDATE_GOLDEN=1 cargo test --test golden_tests_go_http -- --nocapture

# Compile-check Go goldens with go build
@build: test
    ./scripts/golden-build-go.sh

# Generate fresh dependencies before ordinary Go consumer execution.
@prepare:
    bash ./scripts/generate-go-client-fixtures.sh

@test: prepare
    go -C tests/generated-clients/go/composition test ./...
    go -C tests/generated-clients/go/raw test ./...
    go -C tests/generated-clients/go/nullable test ./...
