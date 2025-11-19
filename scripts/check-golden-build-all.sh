#!/bin/bash
# Check build status for all golden test directories (TypeScript and Go)
#
# This script runs both TypeScript and Go build checks on all golden test
# directories to verify that all generated code compiles without errors.

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Get the script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Track overall results
OVERALL_FAILED=0

echo -e "${BLUE}==========================================${NC}"
echo -e "${BLUE}Golden Build Check - All Generators${NC}"
echo -e "${BLUE}==========================================${NC}"
echo ""

# Run TypeScript build check
echo -e "${BLUE}Running TypeScript Fetch build checks...${NC}"
echo ""
if "$SCRIPT_DIR/check-golden-build-typescript-fetch.sh"; then
    echo -e "${GREEN}✓ TypeScript Fetch checks passed${NC}"
else
    echo -e "${RED}✗ TypeScript Fetch checks failed${NC}"
    OVERALL_FAILED=1
fi
echo ""

# Run Go build check
echo -e "${BLUE}Running Go HTTP build checks...${NC}"
echo ""
if "$SCRIPT_DIR/check-golden-build-go-http.sh"; then
    echo -e "${GREEN}✓ Go HTTP checks passed${NC}"
else
    echo -e "${RED}✗ Go HTTP checks failed${NC}"
    OVERALL_FAILED=1
fi
echo ""

# Final summary
echo -e "${BLUE}==========================================${NC}"
echo -e "${BLUE}Overall Summary${NC}"
echo -e "${BLUE}==========================================${NC}"
if [ $OVERALL_FAILED -eq 0 ]; then
    echo -e "${GREEN}All build checks passed!${NC}"
    exit 0
else
    echo -e "${RED}Some build checks failed${NC}"
    exit 1
fi
