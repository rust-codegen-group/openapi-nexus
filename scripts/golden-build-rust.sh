#!/usr/bin/env bash
set -euo pipefail

backend="${1:?Usage: golden-build-rust.sh <reqwest|ureq|aioduct>}"

root="$(cd "$(dirname "$0")/.." && pwd)"
golden="$root/tests/golden/rust/rust-$backend"

# shellcheck source=./scripts/golden-build-common.sh
source "$root/scripts/golden-build-common.sh"
projects=$(golden_select_projects "$root" "rust-$backend" "$golden" Cargo.toml.golden)

shared_target="${CARGO_TARGET_DIR:-$root/target/golden-build/rust-$backend}"
case "$shared_target" in
  /*) ;;
  *) shared_target="$PWD/$shared_target" ;;
esac
mkdir -p "$shared_target"
total=0; passed=0; failed=0; fails=()

while IFS= read -r name <&3; do
  test_dir="$golden/$name/"
  total=$((total + 1))
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
  if (cd "$tmp" && CARGO_TARGET_DIR="$shared_target" cargo check) >"$log" 2>&1; then
    echo "ok"
    passed=$((passed + 1))
  else
    echo "FAIL"
    failed=$((failed + 1))
    fails+=("$name")
    sed 's/^/    /' "$log" || true
  fi

  rm -rf "$tmp" "$log"
  trap - EXIT
done 3<<< "$projects"

echo
echo "Rust-$backend: $passed/$total passed"
if [ "$failed" -gt 0 ]; then
  echo "failed:"
  for n in "${fails[@]}"; do echo "  - $n"; done
  exit 1
fi
