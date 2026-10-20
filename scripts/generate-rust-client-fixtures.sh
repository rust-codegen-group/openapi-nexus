#!/usr/bin/env bash
set -euo pipefail

backend="${1:?Usage: generate-rust-client-fixtures.sh <reqwest|ureq|aioduct>}"
case "$backend" in
  reqwest|ureq|aioduct) ;;
  *) echo "unsupported Rust backend: $backend" >&2; exit 1 ;;
esac

root="$(cd "$(dirname "$0")/.." && pwd)"
project="$root/tests/generated-clients/rust/$backend"

for fixture in typed-error-responses rust-error-payload-shapes nested-nullable; do
  output="$project/generated/$fixture"
  # These fixed fixture directories contain only disposable generated output.
  # Remove stale source files so a deleted declaration cannot pass unnoticed.
  rm -rf "$output"
  cargo run --quiet --manifest-path "$root/Cargo.toml" --bin openapi-nexus -- generate \
    --config "$root/tests/generated-clients/config.toml" \
    --input "$root/tests/fixtures/valid/$fixture.yaml" \
    --output "$output" \
    --generators "rust-$backend"
done
