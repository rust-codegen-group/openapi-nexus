# Golden tests: Java

set working-directory := '..'

# Update Java .golden files
@update:
    UPDATE_GOLDEN=1 cargo test --test golden_tests_java_okhttp -- --nocapture

# Compile-check Java goldens with gradle
@build:
    ./scripts/golden-build-java.sh
