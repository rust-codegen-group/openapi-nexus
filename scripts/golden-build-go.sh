#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
golden="$root/tests/golden/go/go-http"

# shellcheck source=./scripts/golden-build-common.sh
source "$root/scripts/golden-build-common.sh"
golden_require_projects "$golden" go.mod.golden

total=0; passed=0; failed=0; fails=()

for test_dir in "$golden"/*/; do
  [ -f "$test_dir/go.mod.golden" ] || continue
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
  if (cd "$tmp" && go build ./...) >"$log" 2>&1; then
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
done

echo
golden_require_selection "$total"
echo "Go: $passed/$total passed"
if [ "$failed" -gt 0 ]; then
  echo "failed:"
  for n in "${fails[@]}"; do echo "  - $n"; done
  exit 1
fi
