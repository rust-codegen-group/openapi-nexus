#!/usr/bin/env bash
set -euo pipefail

backend="${1:?Usage: generate-jvm-client-fixtures.sh <java|kotlin>}"
case "$backend" in
  java|kotlin) ;;
  *) echo "unsupported JVM backend: $backend" >&2; exit 1 ;;
esac
root="$(cd "$(dirname "$0")/.." && pwd)"
project="$root/tests/generated-clients/jvm/$backend"

while read -r name fixture; do
  case "$name" in
    ""|\#*) continue ;;
    *[!a-zA-Z0-9_-]*) echo "invalid fixture name: $name" >&2; exit 1 ;;
  esac
  output="$project/generated/$name"
  # Only these fixed, ignored fixture output directories are disposable.
  rm -rf "$output"
  cargo run --quiet --manifest-path "$root/Cargo.toml" --bin openapi-nexus -- generate \
    --config "$root/tests/generated-clients/config.toml" \
    --input "$root/tests/fixtures/$fixture" --output "$output" --generators "$backend-okhttp"
done < "$root/tests/generated-clients/jvm/fixtures.txt"
