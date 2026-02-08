#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/tests/_common.sh
source "$SCRIPT_DIR/_common.sh"

OUT_DIR="$(make_out_dir "01-mmds-ingest-validation")"

print_section "Building mmdflux"
ensure_mmdflux_bin

echo "Output dir: $OUT_DIR"

print_section "Mermaid -> MMDS (layout)"
"$MMDFLUX_BIN" --format mmds "$REPO_ROOT/tests/fixtures/flowchart/simple.mmd" \
  >"$OUT_DIR/simple.layout.mmds.json"
sed -n '1,80p' "$OUT_DIR/simple.layout.mmds.json"

print_section "MMDS -> text (valid layout fixture)"
"$MMDFLUX_BIN" "$REPO_ROOT/tests/fixtures/mmds/layout-valid-flowchart.json" \
  >"$OUT_DIR/layout-valid-flowchart.txt"
cat "$OUT_DIR/layout-valid-flowchart.txt"

print_section "Validation failures (expected)"
run_expect_fail \
  "dangling edge target" \
  "$OUT_DIR/invalid-dangling-edge-target" \
  "$MMDFLUX_BIN" "$REPO_ROOT/tests/fixtures/mmds/invalid/dangling-edge-target.json"

run_expect_fail \
  "missing node id" \
  "$OUT_DIR/invalid-missing-node-id" \
  "$MMDFLUX_BIN" "$REPO_ROOT/tests/fixtures/mmds/invalid/missing-node-id.json"

printf '\nSaved artifacts under: %s\n' "$OUT_DIR"
