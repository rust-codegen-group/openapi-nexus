#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
golden="$root/tests/golden/kotlin/kotlin-okhttp"

# shellcheck source=./scripts/golden-build-common.sh
source "$root/scripts/golden-build-common.sh"
golden_require_projects "$golden" build.gradle.kts.golden

tmp=$(mktemp -d)
log=$(mktemp)
trap 'rm -rf "$tmp" "$log"' EXIT

settings="$tmp/settings.gradle.kts"
cat > "$settings" <<'SETTINGS'
plugins {
    id("org.gradle.toolchains.foojay-resolver-convention") version "1.0.0"
}
rootProject.name = "golden-kotlin"
SETTINGS

total=0
tasks=()

for test_dir in "$golden"/*/; do
  [ -f "$test_dir/build.gradle.kts.golden" ] || continue
  total=$((total + 1))
  name=$(basename "$test_dir")
  tasks+=(":$name:compileKotlin")

  mkdir -p "$tmp/$name"
  while IFS= read -r -d '' f; do
    rel="${f#"$test_dir"}"
    dst="$tmp/$name/${rel%.golden}"
    mkdir -p "$(dirname "$dst")"
    cp "$f" "$dst"
  done < <(find "$test_dir" -type f -name '*.golden' -print0)

  cat >> "$settings" <<SETTINGS
include(":$name")
project(":$name").projectDir = file("$name")
SETTINGS
done

echo
golden_require_selection "$total"

echo "Kotlin: compiling $total golden projects"
if (cd "$tmp" && gradle --no-daemon --quiet --parallel --continue "${tasks[@]}") >"$log" 2>&1; then
  echo "Kotlin: $total/$total passed"
else
  echo "Kotlin: build failed"
  sed 's/^/    /' "$log" || true
  exit 1
fi
