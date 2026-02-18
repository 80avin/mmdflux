#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/tests/_common.sh
source "$SCRIPT_DIR/_common.sh"

OUT_DIR="$(make_out_dir "06-plan-0075-routing-preview-qa")"

run_exact_test() {
  local test_bin="$1"
  local test_name="$2"
  local output

  output="$(
    cd "$REPO_ROOT"
    cargo test --test "$test_bin" "$test_name" -- --exact
  )"
  printf '%s\n' "$output"

  if ! grep -q "running 1 test" <<<"$output"; then
    echo "FAILED expected exactly one matching test: $test_bin::$test_name"
    exit 1
  fi
}

render_mmds() {
  local mode="$1"
  local fixture="$2"
  local out_file="$3"
  "$MMDFLUX_BIN" \
    --format mmds \
    --geometry-level routed \
    --edge-routing "$mode" \
    "$REPO_ROOT/tests/fixtures/flowchart/$fixture" >"$out_file"
}

assert_same() {
  local fixture="$1"
  local base="$OUT_DIR/${fixture%.mmd}"
  local full="$base.full-compute.mmds.json"
  local unified="$base.unified-preview.mmds.json"
  local diff_file="$base.same-check.diff"

  render_mmds "full-compute" "$fixture" "$full"
  render_mmds "unified-preview" "$fixture" "$unified"

  if cmp -s "$full" "$unified"; then
    echo "OK same: $fixture"
  else
    diff -u "$full" "$unified" >"$diff_file" || true
    echo "FAILED expected same but got delta: $fixture"
    echo "diff: $diff_file"
    exit 1
  fi
}

assert_diff() {
  local fixture="$1"
  local base="$OUT_DIR/${fixture%.mmd}"
  local full="$base.full-compute.mmds.json"
  local unified="$base.unified-preview.mmds.json"
  local diff_file="$base.expected-delta.diff"

  render_mmds "full-compute" "$fixture" "$full"
  render_mmds "unified-preview" "$fixture" "$unified"

  if cmp -s "$full" "$unified"; then
    echo "FAILED expected delta but outputs are identical: $fixture"
    exit 1
  fi

  diff -u "$full" "$unified" >"$diff_file" || true
  echo "OK expected delta: $fixture"
  echo "diff: $diff_file"
}

print_section "Building mmdflux"
ensure_mmdflux_bin
echo "Output dir: $OUT_DIR"

print_section "Parity + rollback gates (plan 0075 targeted tests)"
(
  run_exact_test "svg_snapshots" "svg_full_compute_override_matches_legacy_straight_core_subset"
  run_exact_test "svg_snapshots" "svg_unified_preview_parity_fixture_subset_matches_expected_classification"
  run_exact_test "routed_geometry" "unified_preview_preserves_core_routed_geometry_contracts"
  run_exact_test "routed_geometry" "unified_router_produces_axis_aligned_forward_paths"
  run_exact_test "routed_geometry" "snap_path_to_grid_preserves_start_and_end_nodes"
  run_exact_test "integration" "test_svg_unified_preview_differs_from_legacy_for_cycle_fixture"
) | tee "$OUT_DIR/targeted-gates.log"

print_section "MMDS routed mode checks (full-compute vs unified-preview)"
assert_diff "simple.mmd"
assert_diff "chain.mmd"
assert_diff "simple_cycle.mmd"

print_section "Determinism checks"
"$MMDFLUX_BIN" \
  --format svg \
  --edge-style straight \
  --edge-routing unified-preview \
  "$REPO_ROOT/tests/fixtures/flowchart/simple_cycle.mmd" >"$OUT_DIR/simple_cycle.unified-preview.run1.svg"
"$MMDFLUX_BIN" \
  --format svg \
  --edge-style straight \
  --edge-routing unified-preview \
  "$REPO_ROOT/tests/fixtures/flowchart/simple_cycle.mmd" >"$OUT_DIR/simple_cycle.unified-preview.run2.svg"
cmp -s "$OUT_DIR/simple_cycle.unified-preview.run1.svg" "$OUT_DIR/simple_cycle.unified-preview.run2.svg"
echo "OK deterministic SVG unified-preview"

render_mmds "unified-preview" "simple_cycle.mmd" "$OUT_DIR/simple_cycle.unified-preview.run1.mmds.json"
render_mmds "unified-preview" "simple_cycle.mmd" "$OUT_DIR/simple_cycle.unified-preview.run2.mmds.json"
cmp -s "$OUT_DIR/simple_cycle.unified-preview.run1.mmds.json" "$OUT_DIR/simple_cycle.unified-preview.run2.mmds.json"
echo "OK deterministic MMDS unified-preview"

print_section "Full regression gates"
(
  cd "$REPO_ROOT"
  just test-file integration
  just test-file svg_render
  just test-file svg_snapshots
  just test-file routed_geometry
  cargo test svg_snapshot_all_fixtures
  cargo test svg_snapshot_orthogonal_fixture_subset
  just lint
) | tee "$OUT_DIR/full-gates.log"

printf '\nPlan 0075 QA checks passed.\n'
printf 'Artifacts: %s\n' "$OUT_DIR"
