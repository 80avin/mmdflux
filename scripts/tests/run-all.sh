#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUN_ID="${RUN_ID:-$(date +%Y%m%d-%H%M%S)}"
export RUN_ID

scripts=(
  "01-mmds-ingest-validation.sh"
  "02-subgraph-endpoint-intent.sh"
  "03-positioned-capability-matrix.sh"
  "04-extensions-profiles-compat.sh"
  "05-roundtrip-and-conformance.sh"
)

for script in "${scripts[@]}"; do
  echo
  echo "==================== ${script} ===================="
  "$SCRIPT_DIR/$script"
done

echo

echo "All manual checks finished."
echo "Artifacts: $SCRIPT_DIR/out/$RUN_ID"
