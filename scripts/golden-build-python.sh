#!/usr/bin/env bash
set -euo pipefail

backend="${1:?Usage: golden-build-python.sh <httpx|requests>}"

root="$(cd "$(dirname "$0")/.." && pwd)"
golden="$root/tests/golden/python/python-$backend"

# shellcheck source=./scripts/golden-build-common.sh
source "$root/scripts/golden-build-common.sh"
golden_require_projects "$golden" pyproject.toml.golden

# One dependency installation and checker resolution per backend, not per fixture.
deps=$(mktemp -d)
tmp=""; log=""
cleanup() {
  rm -rf "$deps"
  [ -z "$tmp" ] || rm -rf "$tmp"
  [ -z "$log" ] || rm -f "$log"
}
trap cleanup EXIT
"${PYTHON:-python3}" -m pip install "$backend" --quiet --target "$deps"
if command -v pyright >/dev/null 2>&1; then
  checker=$(command -v pyright)
else
  checker=$(npm exec --yes --package pyright -- which pyright)
fi

total=0; passed=0; failed=0; fails=()

for test_dir in "$golden"/*/; do
  [ -f "$test_dir/pyproject.toml.golden" ] || continue
  total=$((total + 1))
  name=$(basename "$test_dir")
  printf "[%d] %s... " "$total" "$name"

  tmp=$(mktemp -d)

  while IFS= read -r -d '' f; do
    rel="${f#"$test_dir"}"
    dst="$tmp/${rel%.golden}"
    mkdir -p "$(dirname "$dst")"
    cp "$f" "$dst"
  done < <(find "$test_dir" -type f -name '*.golden' -print0)

  cat > "$tmp/pyrightconfig.json" <<PYRIGHT
{
    "pythonVersion": "3.12",
    "typeCheckingMode": "strict",
    "extraPaths": ["$deps"],
    "reportUnnecessaryIsInstance": "none"
}
PYRIGHT

  log=$(mktemp)
  if (cd "$tmp" && "$checker") >"$log" 2>&1; then
    echo "ok"
    passed=$((passed + 1))
  else
    echo "FAIL"
    failed=$((failed + 1))
    fails+=("$name")
    sed 's/^/    /' "$log" || true
  fi

  rm -rf "$tmp" "$log"
  tmp=""; log=""
done

echo
golden_require_selection "$total"
echo "Python-$backend: $passed/$total passed"
if [ "$failed" -gt 0 ]; then
  echo "failed:"
  for n in "${fails[@]}"; do echo "  - $n"; done
  exit 1
fi
