#!/usr/bin/env bash
set -euo pipefail

backend="${1:?Usage: golden-build-python.sh <httpx|requests>}"

root="$(cd "$(dirname "$0")/.." && pwd)"
golden="$root/tests/golden/python/python-$backend"

if [ ! -d "$golden" ]; then
  echo "error: golden dir not found: $golden" >&2
  exit 1
fi

total=0; passed=0; failed=0; fails=()

for test_dir in "$golden"/*/; do
  [ -f "$test_dir/pyproject.toml.golden" ] || continue
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

  pip3 install "$backend" --quiet --target "$tmp/.deps" 2>/dev/null

  cat > "$tmp/pyrightconfig.json" <<PYRIGHT
{
    "pythonVersion": "3.12",
    "typeCheckingMode": "strict",
    "extraPaths": [".deps"]
}
PYRIGHT

  log=$(mktemp)
  if (cd "$tmp" && npx pyright) >"$log" 2>&1; then
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
echo "Python-$backend: $passed/$total passed"
if [ "$failed" -gt 0 ]; then
  echo "failed:"
  for n in "${fails[@]}"; do echo "  - $n"; done
  exit 1
fi
