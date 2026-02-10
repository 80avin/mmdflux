# List available recipes
default:
    @just --list

# Run all tests
test *args:
    cargo nextest run {{ args }}

# Run all tests (CI mode: no fail-fast, verbose)
test-ci *args:
    cargo nextest run --profile ci {{ args }}

# Run a specific test file (e.g. just test-file integration)
test-file name *args:
    cargo nextest run --test {{ name }} {{ args }}

# Build (debug)
build *args:
    cargo build {{ args }}

# Build (release)
release *args:
    cargo build --release {{ args }}

# Run clippy and fmt check
lint:
    cargo +nightly fmt -- --check
    cargo clippy -- -D warnings

# Run clippy with auto-fix
fix *args:
    cargo clippy --fix --all-targets --all-features --allow-dirty --allow-staged -- -D warnings {{ args }}

# Format code
fmt:
    cargo +nightly fmt

# Run the CLI
run *args:
    cargo run -- {{ args }}

# Run MMDS conformance checks (semantic/layout/visual tiers)
conformance *args:
    cargo nextest run --test mmds_conformance --success-output immediate {{ args }}

# Check that everything compiles, passes lint, and tests
check: lint test

# Build library for wasm32 without CLI-only dependencies
wasm-build:
    cargo build --target wasm32-unknown-unknown --no-default-features --lib

# Validate wasm-safe library + CLI feature contract
wasm-test:
    cargo test --test wasm_cli_contract --test wasm_just_recipes
    cargo check --target wasm32-unknown-unknown --no-default-features --lib
