#!/usr/bin/env bash
set -euo pipefail

if [[ $# != 1 ]]; then
  echo "Usage: generate-typescript-client-fixtures.sh <empty-output-directory>" >&2
  exit 1
fi
output="$1"
root="$(cd "$(dirname "$0")/.." && pwd -P)"
fixtures="$root/tests/fixtures/valid"
manifest="$root/tests/generated-clients/typescript/fixtures.txt"
generator=typescript-fetch
if [[ ! -d "$output" || -L "$output" ]]; then
  echo "expected an empty output directory: $output" >&2
  exit 1
fi
output="$(cd "$output" && pwd -P)"
shopt -s dotglob nullglob
contents=("$output"/*)
if [[ -n "${contents[0]:-}" ]]; then
  echo "expected an empty output directory: $output" >&2
  exit 1
fi

fail() {
  echo "$manifest:$line_number: $1" >&2
  exit 1
}

# Validate all rows before starting cargo: invalid generator options can fall
# back to defaults instead of failing, which would silently change coverage.
rows=() outputs=() pack_roots=() source_roots=()
line_number=0
while IFS= read -r line || [[ -n "$line" ]]; do
  ((line_number += 1))
  read -r -a fields <<< "$line"
  kind="${fields[0]:-}"
  case "$kind" in
    ''|\#*) continue ;;
    pack|source) ;;
    *) fail "invalid fixture kind: $kind" ;;
  esac
  name="${fields[1]:-}"
  fixture="${fields[2]:-}"
  if [[ ! "$name" =~ ^[a-z0-9]+(-[a-z0-9]+)*$ ]]; then
    fail "invalid output basename: $name"
  fi
  if [[ ! "$fixture" =~ ^[a-zA-Z0-9_-]+(/[a-zA-Z0-9_-]+)*\.(yaml|yml|json)$ || ! -f "$fixtures/$fixture" ]]; then
    fail "invalid or missing fixture: $fixture"
  fi
  for previous in "${outputs[@]+"${outputs[@]}"}"; do
    if [[ "$name" == "$previous" ]]; then
      fail "duplicate output basename: $name"
    fi
  done
  keys=" "
  for option in "${fields[@]:3}"; do
    case "$option" in
      property_naming=preserve|property_naming=camelCase|emit_type_guards=true|emit_type_guards=false) ;;
      tagged_union_strategy=firstMatch|tagged_union_strategy=strict|tagged_union_strategy=raw|untagged_union_strategy=firstMatch|untagged_union_strategy=strict|untagged_union_strategy=raw) ;;
      *) fail "invalid generator option for $name: $option" ;;
    esac
    key="${option%%=*}"
    if [[ "$keys" == *" $key "* ]]; then
      fail "duplicate generator option for $name: $key"
    fi
    keys+="$key "
  done
  case "$kind" in
    pack) pack_roots+=("$name") ;;
    source) source_roots+=("$name") ;;
  esac
  rows+=("$line")
  outputs+=("$name")
done < "$manifest"
if [[ -z "${pack_roots[0]:-}" || -z "${source_roots[0]:-}" ]]; then
  echo "$manifest: expected both pack and source fixtures" >&2
  exit 1
fi

cd "$root"
for line in "${rows[@]}"; do
  read -r -a fields <<< "$line"
  destination="$output/${fields[1]}"
  args=(--config "$root/tests/generated-clients/config.toml" --input "$fixtures/${fields[2]}"
    --output "$destination" --generators "$generator")
  # Packed cases use the Vite+ generator layout. Source-only cases keep the
  # generator's default layout and are compiled from their selected consumer.
  if [[ "${fields[0]}" == pack ]]; then
    args+=(--generator-config "$generator.toolchain=vp")
  fi
  for option in "${fields[@]:3}"; do
    args+=(--generator-config "$generator.$option")
  done
  cargo run --quiet --bin openapi-nexus -- generate "${args[@]}"
  if [[ ! -d "$destination" ]]; then
    echo "missing generated TypeScript project: $destination" >&2
    exit 1
  fi
done

printf '%s\n' "${pack_roots[@]}" > "$output/pack-roots.txt"
printf '%s\n' "${source_roots[@]}" > "$output/source-roots.txt"
