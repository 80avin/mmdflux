# mmdflux Web Playground

Static Vite + TypeScript playground for `mmdflux-wasm`.

## Commands

```bash
npm install
npm run test
npm run build
npm run dev
npm run benchmark:smoke
```

`npm run dev`, `npm run build`, and `npm run test` call `wasm-pack` to refresh `src/wasm-pkg` from `../crates/mmdflux-wasm`.

`npm run benchmark:smoke` also refreshes `src/wasm-pkg`, then runs `scripts/benchmark-smoke.ts` to execute reduced benchmark scenarios against mmdflux and mermaid with conservative CI thresholds.

## Deploy Runbook

- Workflow: `.github/workflows/playground-deploy.yml`
- Trigger options:
  - Tag push matching `v*` (for example `v0.9.0`)
  - Manual `workflow_dispatch`
- Artifact path: `web/dist/`
- Pages base path:
  - CI sets `VITE_BASE_PATH=/<repo-name>/`
  - Local builds default to `/` unless `VITE_BASE_PATH` is explicitly set

Operator sequence:

1. Ensure `web` tests/build and `just wasm-build` are green locally.
2. Push a `v*` tag (or run manual dispatch) to start the deploy workflow.
3. Confirm the `Build Playground Artifact` job uploads `web/dist/`.
4. Confirm the `Deploy Playground` job publishes a `github-pages` URL.

## Benchmark Runbook

- Benchmark mode URL flag: `?benchmark=true`
- Local usage:
  1. Run `npm run dev`
  2. Open `http://localhost:5173/?benchmark=true`
  3. Click **Run Benchmark**
  4. Optionally click **Export JSON** for a schema-versioned report

The benchmark runner uses mmdflux WASM and mermaid through a shared `warm`/`render` contract and reports `mean`, `median`, `p95`, `min`, and `max` values.

Interpretation caveats:

- Results are machine/browser specific and should be compared on the same host/runtime.
- Smoke checks intentionally use reduced scenarios/iterations and are not a replacement for full benchmark studies.
- Benchmark loading is isolated behind route-gated lazy imports to avoid main playground overhead.

## Benchmark Smoke Policy

- Script: `web/scripts/benchmark-smoke.ts`
- Scenarios: `flowchart-small`, `flowchart-medium` (large is intentionally excluded from smoke checks)
- Iterations: `warmup=1`, `measured=3` per scenario/engine
- Thresholds:
  - `mmdflux`: `mean <= 500ms`, `p95 <= 1000ms`
  - `mermaid`: `mean <= 2000ms`, `p95 <= 3500ms`

The goal is to catch catastrophic regressions while avoiding noisy machine-specific failures.

CI wiring is optional by default: `playground-ci.yml` exposes a `workflow_dispatch` input (`run_benchmark_smoke`) so operators can run the smoke step on demand.

## Included Examples

- Flowchart Basics
- Fan-out
- Sequence Basics
- Sequence Retry
- Class Basics
- Class Interfaces

Examples are wired to the live render pipeline and are useful as smoke fixtures for manual regression checks.
