#!/usr/bin/env bash
set -euo pipefail

if [[ $# != 1 ]]; then
  echo "Usage: generate-go-client-fixtures.sh <empty-output-directory>" >&2
  exit 1
fi
if [[ ! -d "$1" || -L "$1" ]]; then
  echo "expected an empty output directory: $1" >&2
  exit 1
fi
output="$(CDPATH='' cd -- "$1" && pwd -P)"
shopt -s dotglob nullglob
contents=("$output"/*)
shopt -u dotglob
if [[ -n "${contents[0]:-}" ]]; then
  echo "expected an empty output directory: $output" >&2
  exit 1
fi

root="$(cd "$(dirname "$0")/.." && pwd -P)"
project="$root/tests/generated-clients/go"
fixtures="$root/tests/fixtures/valid"
manifest="$project/fixtures.txt"

invalid() {
  echo "$manifest:$line_number: $1" >&2
  exit 1
}

# Preserve the relative ../generated/<name> layout used by the native modules.
# Reject an incomplete or invalid plan before creating any SDKs.
names=() inputs=() overrides=()
line_number=0
while IFS= read -r line || [[ -n "$line" ]]; do
  ((line_number += 1))
  read -r name fixture override extra <<< "$line"
  case "$name" in ''|\#*) continue ;; esac
  if [[ ! "$name" =~ ^[a-z0-9]+(-[a-z0-9]+)*$ || "$name" == generated ]]; then
    invalid "invalid consumer name: $name"
  fi
  if [[ ! -d "$project/$name" || -L "$project/$name" || ! -f "$project/$name/go.mod" || -L "$project/$name/go.mod" ]]; then
    invalid "missing checked-in consumer module: $name/go.mod"
  fi
  if [[ ! "$fixture" =~ ^[a-zA-Z0-9_-]+(/[a-zA-Z0-9_-]+)*\.(yaml|yml|json)$ || ! -f "$fixtures/$fixture" ]]; then
    invalid "invalid or missing fixture: $fixture"
  fi
  case "$override" in
    ''|all_of_strategy=composed|all_of_strategy=raw|tagged_union_strategy=dynamic|tagged_union_strategy=firstMatch|tagged_union_strategy=strict|tagged_union_strategy=raw) ;;
    *) invalid "unsupported generator override for $name: $override" ;;
  esac
  if [[ -n "$extra" ]]; then invalid "unexpected fields for $name: $extra"; fi
  for previous in "${names[@]+"${names[@]}"}"; do
    if [[ "$previous" == "$name" ]]; then invalid "duplicate consumer: $name"; fi
  done
  names+=("$name")
  inputs+=("$fixture")
  overrides+=("$override")
done < "$manifest"
if [[ -z "${names[0]:-}" ]]; then
  echo "$manifest: no Go fixtures selected" >&2
  exit 1
fi
for module in "$project"/*/go.mod; do
  consumer="${module%/go.mod}"
  name="${consumer##*/}"
  if [[ "$name" == generated ]]; then continue; fi
  for registered in "${names[@]}"; do
    if [[ "$registered" == "$name" ]]; then continue 2; fi
  done
  echo "$manifest: missing fixture for consumer: $name" >&2
  exit 1
done

for i in "${!names[@]}"; do
  name="${names[$i]}"
  options=(--config "$root/tests/generated-clients/config.toml"
    --input "$fixtures/${inputs[$i]}" --output "$output/$name" --generators go-http)
  if [[ -n "${overrides[$i]}" ]]; then
    options+=(--generator-config "go-http.${overrides[$i]}")
  fi
  cargo run --quiet --manifest-path "$root/Cargo.toml" --bin openapi-nexus -- generate "${options[@]}"
  if [[ ! -f "$output/$name/go.mod" ]]; then
    echo "missing generated Go module: $output/$name/go.mod" >&2
    exit 1
  fi
done
printf '%s\n' "${names[@]}" > "$output/consumer-roots.txt"
