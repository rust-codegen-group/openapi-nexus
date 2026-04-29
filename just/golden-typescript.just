# Golden tests: TypeScript

set working-directory := '..'

# Update TypeScript .golden files
@update:
    UPDATE_GOLDEN=1 cargo test --test golden_tests_typescript_fetch -- --nocapture

# Compile-check TypeScript goldens with tsc --noEmit
@build:
    ./scripts/golden-build-typescript.sh
