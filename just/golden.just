# Golden test commands (update .golden files, run comparison tests, compile-check generated code)

set working-directory := '..'

# Run all golden tests (verify generated output matches .golden files)
@check:
    cargo test --test golden_tests_typescript_fetch
    cargo test --test golden_tests_go_http
    cargo test --test golden_tests_rust_reqwest
    cargo test --test golden_tests_rust_ureq
    cargo test --test golden_tests_rust_aioduct
    cargo test --test golden_tests_python_httpx

# Update .golden files for all generators
@update: update-ts update-go update-rust update-python

# Update TypeScript .golden files only
@update-ts:
    UPDATE_GOLDEN=1 cargo test --test golden_tests_typescript_fetch -- --nocapture

# Update Go .golden files only
@update-go:
    UPDATE_GOLDEN=1 cargo test --test golden_tests_go_http -- --nocapture

# Update all Rust .golden files
@update-rust: update-rust-reqwest update-rust-ureq update-rust-aioduct

# Update Rust reqwest .golden files only
@update-rust-reqwest:
    UPDATE_GOLDEN=1 cargo test --test golden_tests_rust_reqwest -- --nocapture

# Update Rust ureq .golden files only
@update-rust-ureq:
    UPDATE_GOLDEN=1 cargo test --test golden_tests_rust_ureq -- --nocapture

# Update Rust aioduct .golden files only
@update-rust-aioduct:
    UPDATE_GOLDEN=1 cargo test --test golden_tests_rust_aioduct -- --nocapture

# Update Python httpx .golden files only
@update-python:
    UPDATE_GOLDEN=1 cargo test --test golden_tests_python_httpx -- --nocapture

# Compile-check all TypeScript goldens with tsc --noEmit
build-ts: (_build "TypeScript" "tests/golden/typescript/typescript-fetch" "tsconfig.json.golden" "tsc --noEmit")

# Compile-check all Go goldens with go build ./...
build-go: (_build "Go" "tests/golden/go/go-http" "go.mod.golden" "go build ./...")

# Compile-check Rust reqwest goldens with cargo check
build-rust-reqwest: (_build "Rust-reqwest" "tests/golden/rust/rust-reqwest" "Cargo.toml.golden" "cargo check")

# Compile-check Rust ureq goldens with cargo check
build-rust-ureq: (_build "Rust-ureq" "tests/golden/rust/rust-ureq" "Cargo.toml.golden" "cargo check")

# Compile-check Rust aioduct goldens with cargo check
build-rust-aioduct: (_build "Rust-aioduct" "tests/golden/rust/rust-aioduct" "Cargo.toml.golden" "cargo check")

# Compile-check all Rust goldens
build-rust: build-rust-reqwest build-rust-ureq build-rust-aioduct

# Compile-check all goldens
build-all: build-ts build-go build-rust build-python

# Type-check Python httpx goldens with pyright strict
build-python:
    #!/usr/bin/env bash
    set -euo pipefail
    root="{{ justfile_directory() }}"
    golden="$root/tests/golden/python/python-httpx"
    if [ ! -d "$golden" ]; then
        echo "error: golden dir not found: $golden" >&2
        exit 1
    fi
    total=0; passed=0; failed=0; fails=()
    for test_dir in "$golden"/*/; do
        [ -f "$test_dir/pyproject.toml.golden" ] || continue
        total=$((total+1))
        name=$(basename "$test_dir")
        printf "[%d] %s... " "$total" "$name"
        tmp=$(mktemp -d)
        trap 'rm -rf "$tmp"' EXIT
        while IFS= read -r -d '' f; do
            rel="${f#"$test_dir"}"
            dst="$tmp/${rel%.golden}"
            mkdir -p "$(dirname "$dst")"
            cp "$f" "$dst"
        done < <(find "$test_dir" -type f -name '*.golden' -print0)
        pip3 install httpx --quiet --target "$tmp/.deps" 2>/dev/null
        cat > "$tmp/pyrightconfig.json" <<PYRIGHT
    {
        "pythonVersion": "3.12",
        "typeCheckingMode": "strict",
        "extraPaths": [".deps"]
    }
    PYRIGHT
        log=$(mktemp)
        if (cd "$tmp" && npx pyright) >"$log" 2>&1; then
            echo "ok"
            passed=$((passed+1))
        else
            echo "FAIL"
            failed=$((failed+1))
            fails+=("$name")
            sed 's/^/    /' "$log" || true
        fi
        rm -rf "$tmp" "$log"
        trap - EXIT
    done
    echo
    echo "Python: $passed/$total passed"
    if [ "$failed" -gt 0 ]; then
        echo "failed:"
        for n in "${fails[@]}"; do echo "  - $n"; done
        exit 1
    fi

# Private: materialize each golden subdir to a tempdir (strip .golden suffix) and run CMD
[private]
_build LANG DIR MARKER CMD:
    #!/usr/bin/env bash
    set -euo pipefail
    root="{{ justfile_directory() }}"
    golden="$root/{{ DIR }}"
    if [ ! -d "$golden" ]; then
        echo "error: golden dir not found: $golden" >&2
        exit 1
    fi
    total=0; passed=0; failed=0; fails=()
    for test_dir in "$golden"/*/; do
        [ -f "$test_dir/{{ MARKER }}" ] || continue
        total=$((total+1))
        name=$(basename "$test_dir")
        printf "[%d] %s... " "$total" "$name"
        tmp=$(mktemp -d)
        trap 'rm -rf "$tmp"' EXIT
        while IFS= read -r -d '' f; do
            rel="${f#"$test_dir"}"
            dst="$tmp/${rel%.golden}"
            mkdir -p "$(dirname "$dst")"
            cp "$f" "$dst"
        done < <(find "$test_dir" -type f -name '*.golden' -print0)
        log=$(mktemp)
        if (cd "$tmp" && {{ CMD }}) >"$log" 2>&1; then
            echo "ok"
            passed=$((passed+1))
        else
            echo "FAIL"
            failed=$((failed+1))
            fails+=("$name")
            sed 's/^/    /' "$log" || true
        fi
        rm -rf "$tmp" "$log"
        trap - EXIT
    done
    echo
    echo "{{ LANG }}: $passed/$total passed"
    if [ "$failed" -gt 0 ]; then
        echo "failed:"
        for n in "${fails[@]}"; do echo "  - $n"; done
        exit 1
    fi
