#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/tests/_common.sh
source "$SCRIPT_DIR/_common.sh"

OUT_DIR="$(make_out_dir "03-positioned-capability-matrix")"

print_section "Building mmdflux"
ensure_mmdflux_bin

echo "Output dir: $OUT_DIR"

print_section "Layout-level MMDS supports text and svg"
"$MMDFLUX_BIN" "$REPO_ROOT/tests/fixtures/mmds/positioned/layout-basic.json" \
  >"$OUT_DIR/layout-basic.text.txt"
"$MMDFLUX_BIN" --format svg "$REPO_ROOT/tests/fixtures/mmds/positioned/layout-basic.json" \
  >"$OUT_DIR/layout-basic.svg"
cat "$OUT_DIR/layout-basic.text.txt"

print_section "Routed-level MMDS supports svg"
"$MMDFLUX_BIN" --format svg "$REPO_ROOT/tests/fixtures/mmds/positioned/routed-basic.json" \
  >"$OUT_DIR/routed-basic.svg"
wc -c "$OUT_DIR/routed-basic.svg"

print_section "Routed-level MMDS rejects text/ascii (expected)"
run_expect_fail \
  "routed fixture rendered as text" \
  "$OUT_DIR/routed-text-rejected" \
  "$MMDFLUX_BIN" "$REPO_ROOT/tests/fixtures/mmds/positioned/routed-basic.json"

run_expect_fail \
  "routed fixture rendered as ascii" \
  "$OUT_DIR/routed-ascii-rejected" \
  "$MMDFLUX_BIN" --format ascii "$REPO_ROOT/tests/fixtures/mmds/positioned/routed-basic.json"

if ! grep -q "use --format svg" "$OUT_DIR/routed-text-rejected.stderr"; then
  echo "expected text rejection to include remediation guidance"
  exit 1
fi

if ! grep -q "use --format svg" "$OUT_DIR/routed-ascii-rejected.stderr"; then
  echo "expected ascii rejection to include remediation guidance"
  exit 1
fi

printf '\nSaved artifacts under: %s\n' "$OUT_DIR"
