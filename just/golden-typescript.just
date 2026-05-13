# Golden tests: TypeScript

set working-directory := '..'

# Update TypeScript .golden files
@update:
    UPDATE_GOLDEN=1 cargo test --test golden_tests_typescript_fetch -- --nocapture

# Compile-check all TypeScript goldens (both toolchains)
@build: build-tsc build-vp

# Compile-check TypeScript goldens (tsc toolchain)
@build-tsc:
    ./scripts/golden-build-typescript.sh tsc

# Compile-check TypeScript goldens (vp toolchain)
@build-vp:
    ./scripts/golden-build-typescript.sh vp
