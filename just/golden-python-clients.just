# Native Python consumers of freshly generated, unmodified output.

set working-directory := '..'

@prepare backend:
    #!/usr/bin/env bash
    set -euo pipefail
    case '{{backend}}' in httpx|requests) ;; *) echo 'Expected httpx or requests' >&2; exit 2 ;; esac
    for policy in default strict raw; do
        strategy="$policy"
        [ "$policy" != default ] || strategy=firstMatch
        cargo run --quiet --bin openapi-nexus -- generate --config tests/generated-clients/config.toml --input tests/fixtures/valid/python-untagged-union.yaml --output "tests/generated-clients/python/generated/{{backend}}/$policy" --generators 'python-{{backend}}' --generator-config "python-{{backend}}.package_name=union_$policy" --generator-config "python-{{backend}}.untagged_union_strategy=$strategy"
        cargo run --quiet --bin openapi-nexus -- generate --config tests/generated-clients/config.toml --input tests/fixtures/valid/python-additional-properties.yaml --output "tests/generated-clients/python/generated/{{backend}}/additional/$policy" --generators 'python-{{backend}}' --generator-config "python-{{backend}}.package_name=additional_$policy" --generator-config "python-{{backend}}.untagged_union_strategy=$strategy"
    done

@build backend: (prepare backend)
    "${PYRIGHT:-pyright}" --pythonpath "$(command -v "${PYTHON:-python3}")" --project tests/generated-clients/python/pyrightconfig.{{backend}}.json tests/generated-clients/python/generated/{{backend}}/default/union_default tests/generated-clients/python/generated/{{backend}}/strict/union_strict tests/generated-clients/python/generated/{{backend}}/raw/union_raw tests/generated-clients/python/generated/{{backend}}/additional/default/additional_default tests/generated-clients/python/generated/{{backend}}/additional/strict/additional_strict tests/generated-clients/python/generated/{{backend}}/additional/raw/additional_raw tests/generated-clients/python/typing_contracts.py tests/generated-clients/python/additional_typing_contracts.py

@test backend: (build backend)
    NEXUS_PYTHON_BACKEND={{backend}} PYTHONPATH="tests/generated-clients/python/generated/{{backend}}/default:tests/generated-clients/python/generated/{{backend}}/strict:tests/generated-clients/python/generated/{{backend}}/raw:tests/generated-clients/python/generated/{{backend}}/additional/default:tests/generated-clients/python/generated/{{backend}}/additional/strict:tests/generated-clients/python/generated/{{backend}}/additional/raw" "${PYTHON:-python3}" -m unittest discover -v -s tests/generated-clients/python/tests
