# Golden tests: Go

set working-directory := '..'

# Update Go .golden files
@update:
    UPDATE_GOLDEN=1 cargo test --test golden_tests_go_http -- --nocapture

# Compile-check all registered Go golden projects.
@compile:
    ./scripts/golden-build-go.sh

# Preserve the combined golden and fresh-client validation entrypoint.
@build: compile test

# Refresh generated dependencies for direct Go or IDE use.
@prepare: (_fixtures "prepare")

@test: (_fixtures "test")

@_fixtures action:
    #!/usr/bin/env bash
    set -euo pipefail
    action="{{ action }}"
    project="$(pwd -P)/tests/generated-clients/go"
    if [[ "$action" == prepare ]]; then
        parent="$project"
        [[ "$(cd "$parent" && pwd -P)" == "$parent" ]]
    else
        parent="$(cd "${TMPDIR:-/tmp}" && pwd -P)"
    fi
    run="$(mktemp -d "$parent/.nexus-go.XXXXXX")"
    cleanup() {
        if [[ -d "$run" && ! -L "$run" && "$(cd "$run" && pwd -P)" == "$run" ]]; then
            rm -rf -- "$run"
        fi
    }
    trap cleanup EXIT
    generated="$run/generated"
    mkdir -- "$generated"
    bash ./scripts/generate-go-client-fixtures.sh "$generated"
    if [[ "$action" == prepare ]]; then
        output="$project/generated"
        if [[ -e "$output" || -L "$output" ]]; then
            [[ -d "$output" && ! -L "$output" && "$(cd "$output" && pwd -P)" == "$output" ]]
            rm -rf -- "$output"
        fi
        [[ "$(cd "$generated" && pwd -P)" == "$generated" ]]
        mv -- "$generated" "$output"
        exit
    fi

    while IFS= read -r consumer <&3; do
        cp -R -- "$project/$consumer" "$run/$consumer"
        replacement=$(GOWORK=off go -C "$run/$consumer" list -m -f '{{{{with .Replace}}{{{{.Path}}{{{{end}}' example.com/sdk)
        if [[ "$replacement" != "../generated/$consumer" ]]; then
            printf 'Go consumer %s must replace example.com/sdk with ../generated/%s (got %s)\n' "$consumer" "$consumer" "$replacement" >&2
            exit 1
        fi
        GOWORK=off go -C "$run/$consumer" test ./...
    done 3< "$generated/consumer-roots.txt"