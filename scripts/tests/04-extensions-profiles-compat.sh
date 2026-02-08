#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/tests/_common.sh
source "$SCRIPT_DIR/_common.sh"

OUT_DIR="$(make_out_dir "04-extensions-profiles-compat")"

print_section "Building mmdflux"
ensure_mmdflux_bin

echo "Output dir: $OUT_DIR"

print_section "Unknown extension namespace is tolerated"
"$MMDFLUX_BIN" "$REPO_ROOT/tests/fixtures/mmds/profiles/unknown-extension.json" \
  >"$OUT_DIR/unknown-extension.text.txt"
"$MMDFLUX_BIN" --format svg "$REPO_ROOT/tests/fixtures/mmds/profiles/unknown-extension.json" \
  >"$OUT_DIR/unknown-extension.svg"
cat "$OUT_DIR/unknown-extension.text.txt"

print_section "Mixed known/unknown profiles are tolerated"
"$MMDFLUX_BIN" "$REPO_ROOT/tests/fixtures/mmds/profiles/mixed-known-unknown.json" \
  >"$OUT_DIR/mixed-known-unknown.text.txt"
cat "$OUT_DIR/mixed-known-unknown.text.txt"

print_section "Unknown core version is rejected (expected)"
run_expect_fail \
  "unsupported MMDS core version" \
  "$OUT_DIR/unknown-core-version" \
  "$MMDFLUX_BIN" "$REPO_ROOT/tests/fixtures/mmds/profiles/unknown-core-version.json"

print_section "Malformed extensions payload is rejected (expected)"
run_expect_fail \
  "extensions payload must be object" \
  "$OUT_DIR/extensions-not-object" \
  "$MMDFLUX_BIN" "$REPO_ROOT/tests/fixtures/mmds/invalid/extensions-not-object.json"

printf '\nSaved artifacts under: %s\n' "$OUT_DIR"
