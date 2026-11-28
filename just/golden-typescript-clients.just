# Native consumers of packed TypeScript SDKs and an isolated generated source graph.

set working-directory := '..'
set positional-arguments

@prepare: (_fixtures "prepare")

@build: (_fixtures "build")

@test: (_fixtures "test")

@_fixtures action:
    #!/usr/bin/env bash
    set -euo pipefail
    action="$1"
    root="$(pwd -P)"
    relative="tests/generated-clients/typescript"
    project="$root/$relative"
    if [[ "$action" == prepare ]]; then
        parent="$project"
        [[ "$(cd "$parent" && pwd -P)" == "$parent" ]]
    else
        parent="$(cd "${TMPDIR:-/tmp}" && pwd -P)"
        case "$parent/" in "$root/"*) echo 'TypeScript validation requires TMPDIR outside the checkout' >&2; exit 2 ;; esac
    fi
    run="$(mktemp -d "$parent/.nexus-typescript.XXXXXX")"
    cleanup() {
        if [[ -d "$run" && ! -L "$run" && "$(cd "$run" && pwd -P)" == "$run" ]]; then
            rm -rf -- "$run"
        fi
    }
    trap cleanup EXIT
    if [[ "$action" == prepare ]]; then
        bash ./scripts/generate-typescript-client-fixtures.sh "$run"
        output="$project/generated"
        if [[ -e "$output" || -L "$output" ]]; then
            [[ -d "$output" && ! -L "$output" && "$(cd "$output" && pwd -P)" == "$output" ]]
            rm -rf -- "$output"
        fi
        [[ "$(cd "$run" && pwd -P)" == "$run" ]]
        mv -- "$run" "$output"
        exit
    fi

    # Preserve the relative paths used by the real declaration and runtime consumers.
    consumer="$run/$relative"
    generated="$consumer/generated"
    mkdir -p -- "$generated"
    cp -- "$project/"*.ts "$project/"tsconfig*.json "$consumer/"
    cp -R -- "$project/tests" "$consumer/"
    bash ./scripts/generate-typescript-client-fixtures.sh "$generated"

    # Vite+ checks the original projects outside the checkout, where sources are
    # not affected by the repository ignore rules. No copying dist back to the IDE.
    tsc=''
    while IFS= read -r mode <&3; do
        package="$generated/$mode"
        printf '\nChecking and packing TypeScript fixture: %s\n' "$mode"
        (cd "$package" && vp install && vp --version && vp check && vp pack && vp check)
        if [[ ! -s "$package/dist/index.mjs" || ! -s "$package/dist/index.d.mts" ]]; then
            echo "Missing packed bundle or declarations for fixture: $mode" >&2
            exit 1
        fi
        if [[ -z "$tsc" ]]; then tsc="$package/node_modules/.bin/tsc"; fi
    done 3< "$generated/pack-roots.txt"
    if [[ ! -x "$tsc" ]]; then
        echo "Missing TypeScript compiler from the packed fixture: $tsc" >&2
        exit 1
    fi

    printf '\nChecking packed TypeScript declarations\n'
    "$tsc" --project "$consumer/tsconfig.json"
    printf '\nChecking selected TypeScript source graphs (not packed):\n'
    while IFS= read -r mode; do printf '  %s\n' "$mode"; done < "$generated/source-roots.txt"
    "$tsc" --project "$consumer/tsconfig.isolation.json"

    if [[ "$action" == test ]]; then
        (cd "$consumer" && node --test tests/*.test.mjs)
    fi
