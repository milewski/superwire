#!/bin/bash

# Test runner for all example workflows
# This script runs each workflow and reports success/failure

set -e

EXAMPLE_DIR="crates/example/workflows"
BINARY="target/debug/engine-ai-example"

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "Building project..."
cargo build --quiet

echo ""
echo "=========================================="
echo "Running AI Engine DSL Example Workflows"
echo "=========================================="
echo ""

PASSED=0
FAILED=0
TOTAL=0

# Find all .engine.ai files and sort them
for workflow in $(find "$EXAMPLE_DIR" -name "*.engine.ai" | sort); do
    TOTAL=$((TOTAL + 1))
    filename=$(basename "$workflow")

    echo -n "Testing $filename... "

    if RUST_LOG=error "$BINARY" "$workflow" > /tmp/workflow_output.json 2>&1; then
        echo -e "${GREEN}✓ PASSED${NC}"
        PASSED=$((PASSED + 1))

        # Show output summary
        if [ -s /tmp/workflow_output.json ]; then
            echo "  Output: $(cat /tmp/workflow_output.json | jq -c . 2>/dev/null || cat /tmp/workflow_output.json | head -c 100)"
        fi
    else
        echo -e "${RED}✗ FAILED${NC}"
        FAILED=$((FAILED + 1))
        echo "  Error output:"
        cat /tmp/workflow_output.json | head -20 | sed 's/^/    /'
    fi
    echo ""
done

echo "=========================================="
echo "Test Summary"
echo "=========================================="
echo "Total:  $TOTAL"
echo -e "Passed: ${GREEN}$PASSED${NC}"
echo -e "Failed: ${RED}$FAILED${NC}"
echo ""

if [ $FAILED -eq 0 ]; then
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
else
    echo -e "${RED}Some tests failed.${NC}"
    exit 1
fi
