# Golden tests: Go

set working-directory := '..'

# Update Go .golden files
@update:
    UPDATE_GOLDEN=1 cargo test --test golden_tests_go_http -- --nocapture

# Compile-check Go goldens with go build
@build:
    ./scripts/golden-build-go.sh
