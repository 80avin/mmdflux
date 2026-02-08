#!/usr/bin/env bash

set -euo pipefail

TESTS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$TESTS_DIR/../.." && pwd)"
MMDFLUX_BIN="$REPO_ROOT/target/debug/mmdflux"
RUN_ID="${RUN_ID:-$(date +%Y%m%d-%H%M%S)}"

ensure_mmdflux_bin() {
  cargo build --quiet --manifest-path "$REPO_ROOT/Cargo.toml" --bin mmdflux
}

make_out_dir() {
  local name="$1"
  local dir="$REPO_ROOT/scripts/tests/out/$RUN_ID/$name"
  mkdir -p "$dir"
  printf '%s\n' "$dir"
}

print_section() {
  local title="$1"
  printf '\n== %s ==\n' "$title"
}

run_expect_fail() {
  local label="$1"
  local out_prefix="$2"
  shift 2

  if "$@" >"${out_prefix}.stdout" 2>"${out_prefix}.stderr"; then
    echo "[unexpected pass] $label"
    echo "expected command to fail"
    return 1
  fi

  echo "[expected failure] $label"
  cat "${out_prefix}.stderr"
}
