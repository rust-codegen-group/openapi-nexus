#!/usr/bin/env bash
set -euo pipefail

backend="${1:?Usage: generate-rust-client-fixtures.sh <reqwest|ureq|aioduct>}"
case "$backend" in
  reqwest|ureq|aioduct) ;;
  *) echo "unsupported Rust backend: $backend" >&2; exit 1 ;;
esac

root="$(cd "$(dirname "$0")/.." && pwd -P)"
rust="$root/tests/generated-clients/rust"
manifest="$rust/fixtures.txt"
project="$rust/$backend"
fixtures="$root/tests/fixtures/valid"
generator="rust-$backend"

fail() {
  echo "$manifest:$line_number: $1" >&2
  exit 1
}

# Preflight the plan before replacing any generated Cargo dependency.
rows=() names=()
line_number=0
while IFS= read -r line || [[ -n "$line" ]]; do
  ((line_number += 1))
  read -r -a fields <<< "$line"
  scope="${fields[0]:-}"
  case "$scope" in
    ''|\#*) continue ;;
    all|reqwest|ureq|aioduct) ;;
    *) fail "unknown backend: $scope" ;;
  esac
  name="${fields[1]:-}"
  fixture="${fields[2]:-}"
  profile="${fields[3]:-}"
  [[ "$name" =~ ^[a-z0-9]+(-[a-z0-9]+)*$ ]] || fail "invalid output basename: $name"
  [[ "$fixture" =~ ^[a-z0-9_-]+\.(yaml|yml|json)$ && -f "$fixtures/$fixture" ]] || fail "invalid or missing fixture: $fixture"
  case "$profile" in standard|policies) ;; *) fail "invalid config profile: $profile" ;; esac
  keys=' '
  for option in "${fields[@]:4}"; do
    key="${option%%=*}"
    case "$option" in
      crate_name=*) [[ "${option#*=}" =~ ^[a-z][a-z0-9_-]*$ ]] || fail "invalid crate name: $option" ;;
      untagged_union_strategy=firstMatch|untagged_union_strategy=strict|untagged_union_strategy=raw|tagged_union_strategy=firstMatch|tagged_union_strategy=strict|tagged_union_strategy=raw) ;;
      *) fail "invalid generator option: $option" ;;
    esac
    [[ "$keys" != *" $key "* ]] || fail "duplicate generator option: $key"
    keys+="$key "
  done
  if [[ "$scope" == all || "$scope" == "$backend" ]]; then
    for previous in "${names[@]+"${names[@]}"}"; do
      [[ "$name" != "$previous" ]] || fail "duplicate output basename for $backend: $name"
    done
    names+=("$name")
    rows+=("$line")
  fi
done < "$manifest"
[[ -n "${rows[0]:-}" ]] || fail "no fixtures for $backend"

[[ "$(cd "$project" && pwd -P)" == "$project" ]]
output="$project/generated"
if [[ -e "$output" || -L "$output" ]]; then
  [[ -d "$output" && ! -L "$output" && "$(cd "$output" && pwd -P)" == "$output" ]]
fi
stage="$(mktemp -d "$project/.nexus-rust.XXXXXX")"
cleanup() {
  if [[ -d "$stage" && ! -L "$stage" && "$(cd "$stage" && pwd -P)" == "$stage" ]]; then
    rm -rf -- "$stage"
  fi
}
trap cleanup EXIT

for row in "${rows[@]}"; do
  read -r -a fields <<< "$row"
  case "${fields[3]}" in
    standard) config="$root/tests/generated-clients/config.toml" ;;
    policies) config="$rust/union-policies.toml" ;;
  esac
  dest="$stage/${fields[1]}"
  options=(--config "$config" --input "$fixtures/${fields[2]}" --output "$dest" --generators "$generator")
  for option in "${fields[@]:4}"; do
    options+=(--generator-config "$generator.$option")
  done
  cargo run --quiet --manifest-path "$root/Cargo.toml" --bin openapi-nexus -- generate "${options[@]}"
  if [[ ! -f "$dest/Cargo.toml" ]]; then
    echo "missing generated Cargo manifest: $dest/Cargo.toml" >&2
    exit 1
  fi
done

if [[ -e "$output" || -L "$output" ]]; then
  [[ -d "$output" && ! -L "$output" && "$(cd "$output" && pwd -P)" == "$output" ]]
  rm -rf -- "$output"
fi
[[ "$(cd "$stage" && pwd -P)" == "$stage" ]]
mv -- "$stage" "$output"
