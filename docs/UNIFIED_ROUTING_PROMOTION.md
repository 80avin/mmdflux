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
  - Keep `--routing-mode full-compute` available for at least one release.

## Current Decision (2026-02-11)

- [x] Promotion scope is explicit:
  - **Release N scope: flowchart only.**
  - **Graph-family wide promotion is deferred.**
- [x] Why graph-family promotion is deferred:
  - Class diagram DAGRE SVG path still uses legacy direct render path in `ClassInstance` and does not mirror flowchart unified-preview call-site routing.
  - Class routed MMDS path currently uses engine-derived routing mode directly and does not apply `config.routing_mode` override semantics used by flowchart.
  - Flowchart has explicit unified-vs-legacy parity/rollback harnesses; class currently has snapshot compliance coverage but no equivalent routing-mode parity gate.
- [x] Backward-edge policy (current):
  - Keep backward-edge hint fallback as intentional release-N behavior.
- [x] Rollback policy (current):
  - Keep `--routing-mode full-compute` documented and supported for at least one release after default flip.

## Phase-0 Policy Specs (Pre-Implementation Contracts)

These specs are documented and test-scaffolded before runtime behavior changes.

### Q1 Face-Anchor Overflow Spec (Task 0.2)

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

### Q1+Q2 Precedence Spec (Task 0.3)

- Canonical backward channel remains direction-fixed:
  - `TD/BT`: `Right`
  - `LR/RL`: `Bottom`
- On overflowed targets, incoming edges are deterministically ordered by edge index.
- Q1 overflow candidates use deterministic alternating spill slots from Task 0.2.
- Precedence rule under contention:
  - Backward edges retain canonical Q2 channel.
  - Forward edges must not consume canonical backward channel on overflowed targets.
- Fixture-backed precedence contract:
  - `q1_q2_conflict.mmd` keeps `Q2 -> B` on the TD canonical right lane.
  - The same fixture enforces that exactly one inbound edge to `B` uses the right face.

### Q6 Non-ViewBox Metric Gate Spec (Task 0.4)

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

## Accepted Deltas (Release N, Flowchart Scope)

This is the decision record for known output differences when unified routing is promoted.

| Area | Classification | Decision | Notes |
| ---- | -------------- | -------- | ----- |
| Flowchart SVG linear parity-classification subset (`simple.mmd`, `chain.mmd`, `simple_cycle.mmd`, `decision.mmd`, `fan_out.mmd`, `left_right.mmd`, `subgraph_direction_cross_boundary.mmd`, `multi_subgraph_direction_override.mmd`) | accepted-improvement | Accept for Release N | Unified-preview deltas are explicitly classified and test-enforced in `svg_unified_preview_parity_fixture_subset_matches_expected_classification`. |
| Flowchart text Q3 fixture parity (`labeled_edges.mmd`, `inline_label_flowchart.mmd`) | must-match | Must match | `text_q3_fixtures_match_between_unified_preview_and_full_compute_modes` enforces routing-mode parity for text output across Q3 fixtures. |
| Flowchart backward-edge routing behavior | accepted-design | Accept for Release N | Keep route-hint fallback as intentional behavior (stability-first) instead of forcing full backward-edge unification in this release. |
| Rollback parity guard (`--routing-mode full-compute`) for linear core subset (`simple.mmd`, `chain.mmd`, `simple_cycle.mmd`) | must-match-legacy | Must match | `svg_full_compute_override_matches_legacy_linear_core_subset` requires byte-identical legacy parity when rollback mode is selected. |

### Delta Evidence Sources

- `plans/archive/0075-orthogonal-routing-unification/findings/discovery-unified-preview-svg-linear-core-parity-delta-simple-cycle.md`
- `plans/archive/0075-orthogonal-routing-unification/findings/note-unified-preview-backward-edge-fallback-uses-existing-hints.md`
- `tests/svg_snapshots.rs`:
  - `svg_unified_preview_parity_fixture_subset_matches_expected_classification`
  - `unified_preview_svg_output_is_deterministic_for_fixture_subset`
  - `svg_full_compute_override_matches_legacy_linear_core_subset`
- `tests/mmds_json.rs`:
  - `unified_preview_mmds_routed_output_is_deterministic_for_fixture_subset`
  - `routed_mmds_defaults_to_full_path_detail`
- `tests/integration.rs`:
  - `text_q3_fixtures_match_between_unified_preview_and_full_compute_modes`
  - `text_renderer_rejects_stale_precomputed_label_anchor_for_q3_fixture`

## Follow-Up Planning Items (Required Before Graph-Family Promotion)

- [ ] Create follow-up implementation plan: **class routing-mode parity plumbing**
  - Make class DAGRE SVG path honor explicit routing-mode override semantics consistently.
  - Make class routed MMDS path honor explicit routing-mode override semantics consistently.
- [ ] Create follow-up implementation plan: **class parity/rollback harness**
  - Add class unified-vs-legacy routing-mode parity tests equivalent in intent to flowchart gates.
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
  - SVG (`--routing-mode unified-preview`)
  - MMDS routed (`--routing-mode unified-preview`)

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

- [x] `simple_cycle.mmd` linear-SVG delta in unified preview is accepted (or fixed):
  - `plans/archive/0075-orthogonal-routing-unification/findings/discovery-unified-preview-svg-linear-core-parity-delta-simple-cycle.md`
- [x] Backward-edge fallback is accepted (or replaced with full unified behavior):
  - `plans/archive/0075-orthogonal-routing-unification/findings/note-unified-preview-backward-edge-fallback-uses-existing-hints.md`
- [x] Alignment tolerance (`0.5`) is accepted and regression-tested:
  - `plans/archive/0075-orthogonal-routing-unification/findings/discovery-orthogonal-builder-alignment-tolerance.md`
  - Regression gate: `shared_builder_keeps_alignment_tolerance_stable_for_near_aligned_points` (`tests/routed_geometry.rs`)
- [x] Direction-policy split between layout-aware and shared policy helpers is accepted for Release N:
  - `plans/archive/0075-orthogonal-routing-unification/findings/discovery-shared-attachment-adapters-need-legacy-direction-and-spread-semantics.md`
  - Release N keeps the current split with explicit fixture-level parity + rollback gates.

## Code Change Checklist For Default Flip

- [ ] Change default routing selection to unified mode for intended scope.
- [ ] Keep CLI override behavior intact:
  - `--routing-mode unified-preview`
  - `--routing-mode full-compute`
  - `--routing-mode pass-through-clip`
- [ ] Keep rollback tests green:
  - `svg_full_compute_override_matches_legacy_linear_core_subset`
- [ ] Update docs to reflect new default:
  - `docs/CLI_REFERENCE.md`
  - `docs/DEBUG.md`
  - `README.md`

## Release Notes Template

Copy into release notes/changelog:

```text
Routing default changed: unified routing is now the default for <scope>.

This may change edge path geometry in some diagrams (notably cycle/backward-edge cases).
Use --routing-mode full-compute to force legacy routing behavior during transition.

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
