#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/tests/_common.sh
source "$SCRIPT_DIR/_common.sh"

OUT_DIR="$(make_out_dir "05-roundtrip-and-conformance")"

print_section "Building mmdflux"
ensure_mmdflux_bin

echo "Output dir: $OUT_DIR"

print_section "Direct flowchart vs MMDS roundtrip (text + svg)"
"$MMDFLUX_BIN" --format mmds "$REPO_ROOT/tests/fixtures/flowchart/complex.mmd" \
  >"$OUT_DIR/complex.roundtrip.mmds.json"

"$MMDFLUX_BIN" "$REPO_ROOT/tests/fixtures/flowchart/complex.mmd" \
  >"$OUT_DIR/complex.direct.text.txt"
"$MMDFLUX_BIN" "$OUT_DIR/complex.roundtrip.mmds.json" \
  >"$OUT_DIR/complex.from-mmds.text.txt"

"$MMDFLUX_BIN" --format svg "$REPO_ROOT/tests/fixtures/flowchart/complex.mmd" \
  >"$OUT_DIR/complex.direct.svg"
"$MMDFLUX_BIN" --format svg "$OUT_DIR/complex.roundtrip.mmds.json" \
  >"$OUT_DIR/complex.from-mmds.svg"

if cmp -s "$OUT_DIR/complex.direct.text.txt" "$OUT_DIR/complex.from-mmds.text.txt"; then
  echo "text outputs are byte-identical for complex fixture"
else
  echo "text outputs differ; showing first diff hunk"
  diff -u "$OUT_DIR/complex.direct.text.txt" "$OUT_DIR/complex.from-mmds.text.txt" | sed -n '1,80p'
fi

print_section "Conformance summary"
(
  cd "$REPO_ROOT"
  just conformance
) | tee "$OUT_DIR/conformance.log"

print_section "Targeted MMDS suites"
(
  cd "$REPO_ROOT"
  just test-file mmds_roundtrip
  just test-file mmds_mermaid_generation
) | tee "$OUT_DIR/mmds-suites.log"

printf '\nInspect these artifacts with your eyes:\n'
echo "  $OUT_DIR/complex.direct.svg"
echo "  $OUT_DIR/complex.from-mmds.svg"
echo "  $OUT_DIR/conformance.log"
