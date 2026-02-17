# Unified Routing Migration: Visual Comparison Report

**Date:** 2026-02-11
**Branch:** `refactor/unified-routing`
**Comparison:** `full-compute` (left/baseline) vs `unified-preview` (right/candidate)
**Gallery:** `scripts/tests/out/20260211-132916-unified-vs-full-sweep/routing-svg-diff-gallery-v2.html`
**Data:** `docs/unified_analysis_raw.json`, `docs/unified_deep_analysis.json`
**Task 0.1 Baseline:** `docs/unified_feedback_baseline.tsv`

## Task 0.1 Baseline Dataset (2026-02-12)

Locked fixture/metric baseline for the first remediation phase is stored in
`docs/unified_feedback_baseline.tsv`.

Current schema:
- `fixture`
- `style`
- `status`
- `diff_lines`
- `full_viewbox_width`
- `full_viewbox_height`
- `unified_viewbox_width`
- `unified_viewbox_height`
- `viewbox_width_delta`
- `viewbox_height_delta`
- `full_route_envelope_width`
- `full_route_envelope_height`
- `unified_route_envelope_width`
- `unified_route_envelope_height`
- `route_envelope_width_delta`
- `route_envelope_height_delta`
- `full_edge_label_count`
- `unified_edge_label_count`
- `edge_label_count_delta`
- `label_position_max_drift`
- `label_position_mean_drift`

Required fixture coverage in this baseline:
- `fan_in.mmd`
- `five_fan_in.mmd`
- `stacked_fan_in.mmd`
- `fan_in_lr.mmd`
- `labeled_edges.mmd`
- `inline_label_flowchart.mmd`
- `double_skip.mmd`
- `skip_edge_collision.mmd`

## Task 0.4 Non-ViewBox Metric Gate Spec (2026-02-12)

Non-viewBox gate now tracks path-level signals in the sweep baseline:

- Route-envelope deltas:
  - `route_envelope_width_delta`
  - `route_envelope_height_delta`
- Label drift stats:
  - `label_position_max_drift`
  - `label_position_mean_drift`
  - plus `edge_label_count_delta` for label cardinality shifts

Reproducible monitor thresholds (script-configurable):

- `ROUTE_ENVELOPE_ABS_DELTA_WARN_PX` (default `24`)
- `LABEL_POSITION_MAX_DRIFT_WARN_PX` (default `40`)
- `LABEL_POSITION_MEAN_DRIFT_WARN_PX` (default `20`)

Sweep script reports these in a `non-viewBox metric summary` section:

```bash
./scripts/tests/08-unified-vs-full-svg-diff-sweep.sh
```

## Task 1.3 Text Label Parity Guard (2026-02-12)

Label-anchor revalidation delivery now includes an explicit text-render guard:

- Precomputed text label anchors are accepted only when the anchor remains
  within `2` cells of the active routed path.
- If a precomputed anchor is stale, text rendering falls back to a
  route-midpoint anchor (with existing collision-avoidance safety).
- Routing-mode parity for text is now fixture-gated for label-anchor defects:
  - `labeled_edges.mmd`
  - `inline_label_flowchart.mmd`

Renderer semantics note:
- SVG/routed-geometry revalidation occurs in float-path space.
- Text revalidation occurs in canvas-cell space at label placement time.
- The guard intent is the same in both renderers: stale anchors must not pull
  labels away from the active routed segment.

## Task 4.1 Style Segment Monitor-Only Checks (2026-02-12)

Style-segment checks remain monitor-only in this plan tranche. No runtime stretching/enforcement
is applied yet.

Active monitor checks:

- Routed geometry monitor:
  - `style_segment_monitor_reports_actionable_summary_for_routed_geometry`
- SVG monitor:
  - `style_segment_monitor_reports_actionable_summary_for_svg`

Current monitor threshold:

- Styled-path minimum segment length: `12px`

Escalation criteria:

- Any monitor violation in CI triggers issue/finding capture (do not silently
  promote).
- If violations persist across repeated sweeps/CI runs, escalate style-segment checks from
  monitor-only to an enforcement task in a follow-up phase.
- Until escalation is approved, keep style-segment checks monitor-only (no
  runtime enforcement toggle) and use monitor output as release-gating telemetry
  only.

## Task 5.1 Promotion + Rollback Finalization (2026-02-12)

Final promotion-time fixture classifications and rollback commands are
consolidated in:

- `docs/UNIFIED_ROUTING_PROMOTION.md`
  - `Final Fixture Classification (Task 5.1)`
  - `Rollback Playbook (Task 5.1)`

Policy posture carried into release gating:

- `long_skip_periphery_detour` default-off; enable selectively with `--policy-long-skip-periphery-detour on`.
- style-segment checks monitor-only; keep escalations tied to monitor
  violations.

## Task 0.2 Fan-In Overflow Policy Spec (2026-02-12)

Fan-in overflow policy is now explicitly fixture-backed and documented as a Phase-0
spec (no runtime behavior change in this task):

- Primary face capacity:
  - `TD/BT`: `4`
  - `LR/RL`: `2`
- Overflow trigger:
  - `incoming_degree > primary_face_capacity(direction)`
- Overflow distribution order:
  - Deterministic alternating side lanes:
    - slot `0`: `LeftOrTop`
    - slot `1`: `RightOrBottom`
    - slot `2`: `LeftOrTop`
    - ...

Fixture-backed trigger expectations:

| Fixture | Direction | Incoming degree | Expected overflow |
| ------- | --------- | --------------- | ----------------- |
| `stacked_fan_in.mmd` | TD | 2 | No |
| `fan_in.mmd` | TD | 3 | No |
| `five_fan_in.mmd` | TD | 5 | Yes |
| `fan_in_lr.mmd` | LR | 3 | Yes |

## Executive Summary

88 SVG basis fixtures compared (72 flowchart + 16 class). 87 have diffs, 1 identical.

| Score | Count | % |
|-------|-------|---|
| Good | 61 | 69% |
| Acceptable | 17 | 19% |
| Needs Work | 11 | 12% |

**Key findings:**
- Path simplification (cubic bezier → straight lines for straight segments) is universal and benign
- 11 fixtures have significant visual issues, all traceable to 3 root causes
- No dimension mismatches, no edge count mismatches
- Self-loops, LR/RL/BT directions, class diagrams all look good
- The main problems are diamond clipping and two subgraph edge bugs

---

## 2026-02-12 Manual Triage Backlog (Current Working Set)

The list below reflects the latest side-by-side review and should be treated as the active backlog for promotion readiness.

### Grouped by Defect vs Enhancement

| Category | Type | Priority | Fixtures / Edges | Judgment |
|----------|------|----------|------------------|----------|
| Backward-edge marker visibility + approach-angle degradation in non-orth styles | Defect | P0 | `decision.mmd` (`Debug -> Start`), `labeled_edges.mmd` (`Handle Error -> Setup`), `http_request.mmd` (`Send Response -> Client`), `complex.mmd` (`More Data? -> Input`), `git_workflow.mmd` backward edges | Needs Work |
| Hidden arrowheads when crossing container/subgraph boundaries | Defect | P0 | `subgraph_direction_nested_both.mmd` (`C -> A`, orthogonal/linear/rounded/basis), `animal_hierarchy.mmd` inheritance heads | Needs Work |
| Ambiguous orthogonal attachment points on routed edges | Defect | P1 | `inline_label_flowchart.mmd`: `Ingest Request -> Audit Log`, `Serve Cached -> Valid?`, `Audit Log -> Emit Metrics`, `Persist Result -> Emit Metrics` | Needs Work |
| Fan-in terminal overlap / arrowhead occlusion under convergence pressure | Defect | P1 | `fan_in_lr.mmd` (basis/linear/rounded) and related fan-in variants | Needs Work |
| Orthogonal over-jogging / circuitous path selection where straight route is obvious | Enhancement | P2 | `nested_subgraph_edges.mmd`, additional orthogonal cases with extra jogs | Improvement Candidate |

### Resolved During Current Iteration

- `multiple_cycles.mmd` orthogonal: removed tiny backward-edge terminal staircase jogs by making rerouted endpoint-validation backward-edge aware before endpoint reclip decisions.
- `nested_subgraph_edge.mmd` orthogonal: removed large lateral detours on subgraph-as-node edges by preserving `from_subgraph` / `to_subgraph` metadata in `from_dagre_layout` (unified routing now keeps container endpoint intent instead of falling back to child-node targeting).
- Non-orth tiny terminal hooks before arrowheads (linear/rounded/basis): added pre-marker terminal-elbow compaction for short start/end elbows; example `ampersand.mmd` merge-in edges now avoid cramped pre-arrow right-angle turns.
- Non-orth backward-edge terminal direction inversion: disabled forward-primary-axis tail enforcement for backward edges in SVG post-processing so final tangents point toward targets (`decision.mmd`, `http_request.mmd`, `labeled_edges.mmd`, `git_workflow.mmd`, and related backward-edge fixtures).

---

## Issue Categories

### Category A: Diamond Node Exit Point Clipping (MAJOR)

**Root cause:** Unified routing clips edge endpoints to the diamond's bounding rectangle instead of its actual diagonal boundary. Full-compute correctly ray-casts to the diamond edge.

**Visual impact:** Edges appear to start/end 12-57px away from the diamond shape, floating in mid-air or connecting to the wrong spot on the bounding box.

**Affected fixtures (9):**
- `flowchart_decision` (38px) — Yes/No branches start at diamond bottom y instead of diagonals
- `flowchart_http_request` (57px) — worst case; Yes/No branches 57px off from diamond lower diagonals
- `flowchart_complex` (26px) — "invalid" edge from Validate diamond, backward edge from More Data? diamond
- `flowchart_inline_label_flowchart` (48px) — multiple diamond exit edges affected
- `flowchart_labeled_edges` (32px) — diamond exit with label
- `flowchart_compat_directive` (33px) — diamond in compat flowchart
- `flowchart_compat_class_annotation` (26px) — diamond in compat flowchart
- `flowchart_compat_kitchen_sink` (39px) — diamond in complex compat flowchart
- `flowchart_ci_pipeline` (20px) — non-diamond but non-rect shape clipping (rounded rect?)

**Example — `decision.mmd` edge "No" (diamond → Debug):**
```
Full:    start=(122.89, 245.15)  — on diamond right diagonal ✓
Unified: start=(143.45, 277.36)  — at diamond bounding-box bottom edge ✗
Shift: 38px
```

**Fix priority:** HIGH — affects all diagrams with diamond nodes

### Category B: Subgraph Edge Vertical Shift (NEW BUG)

**Root cause:** Unified routing appears to use incorrect vertical offsets for edges within or between subgraphs, resulting in edges that are shifted up or collapsed to tiny segments.

**Visual impact:** Edges completely miss their target nodes or collapse into near-invisible stubs.

**Affected fixtures (2):**

**B1: `subgraph_to_subgraph_edge`** — Edge shifted 30px upward
- API Server → Database edge: full-compute starts at y=375 (node bottom), unified at y=345 (inside node)
- Edge ends at y=391 (unified) instead of y=421 (full), missing Database node by 34px
- This is a NEW issue — full-compute has no problems here

**B2: `subgraph_direction_nested_both`** — Edge collapsed to 8px stub
- A → B edge in inner BT subgraph: full-compute spans y=230→184 (46px), unified spans y=176→184 (8px)
- The 46px edge between two nodes has been collapsed to a tiny stub near the target node
- Endpoint diff: 54px

**Fix priority:** CRITICAL — these produce visually broken output

### Category C: Minor Endpoint Shifts (ACCEPTABLE)

**Root cause:** Small differences in how unified routing computes anchor points, likely from rounding or different interpolation of port positions on node boundaries.

**Visual impact:** 3-8px shifts that are generally not visible to the naked eye, especially when arrowhead markers provide 5px of visual slack.

**Affected fixtures (17):**
- `flowchart_direction_override` (8.0px)
- `flowchart_subgraph_direction_cross_boundary` (8.0px)
- `flowchart_subgraph_direction_lr` (8.0px)
- `flowchart_subgraph_direction_mixed` (8.0px)
- `flowchart_subgraph_direction_nested` (8.0px)
- `flowchart_five_fan_in` (5.0px)
- `flowchart_fan_in` (4.8px)
- `flowchart_very_narrow_fan_in` (4.8px)
- `flowchart_multi_subgraph_direction_override` (4.7px)
- `flowchart_narrow_fan_in` (4.4px)
- `flowchart_ampersand` (4.0px)
- `class_animal_hierarchy` (4.1px)
- `flowchart_fan_in_lr` (3.6px)
- `flowchart_diamond_fan` (3.6px)
- `class_inheritance_chain` (3.6px)
- `flowchart_double_skip` (3.2px)
- `class_interface_realization` (3.1px)

**Fix priority:** LOW — visually indistinguishable in most cases

### Category D: Path Simplification (BENIGN)

**Root cause:** Unified routing emits direct line segments (`M x1,y1 L x2,y2`) for straight connections instead of cubic bezier curves (`M x1,y1 C ... L x2,y2`) that describe the same straight path.

**Visual impact:** None. The paths render identically. This is actually an improvement — cleaner SVG output.

**Affected fixtures:** 83 of 88 (all except 2 identical and 3 self-loop variations)

**Fix priority:** N/A — this is a positive change

---

## Fixture-by-Fixture Detail

### Flowchart Fixtures (72)

| Fixture | Score | Max Diff | Notes |
|---------|-------|----------|-------|
| `ampersand.mmd` | Acceptable | 4.0px | Minor fan-out endpoint shifts |
| `backward_in_subgraph.mmd` | Good | 0px | Path simplification; backward edge shapes differ slightly but endpoints match |
| `bidirectional.mmd` | Good | 0px | Path simplification only |
| `bidirectional_arrows.mmd` | Good | 0px | Path simplification only |
| `bottom_top.mmd` | Good | 0px | BT direction works perfectly; straight-line simplification |
| `br_line_breaks.mmd` | Good | 0px | Path simplification only |
| `chain.mmd` | Good | 0px | Path simplification only |
| `ci_pipeline.mmd` | **Needs Work** | 20px | LR edges from rounded-rect nodes shifted ~20px at start |
| `compat_class_annotation.mmd` | **Needs Work** | 26px | Diamond exit edges clipped to bounding box |
| `compat_directive.mmd` | **Needs Work** | 33px | Diamond exit edges clipped to bounding box |
| `compat_frontmatter.mmd` | Good | 0px | Path simplification only |
| `compat_hyphenated_ids.mmd` | Good | 0px | Path simplification only |
| `compat_invisible_edge.mmd` | Good | 2.3px | Tiny shift, not significant |
| `compat_kitchen_sink.mmd` | **Needs Work** | 39px | Diamond exit edges clipped to bounding box |
| `compat_no_direction.mmd` | Good | 0px | Path simplification only |
| `compat_numeric_ids.mmd` | Good | 0px | Path simplification only |
| `complex.mmd` | **Needs Work** | 26px | Diamond clipping on Validate and More Data? nodes; backward edge start shifted |
| `cross_circle_arrows.mmd` | Good | 0px | Path simplification only |
| `decision.mmd` | **Needs Work** | 38px | Classic diamond clipping issue; Yes/No branches displaced |
| `diamond_fan.mmd` | Acceptable | 3.6px | Minor shifts on diamond fan edges |
| `direction_override.mmd` | Acceptable | 8px | Minor shifts in direction-overridden subgraphs |
| `double_skip.mmd` | Acceptable | 3.2px | Minor shifts on long skip edges |
| `edge_styles.mmd` | Good | 0px | Dotted/thick styles preserved; path simplification only |
| `external_node_subgraph.mmd` | Good | 0px | Path simplification only |
| `fan_in.mmd` | Acceptable | 4.8px | Minor shifts on converging edges |
| `fan_in_lr.mmd` | Acceptable | 3.6px | Minor shifts on LR fan-in |
| `fan_out.mmd` | Good | 0px | Path simplification only |
| `five_fan_in.mmd` | Acceptable | 5.0px | Minor shifts on 5-way fan-in |
| `git_workflow.mmd` | Good | 1.2px | Negligible shift |
| `http_request.mmd` | **Needs Work** | 57px | Worst diamond clipping case; Yes/No branches 57px from diamond boundary |
| `inline_edge_labels.mmd` | Good | 0px | Path simplification only; edge labels positioned correctly |
| `inline_label_flowchart.mmd` | **Needs Work** | 48px | Multiple diamond exits; significant clipping displacement |
| `label_spacing.mmd` | Good | 0px | Path simplification only |
| `labeled_edges.mmd` | **Needs Work** | 32px | Diamond exit with labeled edge |
| `left_right.mmd` | Good | 0px | LR direction perfect; straight-line simplification |
| `multi_edge.mmd` | Good | 0px | Path simplification only |
| `multi_edge_labeled.mmd` | Good | 1.9px | Negligible shift |
| `multi_subgraph.mmd` | Good | 0px | Path simplification only |
| `multi_subgraph_direction_override.mmd` | Acceptable | 4.7px | Minor shift in overridden subgraph |
| `multiple_cycles.mmd` | Good | 2.8px | Path simplification; backward edge shapes slightly different |
| `narrow_fan_in.mmd` | Acceptable | 4.4px | Minor shift on narrow fan-in |
| `nested_subgraph.mmd` | Good | 0px | Path simplification only |
| `nested_subgraph_edge.mmd` | Good | 0px | **Identical** — zero diff |
| `nested_subgraph_only.mmd` | Good | 0px | Path simplification only |
| `nested_with_siblings.mmd` | Good | 0px | Path simplification only |
| `right_left.mmd` | Good | 0px | RL direction works; path simplification only |
| `self_loop.mmd` | Good | 0px | **Identical** — self-loop shape and endpoints match |
| `self_loop_labeled.mmd` | Good | 0px | Path simplification; self-loop on diamond correctly touches boundary |
| `self_loop_with_others.mmd` | Good | 0px | Path simplification only |
| `shapes.mmd` | Good | 0px | Path simplification only |
| `shapes_basic.mmd` | Good | 0px | All shape types (rect, rounded, stadium, subroutine, cylinder, diamond, hexagon) — edges connect correctly |
| `shapes_degenerate.mmd` | Good | 0px | Path simplification only |
| `shapes_document.mmd` | Good | 0px | Path simplification only |
| `shapes_junction.mmd` | Good | 0px | Junction nodes connected correctly |
| `shapes_special.mmd` | Good | 0px | Path simplification only |
| `simple.mmd` | Good | 0px | Path simplification only (straight vertical line) |
| `simple_cycle.mmd` | Good | 2.9px | Minor shift on backward edge |
| `simple_subgraph.mmd` | Good | 0px | Path simplification only |
| `skip_edge_collision.mmd` | Good | 2.8px | Minor shift on skip edges |
| `stacked_fan_in.mmd` | Good | 2.5px | Minor shift on stacked fan edges |
| `subgraph_as_node_edge.mmd` | Good | 0px | Path simplification only |
| `subgraph_direction_cross_boundary.mmd` | Acceptable | 8px | Minor shift on cross-boundary edges |
| `subgraph_direction_lr.mmd` | Acceptable | 8px | Minor shift in LR subgraph |
| `subgraph_direction_mixed.mmd` | Acceptable | 8px | Minor shift in mixed-direction subgraphs |
| `subgraph_direction_nested.mmd` | Acceptable | 8px | Minor shift in nested direction subgraphs |
| `subgraph_direction_nested_both.mmd` | **Needs Work** | 54px | A→B edge collapsed to 8px stub (should span 46px) |
| `subgraph_edges.mmd` | Good | 0px | Path simplification only |
| `subgraph_edges_bottom_top.mmd` | Good | 0px | BT subgraph edges work correctly |
| `subgraph_multi_word_title.mmd` | Good | 0px | Path simplification only |
| `subgraph_numeric_id.mmd` | Good | 0px | Path simplification only |
| `subgraph_to_subgraph_edge.mmd` | **Needs Work** | 30px | Edge shifted 30px upward; starts inside node, ends in mid-air (NEW regression) |
| `very_narrow_fan_in.mmd` | Acceptable | 4.8px | Minor shift on very narrow fan edges |

### Class Diagram Fixtures (16)

| Fixture | Score | Max Diff | Notes |
|---------|-------|----------|-------|
| `all_relations.mmd` | Good | 0px | Path simplification only |
| `animal_hierarchy.mmd` | Acceptable | 4.1px | Minor shifts |
| `cardinality_labels.mmd` | Good | 0px | Cardinality labels positioned correctly |
| `class_labels.mmd` | Good | 0px | Path simplification only |
| `direction_bt.mmd` | Good | 0px | BT class diagrams work |
| `direction_lr.mmd` | Good | 0px | LR class diagrams work |
| `direction_rl.mmd` | Good | 0px | RL class diagrams work |
| `direction_tb.mmd` | Good | 0px | TB class diagrams work |
| `inheritance_chain.mmd` | Acceptable | 3.6px | Minor shift on chain edges |
| `interface_realization.mmd` | Acceptable | 3.1px | Minor shift on interface edges |
| `lollipop_interfaces.mmd` | Good | 0px | Path simplification only |
| `members.mmd` | Good | 0px | Path simplification only |
| `namespaces.mmd` | Good | 0px | Path simplification only |
| `relationships.mmd` | Good | 0px | Path simplification only |
| `simple.mmd` | Good | 0px | Path simplification only |
| `two_way_relations.mmd` | Good | 0px | Path simplification only |
| `user_lollipop_repro.mmd` | Good | 0px | Path simplification only |

---

## Prioritized Fix List

### P0 — Must Fix Before Migration

1. **Diamond boundary clipping** (Category A)
   - Unified routing must ray-cast edge endpoints to the diamond's actual diagonal boundary, not its bounding rectangle
   - Affects 9 fixtures, up to 57px displacement
   - Likely a single code change in the endpoint clipping logic

2. **Subgraph edge vertical shift** (Category B — `subgraph_to_subgraph_edge`)
   - Edge between nodes within same subgraph shifted 30px upward
   - NEW regression not present in full-compute
   - Investigate if border node offsets are applied incorrectly

3. **Inner BT subgraph edge collapse** (Category B — `subgraph_direction_nested_both`)
   - A→B edge in inner BT-direction subgraph collapsed to 8px stub
   - 54px displacement; edge effectively disappears
   - May be related to direction override + border node interaction

### P1 — Nice to Fix

4. **Rounded rect clipping** (`ci_pipeline`)
   - 20px shift suggests non-rect clipping may need refinement for rounded rects in LR layouts
   - Lower impact than diamond since the visual gap is smaller

5. **Subgraph direction endpoint precision** (8px shifts across 5 direction override fixtures)
   - Consistent 8px offset in direction-overridden subgraph edges
   - Visible but not visually broken

### P2 — Accept As-Is

6. **Minor fan-in/fan-out shifts** (3-5px across ~12 fixtures)
   - Within arrowhead marker tolerance
   - Not visually distinguishable

---

## What Looks Good

- **Path simplification** — cleaner SVG output, no visual change
- **Self-loops** — shape and endpoints match perfectly
- **LR/RL/BT directions** — all work correctly for simple graphs
- **Edge styles** — dotted, thick, all preserved
- **Edge labels** — positions match
- **Class diagrams** — all 16 fixtures Good or Acceptable
- **All node shapes** — rect, rounded, stadium, subroutine, cylinder, hexagon render correctly (only diamond *clipping* is affected, not diamond *rendering*)
- **Junction nodes** — connected correctly
- **Backward edges** — shape slightly different but endpoints generally match
- **Bidirectional edges** — work correctly
- **Multiple edges between same nodes** — work correctly
