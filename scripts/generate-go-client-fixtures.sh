#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
project="$root/tests/generated-clients/go"
while read -r name fixture strategy; do
  case "$name" in
    ""|\#*) continue ;;
    *[!a-zA-Z0-9_-]*) echo "invalid fixture name: $name" >&2; exit 1 ;;
  esac
  output="$project/generated/$name"
  options=(--config "$root/tests/generated-clients/config.toml"
    --input "$root/tests/fixtures/$fixture" --output "$output" --generators go-http)
  if [ -n "$strategy" ]; then
    case "$strategy" in
      composed|raw) ;;
      *) echo "invalid allOf strategy: $strategy" >&2; exit 1 ;;
    esac
    options+=(--generator-config "go-http.all_of_strategy=$strategy")
  fi
  # Only these fixed, ignored fixture output directories are disposable.
  rm -rf "$output"
  cargo run --quiet --manifest-path "$root/Cargo.toml" --bin openapi-nexus -- generate "${options[@]}"
done < "$project/fixtures.txt"
