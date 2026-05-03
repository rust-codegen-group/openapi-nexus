# Golden tests: Kotlin

set working-directory := '..'

# Update Kotlin .golden files
@update:
    UPDATE_GOLDEN=1 cargo test --test golden_tests_kotlin_okhttp -- --nocapture

# Compile-check Kotlin goldens with gradle
@build:
    ./scripts/golden-build-kotlin.sh
