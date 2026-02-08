#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/tests/_common.sh
source "$SCRIPT_DIR/_common.sh"

OUT_DIR="$(make_out_dir "02-subgraph-endpoint-intent")"

print_section "Building mmdflux"
ensure_mmdflux_bin

echo "Output dir: $OUT_DIR"

print_section "Render direct Mermaid SVG"
"$MMDFLUX_BIN" --format svg "$REPO_ROOT/tests/fixtures/flowchart/subgraph_to_subgraph_edge.mmd" \
  >"$OUT_DIR/direct-from-mermaid.svg"

print_section "Render MMDS SVG (endpoint intent present vs missing)"
"$MMDFLUX_BIN" --format svg "$REPO_ROOT/tests/fixtures/mmds/subgraph-endpoint-subgraph-to-subgraph-present.json" \
  >"$OUT_DIR/mmds-intent-present.svg"
"$MMDFLUX_BIN" --format svg "$REPO_ROOT/tests/fixtures/mmds/subgraph-endpoint-subgraph-to-subgraph-missing.json" \
  >"$OUT_DIR/mmds-intent-missing.svg"

print_section "Quick file stats for visual diff triage"
wc -c \
  "$OUT_DIR/direct-from-mermaid.svg" \
  "$OUT_DIR/mmds-intent-present.svg" \
  "$OUT_DIR/mmds-intent-missing.svg"

print_section "First path elements (compare rough routing)"
for f in \
  "$OUT_DIR/direct-from-mermaid.svg" \
  "$OUT_DIR/mmds-intent-present.svg" \
  "$OUT_DIR/mmds-intent-missing.svg"; do
  echo "-- $(basename "$f")"
  grep -n "<path" "$f" | sed -n '1,6p'
done

printf '\nOpen these side-by-side to inspect routing parity:\n'
echo "  $OUT_DIR/direct-from-mermaid.svg"
echo "  $OUT_DIR/mmds-intent-present.svg"
echo "  $OUT_DIR/mmds-intent-missing.svg"
