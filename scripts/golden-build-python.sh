#!/usr/bin/env bash
set -euo pipefail

backend="${1:?Usage: golden-build-python.sh <httpx|requests> [--compile-only]}"
case "${2:-}" in
  ''|--compile-only) ;;
  *) echo 'expected --compile-only or no second argument' >&2; exit 2 ;;
esac

root="$(cd "$(dirname "$0")/.." && pwd)"
golden="$root/tests/golden/python/python-$backend"

# shellcheck source=./scripts/golden-build-common.sh
source "$root/scripts/golden-build-common.sh"
projects=$(golden_select_projects "$root" "python-$backend" "$golden" pyproject.toml.golden)

# One active-interpreter environment for goldens and fresh native consumers.
deps=$(mktemp -d)
tmp=""; log=""
cleanup() {
  rm -rf "$deps"
  [ -z "$tmp" ] || rm -rf "$tmp"
  [ -z "$log" ] || rm -f "$log"
}
trap cleanup EXIT
"${PYTHON:-python3}" -m venv "$deps"
python="$deps/bin/python"
"$python" -m pip install "$backend" --quiet
if command -v pyright >/dev/null 2>&1; then
  checker=$(command -v pyright)
else
  checker=$(npm exec --yes --package pyright -- which pyright)
fi

total=0; passed=0; failed=0; fails=()

while IFS= read -r name <&3; do
  test_dir="$golden/$name/"
  total=$((total + 1))
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
    "reportUnnecessaryIsInstance": "none"
}
PYRIGHT

  log=$(mktemp)
  if (cd "$tmp" && "$checker" --pythonpath "$python") >"$log" 2>&1; then
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
done 3<<< "$projects"

echo
echo "Python-$backend: $passed/$total passed"
if [ "$failed" -gt 0 ]; then
  echo "failed:"
  for n in "${fails[@]}"; do echo "  - $n"; done
  exit 1
fi

if [[ "${2:-}" != --compile-only ]]; then
  # The combined entrypoint reuses the same interpreter and checker.
  export PYTHON="$python" PYRIGHT="$checker"
  (cd "$root" && just golden-python-clients::test "$backend")
fi
