# Native Python consumers of freshly generated, unmodified output.

set working-directory := '..'
set positional-arguments

@prepare backend: (_fixtures backend "prepare")

@build backend: (_fixtures backend "build")

@test backend: (_fixtures backend "test")

@_fixtures backend action:
    #!/usr/bin/env bash
    set -euo pipefail
    backend="$1"
    action="$2"
    case "$backend" in httpx|requests) ;; *) echo 'Expected httpx or requests' >&2; exit 2 ;; esac
    project="$(pwd -P)/tests/generated-clients/python"
    if [[ "$action" == prepare ]]; then
        parent="$project/generated"
        mkdir -p -- "$parent"
        [[ "$(cd "$parent" && pwd -P)" == "$parent" ]]
    else
        parent="$(cd "${TMPDIR:-/tmp}" && pwd -P)"
        if [[ "$parent" == *:* ]]; then
            echo "Python fixture temporary directory cannot contain a colon: $parent" >&2
            exit 2
        fi
    fi
    run="$(mktemp -d "$parent/.nexus-python-$backend.XXXXXX")"
    cleanup() {
        if [[ -d "$run" && ! -L "$run" && "$(cd "$run" && pwd -P)" == "$run" ]]; then
            rm -rf -- "$run"
        fi
    }
    trap cleanup EXIT
    bash ./scripts/generate-python-client-fixtures.sh "$backend" "$run"
    if [[ "$action" == prepare ]]; then
        output="$parent/$backend"
        if [[ -e "$output" || -L "$output" ]]; then
            [[ -d "$output" && ! -L "$output" && "$(cd "$output" && pwd -P)" == "$output" ]]
            rm -rf -- "$output"
        fi
        [[ "$(cd "$run" && pwd -P)" == "$run" ]]
        mv -- "$run" "$output"
        exit
    fi
    python="$(command -v "${PYTHON:-python3}")"
    [[ "$python" = /* ]] || python="$(pwd -P)/$python"
    PYTHONPATH= "${PYRIGHT:-pyright}" --pythonpath "$python" --project "$run/pyrightconfig.json"
    if [[ "$action" == test ]]; then
        import_paths=()
        while IFS= read -r directory; do import_paths+=("$run/$directory"); done < "$run/import-roots.txt"
        pythonpath="$(IFS=:; printf '%s' "${import_paths[*]}")"
        (cd "$run" && NEXUS_PYTHON_BACKEND="$backend" PYTHONPATH="$pythonpath" PYTHONDONTWRITEBYTECODE=1 \
            "$python" -m unittest discover -v -s "$project/tests")
    fi
