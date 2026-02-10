# WASM Build and Test Commands

Use these reproducible local entrypoints when validating WASM readiness.

## Prerequisite

Install the WASM target once per Rust toolchain:

```bash
rustup target add wasm32-unknown-unknown
```

## Commands

```bash
just wasm-build
just wasm-test
```

- `just wasm-build` compiles the library for `wasm32-unknown-unknown` with
  `--no-default-features` so CLI-only dependencies are excluded.
- `just wasm-test` runs WASM/CLI contract tests plus the wasm32 library compile check.
