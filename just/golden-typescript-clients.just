# Native TypeScript consumer tests against freshly generated clients checked directly by Vite+.

set working-directory := '..'

@prepare:
    rm -rf tests/generated-clients/typescript/generated/camel-case tests/generated-clients/typescript/generated/preserve tests/generated-clients/typescript/generated/union-default tests/generated-clients/typescript/generated/union-strict tests/generated-clients/typescript/generated/union-raw tests/generated-clients/typescript/generated/isolation tests/generated-clients/typescript/generated/tagged-allof tests/generated-clients/typescript/generated/tagged-allof-preserve tests/generated-clients/typescript/generated/overlapping-allof
    cargo run --quiet --bin openapi-nexus -- generate --config tests/generated-clients/config.toml --input tests/fixtures/valid/typescript-additional-properties.yaml --output tests/generated-clients/typescript/generated/camel-case --generators typescript-fetch --generator-config typescript-fetch.property_naming=camelCase --generator-config typescript-fetch.toolchain=vp
    cargo run --quiet --bin openapi-nexus -- generate --config tests/generated-clients/config.toml --input tests/fixtures/valid/typescript-additional-properties.yaml --output tests/generated-clients/typescript/generated/preserve --generators typescript-fetch --generator-config typescript-fetch.toolchain=vp
    cargo run --quiet --bin openapi-nexus -- generate --config tests/generated-clients/config.toml --input tests/fixtures/valid/typescript-untagged-union.yaml --output tests/generated-clients/typescript/generated/union-default --generators typescript-fetch --generator-config typescript-fetch.property_naming=camelCase --generator-config typescript-fetch.toolchain=vp --generator-config typescript-fetch.emit_type_guards=true
    cargo run --quiet --bin openapi-nexus -- generate --config tests/generated-clients/config.toml --input tests/fixtures/valid/typescript-untagged-union.yaml --output tests/generated-clients/typescript/generated/union-strict --generators typescript-fetch --generator-config typescript-fetch.property_naming=camelCase --generator-config typescript-fetch.toolchain=vp --generator-config typescript-fetch.emit_type_guards=true --generator-config typescript-fetch.untagged_union_strategy=strict
    cargo run --quiet --bin openapi-nexus -- generate --config tests/generated-clients/config.toml --input tests/fixtures/valid/typescript-untagged-union.yaml --output tests/generated-clients/typescript/generated/union-raw --generators typescript-fetch --generator-config typescript-fetch.property_naming=camelCase --generator-config typescript-fetch.toolchain=vp --generator-config typescript-fetch.emit_type_guards=true --generator-config typescript-fetch.untagged_union_strategy=raw
    cargo run --quiet --bin openapi-nexus -- generate --config tests/generated-clients/config.toml --input tests/fixtures/valid/typescript-helper-isolation.yaml --output tests/generated-clients/typescript/generated/isolation --generators typescript-fetch --generator-config typescript-fetch.property_naming=camelCase
    cargo run --quiet --bin openapi-nexus -- generate --config tests/generated-clients/config.toml --input tests/fixtures/valid/tagged-allof-payloads.yaml --output tests/generated-clients/typescript/generated/tagged-allof --generators typescript-fetch --generator-config typescript-fetch.property_naming=camelCase --generator-config typescript-fetch.toolchain=vp --generator-config typescript-fetch.emit_type_guards=true
    cargo run --quiet --bin openapi-nexus -- generate --config tests/generated-clients/config.toml --input tests/fixtures/valid/tagged-allof-payloads.yaml --output tests/generated-clients/typescript/generated/tagged-allof-preserve --generators typescript-fetch --generator-config typescript-fetch.toolchain=vp --generator-config typescript-fetch.emit_type_guards=true
    cargo run --quiet --bin openapi-nexus -- generate --config tests/generated-clients/config.toml --input tests/fixtures/valid/typescript-overlapping-allof.yaml --output tests/generated-clients/typescript/generated/overlapping-allof --generators typescript-fetch --generator-config typescript-fetch.property_naming=camelCase --generator-config typescript-fetch.toolchain=vp --generator-config typescript-fetch.untagged_union_strategy=strict

@build: prepare
    #!/usr/bin/env bash
    set -euo pipefail
    project="$PWD/tests/generated-clients/typescript"
    builds=()
    trap 'rm -rf "${builds[@]}"' EXIT
    for mode in camel-case preserve union-default union-strict union-raw tagged-allof tagged-allof-preserve overlapping-allof; do
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
    "$build/node_modules/.bin/tsc" --project "$project/tsconfig.isolation.json"

@test: build
    node --test tests/generated-clients/typescript/tests/*.test.mjs
