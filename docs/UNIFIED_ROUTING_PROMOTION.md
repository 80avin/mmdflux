# Unified Routing Promotion Checklist

Use this checklist when promoting unified routing from preview to the default behavior.

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

## Accepted Deltas (Release N, Flowchart Scope)

This is the decision record for known output differences when unified routing is promoted.

| Area | Classification | Decision | Notes |
| ---- | -------------- | -------- | ----- |
| Flowchart SVG linear, `simple_cycle.mmd` | accepted-neutral | Accept for Release N | Unified preview introduces forward-edge orthogonalization differences; covered by parity gate in `tests/svg_snapshots.rs`. |
| Flowchart backward-edge routing behavior | accepted-design | Accept for Release N | Keep route-hint fallback as intentional behavior (stability-first) instead of forcing full backward-edge unification in this release. |
| Flowchart SVG linear core subset (`simple.mmd`, `chain.mmd`) | must-match-legacy | Must match | Current parity gates require byte-identical outputs for these fixtures under rollback/full-compute expectations. |

### Delta Evidence Sources

- `plans/archive/0075-orthogonal-routing-unification/findings/discovery-unified-preview-svg-linear-core-parity-delta-simple-cycle.md`
- `plans/archive/0075-orthogonal-routing-unification/findings/note-unified-preview-backward-edge-fallback-uses-existing-hints.md`
- `tests/svg_snapshots.rs`:
  - `svg_unified_preview_parity_core_fixture_subset_has_expected_deltas`
  - `svg_full_compute_override_matches_legacy_linear_core_subset`

## Follow-Up Planning Items (Required Before Graph-Family Promotion)

- [ ] Create follow-up implementation plan: **class routing-mode parity plumbing**
  - Make class DAGRE SVG path honor explicit routing-mode override semantics consistently.
  - Make class routed MMDS path honor explicit routing-mode override semantics consistently.
- [ ] Create follow-up implementation plan: **class parity/rollback harness**
  - Add class unified-vs-legacy routing-mode parity tests equivalent in intent to flowchart gates.
- [ ] Create follow-up implementation plan: **graph-family default promotion**
  - Promote unified routing default across class + flowchart only after the above two plans are complete.

## Hard Gates (Must Pass)

- [ ] Run the full plan-0075 QA script:

```bash
./scripts/tests/06-plan-0075-routing-preview-qa.sh
```

- [ ] Expand parity audit beyond core trio (`simple`, `chain`, `simple_cycle`) and classify each diff:
  - `accepted-improvement`
  - `accepted-neutral`
  - `must-match-legacy`

- [ ] Determinism confirmed for unified mode (same input, same output bytes):
  - SVG (`--routing-mode unified-preview`)
  - MMDS routed (`--routing-mode unified-preview`)

- [ ] Full regression gates pass:

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
- [ ] Alignment tolerance (`0.5`) is accepted and regression-tested (or revised):
  - `plans/archive/0075-orthogonal-routing-unification/findings/discovery-orthogonal-builder-alignment-tolerance.md`
- [ ] Direction-policy split between layout-aware and shared policy helpers is accepted (or unified):
  - `plans/archive/0075-orthogonal-routing-unification/findings/discovery-shared-attachment-adapters-need-legacy-direction-and-spread-semantics.md`

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
