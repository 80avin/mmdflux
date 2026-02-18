# Unified Routing Promotion Decision Record

Use this decision record when promoting unified routing from preview to the default behavior.

This checklist assumes you are willing to ship breaking rendering deltas in a minor release.

## Decision Record (Required)

- [x] Promotion scope is explicit:
  - Flowchart only, or graph-family wide.
- [x] Backward-edge policy is explicit:
  - Keep hint fallback as intentional behavior, or fully migrate backward-edge routing.
- [x] Accepted deltas are documented:
  - Which fixtures can differ and why.
- [x] Rollback policy is documented:
  - Keep `--edge-routing full-compute` available for at least one release.

## Current Decision (2026-02-11)

- [x] Promotion scope is explicit:
  - **Release N scope: flowchart only.**
  - **Graph-family wide promotion is deferred.**
- [x] Why graph-family promotion is deferred:
  - Class diagram DAGRE SVG path still uses legacy direct render path in `ClassInstance` and does not mirror flowchart unified-preview call-site routing.
  - Class routed MMDS path currently uses engine-derived edge routing directly and does not apply `config.edge_routing` override semantics used by flowchart.
  - Flowchart has explicit unified-vs-legacy parity/rollback harnesses; class currently has snapshot compliance coverage but no equivalent edge-routing parity gate.
- [x] Backward-edge policy (current):
  - Keep backward-edge hint fallback as intentional release-N behavior.
- [x] Rollback policy (current):
  - Keep `--edge-routing full-compute` documented and supported for at least one release after default flip.

## Phase-0 Policy Specs (Pre-Implementation Contracts)

These specs are documented and test-scaffolded before runtime behavior changes.

### Fan-In Face-Anchor Overflow Spec (Task 0.2)

- Primary face capacity is direction-specific:
  - `TD/BT`: `4`
  - `LR/RL`: `2`
- Overflow activates when:
  - `incoming_degree > primary_face_capacity(direction)`
- Overflow spill slots alternate deterministic side lanes:
  - slot `0`: `LeftOrTop`
  - slot `1`: `RightOrBottom`
  - repeating thereafter
- Fixture-backed trigger contracts:
  - `stacked_fan_in.mmd` (`TD`, degree `2`) -> no overflow
  - `fan_in.mmd` (`TD`, degree `3`) -> no overflow
  - `five_fan_in.mmd` (`TD`, degree `5`) -> overflow
  - `fan_in_lr.mmd` (`LR`, degree `3`) -> overflow

### Fan-In + Backward-Channel Precedence Spec (Task 0.3)

- Canonical backward channel remains direction-fixed:
  - `TD/BT`: `Right`
  - `LR/RL`: `Bottom`
- On overflowed targets, incoming edges are deterministically ordered by edge index.
- Fan-in overflow candidates use deterministic alternating spill slots from Task 0.2.
- Precedence rule under contention:
  - Backward edges retain canonical backward channel.
  - Forward edges must not consume canonical backward channel on overflowed targets.
- Fixture-backed precedence contract:
  - `fan_in_backward_channel_conflict.mmd` keeps `Loop -> B` on the TD canonical right lane.
  - The same fixture enforces that exactly one inbound edge to `B` uses the right face.

### Fan-In + Backward-Channel Toggle Default Decision (Task 2.3)

- Fixture matrix status:
  - Pass (`SVG` + `Text`) for fan-in overflow toggle replay (`on`/`off`) across:
    - `stacked_fan_in.mmd`
    - `fan_in.mmd`
    - `five_fan_in.mmd`
    - `multiple_cycles.mmd`
    - `http_request.mmd`
    - `git_workflow.mmd`
- Decision:
  - Keep fan-in overflow/backward-channel behavior **always enabled** for unified preview.
- Evidence gates:
  - `fan_in_backward_channel_interaction_fixture_matrix_matches_documented_face_policies` (`tests/routed_geometry.rs`)
  - `svg_straight_fan_in_backward_channel_interaction_fixture_matrix_matches_documented_faces` (`tests/svg_render.rs`)
  - `fan_in_backward_channel_interaction_fixture_matrix_matches_documented_policy_in_text_and_svg` (`tests/integration.rs`)

### Non-ViewBox Metric Gate Spec (Task 0.4)

- Sweep baseline must include non-viewBox route/label signals:
  - `route_envelope_width_delta`
  - `route_envelope_height_delta`
  - `label_position_max_drift`
  - `label_position_mean_drift`
  - `edge_label_count_delta`
- Gate reproducibility is anchored in:
  - `scripts/tests/08-unified-vs-full-svg-diff-sweep.sh`
  - `docs/unified_feedback_baseline.tsv`
  - `tests/svg_snapshots.rs` baseline-schema tests
- Monitor thresholds (default, env-overridable):
  - `ROUTE_ENVELOPE_ABS_DELTA_WARN_PX=24`
  - `LABEL_POSITION_MAX_DRIFT_WARN_PX=40`
  - `LABEL_POSITION_MEAN_DRIFT_WARN_PX=20`

### Long-Skip Toggle Retirement (2026-02-17)

- `long_skip_periphery_detour` policy plumbing was removed from runtime, CLI, and tests.
- Unified preview now has a single canonical path for long-skip edges (no on/off matrix).
- Long-skip quality monitoring continues via the shared non-viewBox sweep metrics in:
  - `scripts/tests/08-unified-vs-full-svg-diff-sweep.sh`
  - `docs/unified_feedback_baseline.tsv`

## Accepted Deltas (Release N, Flowchart Scope)

This is the decision record for known output differences when unified routing is promoted.

| Area | Classification | Decision | Notes |
| ---- | -------------- | -------- | ----- |
| Flowchart SVG straight parity-classification subset (`simple.mmd`, `chain.mmd`, `simple_cycle.mmd`, `decision.mmd`, `fan_out.mmd`, `left_right.mmd`, `subgraph_direction_cross_boundary.mmd`, `multi_subgraph_direction_override.mmd`) | accepted-improvement | Accept for Release N | Unified-preview deltas are explicitly classified and test-enforced in `svg_unified_preview_parity_fixture_subset_matches_expected_classification`. |
| Flowchart text label-revalidation fixture parity (`labeled_edges.mmd`, `inline_label_flowchart.mmd`) | must-match | Must match | `text_label_revalidation_fixtures_match_between_unified_preview_and_full_compute_modes` enforces edge-routing parity for text output across label-revalidation fixtures. |
| Flowchart backward-edge routing behavior | accepted-design | Accept for Release N | Keep route-hint fallback as intentional behavior (stability-first) instead of forcing full backward-edge unification in this release. |
| Rollback parity guard (`--edge-routing full-compute`) for straight core subset (`simple.mmd`, `chain.mmd`, `simple_cycle.mmd`) | must-match-legacy | Must match | `svg_full_compute_override_matches_legacy_straight_core_subset` requires byte-identical legacy parity when rollback mode is selected. |

### Delta Evidence Sources

- `plans/archive/0075-orthogonal-routing-unification/findings/discovery-unified-preview-svg-linear-core-parity-delta-simple-cycle.md`
- `plans/archive/0075-orthogonal-routing-unification/findings/note-unified-preview-backward-edge-fallback-uses-existing-hints.md`
- `tests/svg_snapshots.rs`:
  - `svg_unified_preview_parity_fixture_subset_matches_expected_classification`
  - `unified_preview_svg_output_is_deterministic_for_fixture_subset`
  - `svg_full_compute_override_matches_legacy_straight_core_subset`
- `tests/mmds_json.rs`:
  - `unified_preview_mmds_routed_output_is_deterministic_for_fixture_subset`
  - `routed_mmds_defaults_to_full_path_detail`
- `tests/integration.rs`:
  - `text_label_revalidation_fixtures_match_between_unified_preview_and_full_compute_modes`
  - `text_renderer_rejects_stale_precomputed_label_anchor_for_label_revalidation_fixture`

## Follow-Up Planning Items (Required Before Graph-Family Promotion)

- [ ] Create follow-up implementation plan: **class edge-routing parity plumbing**
  - Make class DAGRE SVG path honor explicit edge-routing override semantics consistently.
  - Make class routed MMDS path honor explicit edge-routing override semantics consistently.
- [ ] Create follow-up implementation plan: **class parity/rollback harness**
  - Add class unified-vs-legacy edge-routing parity tests equivalent in intent to flowchart gates.
- [ ] Create follow-up implementation plan: **graph-family default promotion**
  - Promote unified routing default across class + flowchart only after the above two plans are complete.

## Hard Gates (Must Pass)

- [x] Run the full plan-0076 QA script:

```bash
./scripts/tests/07-plan-0076-unified-routing-quality-qa.sh
```

- [x] Expanded parity audit beyond core trio and classified fixture deltas:
  - `accepted-improvement`: `simple.mmd`, `chain.mmd`, `simple_cycle.mmd`, `decision.mmd`, `fan_out.mmd`, `left_right.mmd`, `subgraph_direction_cross_boundary.mmd`, `multi_subgraph_direction_override.mmd`
  - `must-match-legacy` rollback: full-compute override parity for `simple.mmd`, `chain.mmd`, `simple_cycle.mmd`

- [x] Determinism confirmed for unified mode (same input, same output bytes):
  - SVG (`--edge-routing unified-preview`)
  - MMDS routed (`--edge-routing unified-preview`)

- [x] Full regression gates pass:

```bash
just test-file integration
just test-file svg_render
just test-file svg_snapshots
just test-file routed_geometry
cargo test svg_snapshot_all_fixtures
cargo test svg_snapshot_orthogonal_fixture_subset
just lint
```

## Known Follow-Ups To Resolve Or Explicitly Accept

From archived plan 0075 findings:

- [x] `simple_cycle.mmd` straight-SVG delta in unified preview is accepted (or fixed):
  - `plans/archive/0075-orthogonal-routing-unification/findings/discovery-unified-preview-svg-linear-core-parity-delta-simple-cycle.md`
- [x] Backward-edge fallback is accepted (or replaced with full unified behavior):
  - `plans/archive/0075-orthogonal-routing-unification/findings/note-unified-preview-backward-edge-fallback-uses-existing-hints.md`
- [x] Alignment tolerance (`0.5`) is accepted and regression-tested:
  - `plans/archive/0075-orthogonal-routing-unification/findings/discovery-orthogonal-builder-alignment-tolerance.md`
  - Regression gate: `shared_builder_keeps_alignment_tolerance_stable_for_near_aligned_points` (`tests/routed_geometry.rs`)
- [x] Direction-policy split between layout-aware and shared policy helpers is accepted for Release N:
  - `plans/archive/0075-orthogonal-routing-unification/findings/discovery-shared-attachment-adapters-need-legacy-direction-and-spread-semantics.md`
  - Release N keeps the current split with explicit fixture-level parity + rollback gates.

### Final Fixture Classification (Task 5.1)

This final classification is the promotion-time reference for gated behavior in
Plan 0077.

| Policy Area | Fixture Subset | Classification | Gate |
| ---- | ---- | ---- | ---- |
| Fan-in overflow/backward-channel behavior | `stacked_fan_in.mmd`, `fan_in.mmd`, `five_fan_in.mmd`, `multiple_cycles.mmd`, `http_request.mmd`, `git_workflow.mmd`, `fan_in_backward_channel_conflict.mmd` | Must preserve documented face policies with deterministic overflow lanes | `fan_in_backward_channel_interaction_fixture_matrix_matches_documented_face_policies`, `svg_straight_fan_in_backward_channel_interaction_fixture_matrix_matches_documented_faces`, `fan_in_backward_channel_interaction_fixture_matrix_matches_documented_policy_in_text_and_svg` |
| Style-segment monitor-only (styled segment minimum) | `edge_styles.mmd`, `inline_edge_labels.mmd` | monitor-only; escalate on violations | `style_segment_monitor_reports_actionable_summary_for_routed_geometry`, `style_segment_monitor_reports_actionable_summary_for_svg` |
| Label-revalidation text parity | `labeled_edges.mmd`, `inline_label_flowchart.mmd` | `must-match` between `unified-preview` and `full-compute` text output | `text_label_revalidation_fixtures_match_between_unified_preview_and_full_compute_modes` |
| Rollback parity (legacy straight core) | `simple.mmd`, `chain.mmd`, `simple_cycle.mmd` | `must-match-legacy` under rollback mode | `svg_full_compute_override_matches_legacy_straight_core_subset` |

Operational context and escalation notes remain tracked in:
- `docs/UNIFIED_ISSUES.md`

### Rollback Playbook (Task 5.1)

Use this playbook if rollout metrics or fixture gates regress.

1. Run the QA gate script and capture artifacts:

```bash
./scripts/tests/07-plan-0076-unified-routing-quality-qa.sh
```

2. Force legacy routing behavior for immediate rollback validation:

```bash
mmdflux --edge-routing full-compute <input.mmd>
```

3. Re-run targeted gates and compare with baseline classifications:

  - `fan_in_backward_channel_interaction_fixture_matrix_matches_documented_face_policies`
  - `svg_straight_fan_in_backward_channel_interaction_fixture_matrix_matches_documented_faces`
  - `style_segment_monitor_reports_actionable_summary_for_svg`
  - `svg_full_compute_override_matches_legacy_straight_core_subset`

4. Keep style-segment checks monitor-only (no runtime policy toggle) until
   explicit enforcement is approved.

## Code Change Checklist For Default Flip

- [ ] Change default routing selection to unified mode for intended scope.
- [ ] Keep CLI override behavior intact:
  - `--edge-routing unified-preview`
  - `--edge-routing full-compute`
  - `--edge-routing pass-through-clip`
- [ ] Keep rollback tests green:
  - `svg_full_compute_override_matches_legacy_straight_core_subset`
- [ ] Update docs to reflect new default:
  - `docs/CLI_REFERENCE.md`
  - `docs/DEBUG.md`
  - `README.md`

## Release Notes Template

Copy into release notes/changelog:

```text
Routing default changed: unified routing is now the default for <scope>.

This may change edge path geometry in some diagrams (notably cycle/backward-edge cases).
Use --edge-routing full-compute to force legacy routing behavior during transition.

Known accepted deltas:
- <fixture/category>: <short reason>
- <fixture/category>: <short reason>
```

## Recommended One-Release Transition Policy

- Release N:
  - Unified routing default ON.
  - `full-compute` explicitly documented as rollback path.
- Release N+1:
  - Re-evaluate rollback path retention based on issue volume and fixture delta review.
