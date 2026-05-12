#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
golden="$root/tests/golden/typescript/typescript-fetch"

if [ ! -d "$golden" ]; then
  echo "error: golden dir not found: $golden" >&2
  exit 1
fi

# Detect available type checker
if command -v vp &>/dev/null; then
  USE_VP=1
else
  USE_VP=0
  echo "note: vp not found, falling back to tsc --noEmit"
fi

total=0; passed=0; failed=0; fails=()

for test_dir in "$golden"/*/; do
  [ -f "$test_dir/tsconfig.json.golden" ] || continue
  total=$((total + 1))
  name=$(basename "$test_dir")
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
  if [ "$USE_VP" = "1" ]; then
    if [ -f "$tmp/vite.config.ts" ]; then
      (cd "$tmp" && vp install) >>"$log" 2>&1 || true
    fi
    if (cd "$tmp" && vp check --no-fmt && vp run build) >>"$log" 2>&1; then
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
done

echo
echo "TypeScript: $passed/$total passed"
if [ "$failed" -gt 0 ]; then
  echo "failed:"
  for n in "${fails[@]}"; do echo "  - $n"; done
  exit 1
fi
