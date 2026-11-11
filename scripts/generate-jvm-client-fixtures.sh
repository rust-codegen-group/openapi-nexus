#!/usr/bin/env bash
set -euo pipefail

backend="${1:?Usage: generate-jvm-client-fixtures.sh <java|kotlin>}"
case "$backend" in
  java|kotlin) ;;
  *) echo "unsupported JVM backend: $backend" >&2; exit 1 ;;
esac
root="$(cd "$(dirname "$0")/.." && pwd)"
project="$root/tests/generated-clients/jvm/$backend"

manifests=("$root/tests/generated-clients/jvm/fixtures.txt")
if [ -f "$project/fixtures.txt" ]; then manifests+=("$project/fixtures.txt"); fi
for manifest in "${manifests[@]}"; do
  while read -r name fixture strategy; do
    case "$name" in
      ""|\#*) continue ;;
      *[!a-zA-Z0-9_-]*) echo "invalid fixture name: $name" >&2; exit 1 ;;
    esac
    output="$project/generated/$name"
    options=(--config "$root/tests/generated-clients/config.toml"
      --input "$root/tests/fixtures/$fixture" --output "$output" --generators "$backend-okhttp")
    if [ -n "$strategy" ]; then
      case "$strategy" in
        firstMatch|strict|raw) ;;
        *) echo "invalid union strategy: $strategy" >&2; exit 1 ;;
      esac
      options+=(--generator-config "$backend-okhttp.untagged_union_strategy=$strategy")
    fi
    # Only these fixed, ignored fixture output directories are disposable.
    rm -rf "$output"
    cargo run --quiet --manifest-path "$root/Cargo.toml" --bin openapi-nexus -- generate "${options[@]}"
  done < "$manifest"
done
