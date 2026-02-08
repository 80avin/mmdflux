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
