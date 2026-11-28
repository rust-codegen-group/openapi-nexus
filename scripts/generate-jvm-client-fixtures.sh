#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 || $# -gt 2 ]]; then
  echo "Usage: generate-jvm-client-fixtures.sh <java|kotlin> [empty-output-directory]" >&2
  exit 1
fi
backend="$1"
case "$backend" in
  java|kotlin) ;;
  *) echo "unsupported JVM backend: $backend" >&2; exit 1 ;;
esac
root="$(cd "$(dirname "$0")/.." && pwd -P)"
fixtures="$root/tests/fixtures"
jvm="$root/tests/generated-clients/jvm"
project="$jvm/$backend"
generator="$backend-okhttp"

fail() { printf '%s\n' "$1" >&2; exit 1; }

names=() inputs=() overrides=()
registered() {
  for previous in "${names[@]+"${names[@]}"}"; do
    if [[ "$1" == "$previous" ]]; then return 0; fi
  done
  return 1
}

# Gradle uses these same manifests to include client and consumer projects.
# Verify their pairing before generating anything, including unregistered cases.
manifests=("$jvm/fixtures.txt")
if [[ -f "$project/fixtures.txt" ]]; then manifests+=("$project/fixtures.txt"); fi
for manifest in "${manifests[@]}"; do
  line_number=0
  while IFS= read -r line || [[ -n "$line" ]]; do
    ((line_number += 1))
    if [[ "$line" == \#* ]]; then continue; fi
    read -r name fixture override extra <<< "$line"
    if [[ -z "$name" ]]; then continue; fi
    location="$manifest:$line_number"
    if [[ ! "$name" =~ ^[a-zA-Z0-9_-]+$ || -n "$extra" ]]; then
      fail "$location: expected a case name, fixture path, and optional generator setting"
    fi
    if [[ ! "$fixture" =~ ^[a-zA-Z0-9_-]+(/[a-zA-Z0-9_-]+)*\.(yaml|yml|json)$ || ! -f "$fixtures/$fixture" ]]; then
      fail "$location: invalid or missing fixture: $fixture"
    fi
    case "$override" in
      ''|untagged_union_strategy=firstMatch|untagged_union_strategy=strict|untagged_union_strategy=raw) ;;
      tagged_union_strategy=firstMatch|tagged_union_strategy=strict|tagged_union_strategy=raw) ;;
      all_of_strategy=composed|all_of_strategy=raw) ;;
      *) fail "$location: invalid generator setting: $override" ;;
    esac
    if registered "$name"; then fail "$location: duplicate $backend fixture: $name"; fi
    if [[ -d "$jvm/shared-cases/$name" && -d "$project/cases/$name" ]]; then
      fail "$location: both shared and $backend consumers exist for $name"
    elif [[ ! -d "$jvm/shared-cases/$name" && ! -d "$project/cases/$name" ]]; then
      fail "$location: no $backend consumer directory for $name"
    fi
    names+=("$name")
    inputs+=("$fixtures/$fixture")
    overrides+=("$override")
  done < "$manifest"
done
if [[ -z "${names[0]:-}" ]]; then fail "no JVM fixtures registered for $backend"; fi
shopt -s nullglob dotglob
for consumer in "$jvm/shared-cases"/*/ "$project/cases"/*/; do
  case_dir="${consumer%/}"
  if ! registered "${case_dir##*/}"; then fail "unregistered $backend consumer: $case_dir"; fi
done

if [[ $# == 2 ]]; then
  output="$2"
  if [[ ! -d "$output" || -L "$output" ]]; then
    fail "expected an empty output directory: $output"
  fi
  output="$(cd "$output" && pwd -P)"
  existing=("$output"/*)
  if [[ -n "${existing[0]:-}" ]]; then fail "expected an empty output directory: $output"; fi
else
  # Keep the checkout-local SDKs available until their replacements exist.
  [[ "$(cd "$project" && pwd -P)" == "$project" ]]
  output="$(mktemp -d "$project/.nexus-jvm.XXXXXX")"
  cleanup() {
    if [[ -d "$output" && ! -L "$output" && "$(cd "$output" && pwd -P)" == "$output" ]]; then
      rm -rf -- "$output"
    fi
  }
  trap cleanup EXIT
fi

cd "$root"
for i in "${!names[@]}"; do
  destination="$output/${names[$i]}"
  options=(--config "$root/tests/generated-clients/config.toml"
    --input "${inputs[$i]}" --output "$destination" --generators "$generator")
  if [[ -n "${overrides[$i]}" ]]; then
    options+=(--generator-config "$generator.${overrides[$i]}")
  fi
  cargo run --quiet --manifest-path "$root/Cargo.toml" --bin openapi-nexus -- generate "${options[@]}"
  if [[ ! -f "$destination/build.gradle" && ! -f "$destination/build.gradle.kts" ]]; then
    fail "no Gradle project generated at $destination"
  fi
done

if [[ $# == 1 ]]; then
  stable="$project/generated"
  if [[ -e "$stable" || -L "$stable" ]]; then
    [[ -d "$stable" && ! -L "$stable" && "$(cd "$stable" && pwd -P)" == "$stable" ]]
    rm -rf -- "$stable"
  fi
  [[ "$(cd "$output" && pwd -P)" == "$output" ]]
  mv -- "$output" "$stable"
fi
