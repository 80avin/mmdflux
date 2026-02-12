#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/tests/_common.sh
source "$SCRIPT_DIR/_common.sh"

OUT_DIR="$(make_out_dir "07-plan-0076-unified-routing-quality-qa")"
PROMOTION_DOC="$REPO_ROOT/docs/UNIFIED_ROUTING_PROMOTION.md"
ISSUES_DOC="$REPO_ROOT/docs/UNIFIED_ISSUES.md"

print_section "Building mmdflux"
ensure_mmdflux_bin
echo "Output dir: $OUT_DIR"

print_section "Targeted parity/determinism/path-detail gates"
(
  cd "$REPO_ROOT"
  cargo test --test svg_snapshots svg_unified_preview_parity_fixture_subset_matches_expected_classification -- --exact
  cargo test --test svg_snapshots unified_preview_svg_output_is_deterministic_for_fixture_subset -- --exact
  cargo test --test mmds_json unified_preview_mmds_routed_output_is_deterministic_for_fixture_subset -- --exact
  cargo test --test svg_render routed_svg_defaults_to_full_path_detail -- --exact
  cargo test --test mmds_json routed_mmds_defaults_to_full_path_detail -- --exact
  cargo test --test svg_render path_detail_monotonicity_holds_full_compact_simplified -- --exact
  cargo test --test mmds_json path_detail_monotonicity_holds_full_compact_simplified -- --exact
  cargo test --test svg_snapshots q4_rank_span_toggle_fixture_subset_matches_expected_classification -- --exact
  cargo test --test svg_snapshots q4_rank_span_policy_respects_q6_metric_gates_for_fixture_subset -- --exact
  cargo test --test routed_geometry q5_styled_segment_monitor_reports_actionable_summary_for_routed_geometry -- --exact
  cargo test --test svg_render q5_styled_segment_monitor_reports_actionable_summary_for_svg -- --exact
  cargo test --test svg_snapshots promotion_record_has_fixture_classification_and_rollback_validation -- --exact
) | tee "$OUT_DIR/targeted-gates.log"

print_section "Promotion record marker checks"
for marker in \
  "### Final Fixture Classification (Task 5.1)" \
  "### Rollback Playbook (Task 5.1)" \
  "--routing-mode full-compute" \
  "--policy-q1 off" \
  "--policy-q4 off" \
  "--policy-q5 off"; do
  if ! rg -F -- "$marker" "$PROMOTION_DOC" >/dev/null; then
    echo "Missing promotion-record marker: $marker" >&2
    exit 1
  fi
done

if ! rg -F -- "Task 5.1 Promotion + Rollback Finalization" "$ISSUES_DOC" >/dev/null; then
  echo "Missing Task 5.1 linkage in $ISSUES_DOC" >&2
  exit 1
fi

echo "Promotion record marker checks passed." | tee "$OUT_DIR/promotion-marker-gates.log"

print_section "Plan 0076 validation matrix"
(
  cd "$REPO_ROOT"
  cargo test --test routed_geometry
  cargo test --test svg_render
  cargo test --test mmds_json
  cargo test --test svg_snapshots
  cargo test svg_snapshot_all_fixtures
  cargo test svg_snapshot_orthogonal_fixture_subset
  just test-file integration
  just test-file svg_render
  just test-file svg_snapshots
  just test-file routed_geometry
  just lint
) | tee "$OUT_DIR/matrix.log"

printf '\nPlan 0076 QA checks passed.\n'
printf 'Artifacts: %s\n' "$OUT_DIR"
