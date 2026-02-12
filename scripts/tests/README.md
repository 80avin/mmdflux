# Manual Epic Verification Scripts (Research 0041)

These scripts provide manual, visual checks for the MMDS epic work (plans 0061-0067, plus carry-forward behavior).

## Usage

Run an individual script:

```bash
./scripts/tests/01-mmds-ingest-validation.sh
```

Run all scripts:

```bash
./scripts/tests/run-all.sh
```

Outputs are written to:

- `scripts/tests/out/<timestamp>/...`

You can force a specific output directory timestamp:

```bash
RUN_ID=local-check-1 ./scripts/tests/run-all.sh
```

## Scripts

- `01-mmds-ingest-validation.sh`
  - Mermaid -> MMDS
  - MMDS -> text (valid payload)
  - expected validation failures
- `02-subgraph-endpoint-intent.sh`
  - endpoint-intent present vs missing SVG render comparison
- `03-positioned-capability-matrix.sh`
  - layout vs routed behavior matrix
  - expected routed text/ascii rejection guidance
- `04-extensions-profiles-compat.sh`
  - unknown profile/extension tolerance
  - unknown core/malformed extension rejection
- `05-roundtrip-and-conformance.sh`
  - direct vs MMDS roundtrip render inspection
  - `just conformance` summary + targeted MMDS suites
- `06-plan-0075-routing-preview-qa.sh`
  - focused QA sweep for orthogonal routing unification preview
  - parity/rollback gates, MMDS mode diffs, determinism checks, full regression gates
- `07-plan-0076-unified-routing-quality-qa.sh`
  - plan-level QA sweep for unified routing promotion hardening
  - expanded parity classification, determinism, routed path-detail defaults, and validation matrix
- `08-unified-vs-full-svg-diff-sweep.sh`
  - reusable full-compute vs unified-preview SVG sweep
  - renders fixture/style matrices, writes per-style TSV reports, and generates `routing-svg-diff-gallery-v2.html`
