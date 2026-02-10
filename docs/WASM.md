# WASM Build and Test Commands

Use these reproducible local entrypoints when validating WASM readiness.

## Prerequisite

Install the WASM target and wasm-pack once per environment:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --locked
```

`just wasm-test` runs browser tests in headless Chrome. The helper script
`scripts/run-wasm-browser-tests.sh` will auto-detect Chrome/Chromium and
download a matching Chromedriver into `target/chromedriver/` when needed.
You can override detection with `BROWSER=/path/to/chrome` and
`CHROMEDRIVER=/path/to/chromedriver`.

## Commands

```bash
just wasm-build
just wasm-test
```

- `just wasm-build` compiles the library for `wasm32-unknown-unknown` with
  both `web` and `bundler` wasm-pack targets.
- `just wasm-test` runs browser-executed wasm-bindgen contract tests for
  `crates/mmdflux-wasm`.

## npm Release Contract

WASM publishing is tag-driven via:

- `.github/workflows/wasm-release.yml`

Rules enforced by the workflow:

- Release tags must be `v*` and tag version must equal crate version.
- Root `Cargo.toml` version and `crates/mmdflux-wasm/Cargo.toml` version must match.
- Bundler package is published to npm as `@mmds/wasm`.

Required repository setup:

- `NPM_TOKEN` secret with publish rights to the `@mmds` npm organization.

Local preflight before tagging:

```bash
cargo test --features cli
just wasm-build
just wasm-test
```
