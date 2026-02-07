#!/bin/bash

# Compare output of two mmdflux binaries against all test fixtures

BINARY1="${BINARY1:-$HOME/src/mmdflux/target/debug/mmdflux}"
BINARY2="${BINARY2:-$HOME/src/mmdflux-4a-brandes-kopf/target/debug/mmdflux}"
FIXTURES_DIR="${FIXTURES_DIR:-$HOME/src/mmdflux/tests/fixtures/flowchart}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Counters
identical=0
different=0
errors=0
total=0

# Arrays to track results
declare -a identical_files
declare -a different_files
declare -a error_files

echo "========================================"
echo "mmdflux Binary Comparison"
echo "========================================"
echo ""
echo "Binary 1: $BINARY1"
echo "Binary 2: $BINARY2"
echo "Fixtures: $FIXTURES_DIR"
echo ""

# Check binaries exist
if [[ ! -x "$BINARY1" ]]; then
    echo -e "${RED}Error: Binary 1 not found or not executable: $BINARY1${NC}"
    exit 1
fi

if [[ ! -x "$BINARY2" ]]; then
    echo -e "${RED}Error: Binary 2 not found or not executable: $BINARY2${NC}"
    exit 1
fi

# Check fixtures directory exists
if [[ ! -d "$FIXTURES_DIR" ]]; then
    echo -e "${RED}Error: Fixtures directory not found: $FIXTURES_DIR${NC}"
    exit 1
fi

echo "----------------------------------------"
echo "Comparing outputs..."
echo "----------------------------------------"
echo ""

# Process each .mmd file
for fixture in "$FIXTURES_DIR"/*.mmd; do
    if [[ ! -f "$fixture" ]]; then
        continue
    fi

    filename=$(basename "$fixture")
    ((total++))

    # Run both binaries and capture output
    output1=$("$BINARY1" "$fixture" 2>&1)
    exit1=$?

    output2=$("$BINARY2" "$fixture" 2>&1)
    exit2=$?

    # Check for errors
    if [[ $exit1 -ne 0 && $exit2 -ne 0 ]]; then
        echo -e "${YELLOW}⚠ $filename${NC} - Both binaries failed"
        ((errors++))
        error_files+=("$filename (both failed)")
        continue
    elif [[ $exit1 -ne 0 ]]; then
        echo -e "${YELLOW}⚠ $filename${NC} - Binary 1 failed (exit $exit1)"
        ((errors++))
        error_files+=("$filename (binary 1 failed)")
        continue
    elif [[ $exit2 -ne 0 ]]; then
        echo -e "${YELLOW}⚠ $filename${NC} - Binary 2 failed (exit $exit2)"
        ((errors++))
        error_files+=("$filename (binary 2 failed)")
        continue
    fi

    # Compare outputs
    if [[ "$output1" == "$output2" ]]; then
        echo -e "${GREEN}✓ $filename${NC} - identical"
        ((identical++))
        identical_files+=("$filename")
    else
        echo -e "${RED}✗ $filename${NC} - DIFFERENT"
        ((different++))
        different_files+=("$filename")
    fi
done

echo ""
echo "========================================"
echo "DIFFERENCES"
echo "========================================"

# Show diffs for files that differ
if [[ ${#different_files[@]} -gt 0 ]]; then
    for filename in "${different_files[@]}"; do
        fixture="$FIXTURES_DIR/$filename"
        echo ""
        echo -e "${BLUE}--- $filename ---${NC}"
        echo ""

        # Get outputs again for diff
        output1=$("$BINARY1" "$fixture" 2>&1)
        output2=$("$BINARY2" "$fixture" 2>&1)

        # Create temp files for diff
        tmp1=$(mktemp)
        tmp2=$(mktemp)
        echo "$output1" > "$tmp1"
        echo "$output2" > "$tmp2"

        # Show unified diff with labels
        diff -u --label "mmdflux (original)" --label "mmdflux (brandes-kopf)" "$tmp1" "$tmp2" || true

        # Cleanup
        rm -f "$tmp1" "$tmp2"

        echo ""
    done
else
    echo ""
    echo "No differences found!"
fi

echo ""
echo "========================================"
echo "SUMMARY"
echo "========================================"
echo ""
echo -e "Total fixtures:  $total"
echo -e "${GREEN}Identical:       $identical${NC}"
echo -e "${RED}Different:       $different${NC}"
if [[ $errors -gt 0 ]]; then
    echo -e "${YELLOW}Errors:          $errors${NC}"
fi
echo ""
echo -e "${BLUE}$identical of $total fixtures are identical, $different differ${NC}"

if [[ $errors -gt 0 ]]; then
    echo ""
    echo "Fixtures with errors:"
    for f in "${error_files[@]}"; do
        echo "  - $f"
    done
fi

echo ""

# Exit with non-zero if there are differences
if [[ $different -gt 0 || $errors -gt 0 ]]; then
    exit 1
fi
exit 0
