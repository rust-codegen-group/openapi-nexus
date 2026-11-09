# Native TypeScript consumer tests against freshly generated clients checked directly by Vite+.

set working-directory := '..'

@prepare:
    rm -rf tests/generated-clients/typescript/generated/camel-case tests/generated-clients/typescript/generated/preserve
    cargo run --quiet --bin openapi-nexus -- generate --config tests/generated-clients/config.toml --input tests/fixtures/valid/typescript-additional-properties.yaml --output tests/generated-clients/typescript/generated/camel-case --generators typescript-fetch --generator-config typescript-fetch.property_naming=camelCase --generator-config typescript-fetch.toolchain=vp
    cargo run --quiet --bin openapi-nexus -- generate --config tests/generated-clients/config.toml --input tests/fixtures/valid/typescript-additional-properties.yaml --output tests/generated-clients/typescript/generated/preserve --generators typescript-fetch --generator-config typescript-fetch.toolchain=vp

@build: prepare
    #!/usr/bin/env bash
    set -euo pipefail
    project="$PWD/tests/generated-clients/typescript"
    builds=()
    trap 'rm -rf "${builds[@]}"' EXIT
    for mode in camel-case preserve; do
        output="$project/generated/$mode"
        build=$(mktemp -d)
        builds+=("$build")
        # Like golden builds, validate outside the ignored generated directory:
        # Vite+ otherwise skips every source file because of the repository ignore.
        tar --exclude=node_modules --exclude=dist -C "$output" -cf - . | tar -C "$build" -xf -
        (cd "$build" && vp install && vp --version && vp check && vp pack && vp check)
        mkdir -p "$output/dist"
        cp "$build"/dist/* "$output/dist/"
    done
    "$build/node_modules/.bin/tsc" --project "$project/tsconfig.json"

@test: build
    node --test tests/generated-clients/typescript/tests/*.test.mjs
