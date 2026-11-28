#!/usr/bin/env bash
set -euo pipefail

backend="${1:?Usage: generate-python-client-fixtures.sh <httpx|requests> <empty-output-directory>}"
output="${2:?Expected an empty output directory}"
case "$backend" in
  httpx|requests) ;;
  *) echo "unsupported Python backend: $backend" >&2; exit 1 ;;
esac
root="$(cd "$(dirname "$0")/.." && pwd -P)"
project="$root/tests/generated-clients/python"
fixtures="$root/tests/fixtures/valid"
manifest="$project/fixtures.txt"
generator="python-$backend"
if [[ ! -d "$output" || -L "$output" || -n "$(ls -A -- "$output")" ]]; then
  echo "expected an empty output directory: $output" >&2
  exit 1
fi
output="$(cd "$output" && pwd -P)"

# Validate the entire plan before generating any case. Restricting names also
# lets us safely write the relative paths into the Pyright JSON configuration.
directories=() inputs=() packages=() overrides=()
while read -r directory fixture package override extra || [[ -n "$directory" ]]; do
  case "$directory" in ''|\#*) continue ;; esac
  if [[ ! "$directory" =~ ^[a-z0-9_-]+(/[a-z0-9_-]+)*$ || "$directory" == typing || "$directory" == typing/* ||
        ! "$fixture" =~ ^[a-zA-Z0-9_-]+(/[a-zA-Z0-9_-]+)*\.(yaml|yml|json)$ || ! -f "$fixtures/$fixture" ||
        ! "$package" =~ ^[a-z_][a-z0-9_]*$ || -n "$extra" ]]; then
    echo "$manifest: invalid case: $directory $fixture $package $override $extra" >&2
    exit 1
  fi
  case "$override" in
    ''|untagged_union_strategy=firstMatch|untagged_union_strategy=strict|untagged_union_strategy=raw|tagged_union_strategy=firstMatch|tagged_union_strategy=strict|tagged_union_strategy=raw) ;;
    *) echo "$manifest: invalid generator override for $directory: $override" >&2; exit 1 ;;
  esac
  for previous in "${directories[@]+"${directories[@]}"}"; do
    if [[ "$directory" == "$previous" || "$directory" == "$previous/"* || "$previous" == "$directory/"* ]]; then
      echo "$manifest: overlapping case directories: $previous and $directory" >&2
      exit 1
    fi
  done
  for previous in "${packages[@]+"${packages[@]}"}"; do
    if [[ "$package" == "$previous" ]]; then
      echo "$manifest: duplicate import package: $package" >&2
      exit 1
    fi
  done
  directories+=("$directory")
  inputs+=("$fixtures/$fixture")
  packages+=("$package")
  overrides+=("$override")
done < "$manifest"
if [[ -z "${directories[0]:-}" ]]; then
  echo "$manifest: no fixtures selected" >&2
  exit 1
fi

for i in "${!directories[@]}"; do
  options=(--config "$root/tests/generated-clients/config.toml"
    --input "${inputs[$i]}" --output "$output/${directories[$i]}" --generators "$generator"
    --generator-config "$generator.package_name=${packages[$i]}")
  if [[ -n "${overrides[$i]}" ]]; then
    options+=(--generator-config "$generator.${overrides[$i]}")
  fi
  cargo run --quiet --manifest-path "$root/Cargo.toml" --bin openapi-nexus -- generate "${options[@]}"
  if [[ ! -d "$output/${directories[$i]}/${packages[$i]}" ]]; then
    echo "missing generated package: ${directories[$i]}/${packages[$i]}" >&2
    exit 1
  fi
done

mkdir -- "$output/typing"
cp -- "$project/"*typing_contracts.py "$output/typing/"
cp -- "$project/pyrightconfig.json" "$output/pyright-base.json"
printf '%s\n' "${directories[@]}" > "$output/import-roots.txt"
{
  printf '{\n  "extends": "./pyright-base.json",\n  "include": ["typing"'
  for i in "${!directories[@]}"; do
    printf ',\n    "%s/%s"' "${directories[$i]}" "${packages[$i]}"
  done
  printf '\n  ],\n  "extraPaths": ['
  separator=''
  for directory in "${directories[@]}"; do
    printf '%s\n    "%s"' "$separator" "$directory"
    separator=','
  done
  printf '\n  ]\n}\n'
} > "$output/pyrightconfig.json"
