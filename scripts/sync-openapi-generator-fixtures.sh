#!/usr/bin/env bash
# Sync OpenAPI Generator test fixtures (3_0, 3_1) into tests/openapi-generator-fixtures.
# Run from the repository root.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_BASE="${REPO_ROOT}/tests/openapi-generator-fixtures"
UPSTREAM_URL="https://github.com/OpenAPITools/openapi-generator.git"
RESOURCES_PATH="modules/openapi-generator/src/test/resources"
OPENAPI_GENERATOR_SYNC_TMP="${TMPDIR:-/tmp}/openapi-nexus-og-fixtures-sync-$$"

cleanup() {
  rm -rf "${OPENAPI_GENERATOR_SYNC_TMP}"
}
trap cleanup EXIT

mkdir -p "${OPENAPI_GENERATOR_SYNC_TMP}"
cd "${OPENAPI_GENERATOR_SYNC_TMP}"

echo "Cloning OpenAPI Generator (shallow)..."
git clone --depth 1 "${UPSTREAM_URL}" repo

SRC="${OPENAPI_GENERATOR_SYNC_TMP}/repo/${RESOURCES_PATH}"
if [[ ! -d "${SRC}/3_0" || ! -d "${SRC}/3_1" ]]; then
  echo "Expected 3_0 and 3_1 under ${SRC}" >&2
  exit 1
fi

echo "Copying 3_0 -> oas30..."
rm -rf "${TARGET_BASE}/oas30"
cp -R "${SRC}/3_0" "${TARGET_BASE}/oas30"

echo "Copying 3_1 -> oas31..."
rm -rf "${TARGET_BASE}/oas31"
cp -R "${SRC}/3_1" "${TARGET_BASE}/oas31"

echo "Done. Fixtures updated under ${TARGET_BASE}"
