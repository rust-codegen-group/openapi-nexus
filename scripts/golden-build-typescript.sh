#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
golden="$root/tests/golden/typescript/typescript-fetch"

# shellcheck source=./scripts/golden-build-common.sh
source "$root/scripts/golden-build-common.sh"
toolchain="${1:-}"
if [ "$toolchain" != "tsc" ] && [ "$toolchain" != "vp" ]; then
  echo "usage: $0 <tsc|vp>" >&2
  exit 1
fi
projects=$(golden_select_projects "$root" typescript-fetch "$golden" tsconfig.json.golden "$toolchain")

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
  if [ "$toolchain" = "vp" ]; then
    if (cd "$tmp" && vp install && vp check && vp pack && vp check) >>"$log" 2>&1; then
      echo "ok"
      passed=$((passed + 1))
    else
      echo "FAIL"
      failed=$((failed + 1))
      fails+=("$name")
      sed 's/^/    /' "$log" || true
    fi
  else
    if (cd "$tmp" && tsc --noEmit) >>"$log" 2>&1; then
      echo "ok"
      passed=$((passed + 1))
    else
      echo "FAIL"
      failed=$((failed + 1))
      fails+=("$name")
      sed 's/^/    /' "$log" || true
    fi
  fi

  rm -rf "$tmp" "$log"
  trap - EXIT
done 3<<< "$projects"

echo
echo "TypeScript ($toolchain): $passed/$total passed"
if [ "$failed" -gt 0 ]; then
  echo "failed:"
  for n in "${fails[@]}"; do echo "  - $n"; done
  exit 1
fi
