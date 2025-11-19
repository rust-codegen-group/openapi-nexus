#!/bin/bash
# Check TypeScript build status for all golden test directories
#
# This script runs `tsc --noEmit` on each golden test directory in
# tests/golden/typescript/ to verify that all generated TypeScript code
# compiles without errors.

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Get the script directory and project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
GOLDEN_DIR="$PROJECT_ROOT/tests/golden/typescript/typescript-fetch"

# Check if TypeScript compiler is available
if ! command -v tsc &> /dev/null; then
    echo -e "${RED}Error: tsc (TypeScript compiler) not found in PATH${NC}"
    echo "Please install TypeScript: npm install -g typescript"
    exit 1
fi

# Check if golden directory exists
if [ ! -d "$GOLDEN_DIR" ]; then
    echo -e "${RED}Error: Golden test directory not found: $GOLDEN_DIR${NC}"
    exit 1
fi

# Arrays to track results
declare -a PASSED_TESTS
declare -a FAILED_TESTS

# Counter for progress
TOTAL_TESTS=0
PASSED_COUNT=0
FAILED_COUNT=0

echo "Checking TypeScript build status for all golden tests..."
echo "Golden test directory: $GOLDEN_DIR"
echo ""

# Iterate through all subdirectories in the golden directory
while IFS= read -r -d '' test_dir; do
    # Get just the directory name (not full path)
    test_name=$(basename "$test_dir")
    
    # Skip if it's not a directory or doesn't have a tsconfig.json.golden
    if [ ! -d "$test_dir" ] || [ ! -f "$test_dir/tsconfig.json.golden" ]; then
        continue
    fi
    
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    
    echo -n "[$TOTAL_TESTS] Checking $test_name... "
    
    # Create a temporary directory for this test
    TEMP_DIR=$(mktemp -d)
    
    # Copy all files from test directory to temp directory, removing .golden suffix
    while IFS= read -r -d '' golden_file; do
        # Get relative path from test_dir
        rel_path="${golden_file#$test_dir/}"
        # Remove .golden suffix
        actual_rel_path="${rel_path%.golden}"
        # Create destination path in temp directory
        dest_file="$TEMP_DIR/$actual_rel_path"
        # Create parent directories if needed
        mkdir -p "$(dirname "$dest_file")"
        # Copy the file
        cp "$golden_file" "$dest_file"
    done < <(find "$test_dir" -type f -name "*.golden" -print0)
    
    # Change to the temp directory and run tsc --noEmit
    # Capture both stdout and stderr
    if (cd "$TEMP_DIR" && tsc --noEmit > /tmp/tsc_output_$$.txt 2>&1); then
        echo -e "${GREEN}✓ PASSED${NC}"
        PASSED_TESTS+=("$test_name")
        PASSED_COUNT=$((PASSED_COUNT + 1))
    else
        echo -e "${RED}✗ FAILED${NC}"
        FAILED_TESTS+=("$test_name")
        FAILED_COUNT=$((FAILED_COUNT + 1))
        
        # Show error output
        echo -e "${YELLOW}  Error output:${NC}"
        sed 's/^/  /' /tmp/tsc_output_$$.txt || true
        echo ""
    fi
    
    # Clean up temp directory and temp file
    rm -rf "$TEMP_DIR"
    rm -f /tmp/tsc_output_$$.txt
    
done < <(find "$GOLDEN_DIR" -mindepth 1 -maxdepth 1 -type d -print0 | sort -z)

echo ""
echo "=========================================="
echo "Summary"
echo "=========================================="
echo -e "Total tests: $TOTAL_TESTS"
echo -e "${GREEN}Passed: $PASSED_COUNT${NC}"
if [ $FAILED_COUNT -gt 0 ]; then
    echo -e "${RED}Failed: $FAILED_COUNT${NC}"
    echo ""
    echo -e "${RED}Failed tests:${NC}"
    for test in "${FAILED_TESTS[@]}"; do
        echo "  - $test"
    done
else
    echo -e "${GREEN}Failed: 0${NC}"
fi
echo "=========================================="

# Exit with non-zero code if any tests failed
if [ $FAILED_COUNT -gt 0 ]; then
    exit 1
else
    exit 0
fi
