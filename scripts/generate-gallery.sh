#!/usr/bin/env bash
# Generate a Markdown gallery of mmdflux text + SVG snapshots.
#
# Usage:
#   ./scripts/generate-gallery.sh
#   ./scripts/generate-gallery.sh --out docs/gallery.md
#   ./scripts/generate-gallery.sh simple edge_styles
#
# By default, writes to docs/gallery.md and includes all fixtures.

set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
FIXTURES="$REPO/tests/fixtures/flowchart"
TEXT_SNAPSHOTS="$REPO/tests/snapshots/flowchart"
SVG_SNAPSHOTS="$REPO/tests/svg-snapshots/flowchart"
OUTFILE="$REPO/docs/gallery.md"

usage() {
  cat <<'EOF'
Generate a Markdown gallery of mmdflux text + SVG snapshots.

Usage:
  ./scripts/generate-gallery.sh
  ./scripts/generate-gallery.sh --out docs/gallery.md
  ./scripts/generate-gallery.sh simple edge_styles

Options:
  -o, --out <path>   Output Markdown path (default: docs/gallery.md)
  -h, --help         Show this help
EOF
}

names=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    -o|--out)
      OUTFILE="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      names+=("$1")
      shift
      ;;
  esac
done

files=()
if [[ ${#names[@]} -gt 0 ]]; then
  for name in "${names[@]}"; do
    f="$FIXTURES/${name}.mmd"
    if [[ -f "$f" ]]; then
      files+=("$f")
    else
      echo "Warning: fixture not found: $f" >&2
    fi
  done
else
  while IFS= read -r f; do
    files+=("$f")
  done < <(find "$FIXTURES" -maxdepth 1 -type f -name '*.mmd' | sort)
fi

if [[ ${#files[@]} -eq 0 ]]; then
  echo "No fixtures found." >&2
  exit 1
fi

mkdir -p "$(dirname "$OUTFILE")"
outdir="$(cd "$(dirname "$OUTFILE")" && pwd)"

relpath() {
  python3 - <<'PY' "$1" "$2"
import os
import sys
print(os.path.relpath(sys.argv[1], sys.argv[2]))
PY
}

commit_sha="$(git -C "$REPO" rev-parse --short HEAD 2>/dev/null || echo "unknown")"

{
  echo "# mmdflux gallery"
  echo
  echo "_Generated from commit \`$commit_sha\` — ${#files[@]} fixtures_"
  echo
  echo "This gallery is generated from test fixtures in \`tests/fixtures/flowchart\`,"
  echo "text snapshots in \`tests/snapshots/flowchart\`, and SVG snapshots in \`tests/svg-snapshots/flowchart\`."
  echo
} > "$OUTFILE"

missing_text=0
missing_svg=0

for f in "${files[@]}"; do
  name="$(basename "$f" .mmd)"
  text="$TEXT_SNAPSHOTS/${name}.txt"
  svg="$SVG_SNAPSHOTS/${name}.svg"

  echo "## $name" >> "$OUTFILE"
  echo >> "$OUTFILE"
  echo "\`tests/fixtures/flowchart/${name}.mmd\`" >> "$OUTFILE"
  echo >> "$OUTFILE"

  if [[ -f "$text" ]]; then
    echo "**Text**" >> "$OUTFILE"
    echo >> "$OUTFILE"
    echo '```text' >> "$OUTFILE"
    cat "$text" >> "$OUTFILE"
    printf '\n```\n\n' >> "$OUTFILE"
  else
    echo "> Missing text snapshot: \`tests/snapshots/flowchart/${name}.txt\`" >> "$OUTFILE"
    echo >> "$OUTFILE"
    missing_text=$((missing_text + 1))
  fi

  if [[ -f "$svg" ]]; then
    svg_rel="$(relpath "$svg" "$outdir")"
    echo "**SVG**" >> "$OUTFILE"
    echo >> "$OUTFILE"
    echo "![${name} svg](${svg_rel})" >> "$OUTFILE"
    echo >> "$OUTFILE"
  else
    echo "> Missing SVG snapshot: \`tests/svg-snapshots/flowchart/${name}.svg\`" >> "$OUTFILE"
    echo >> "$OUTFILE"
    missing_svg=$((missing_svg + 1))
  fi

  echo "<details>" >> "$OUTFILE"
  echo "<summary>Mermaid source</summary>" >> "$OUTFILE"
  echo >> "$OUTFILE"
  echo '```mermaid' >> "$OUTFILE"
  cat "$f" >> "$OUTFILE"
  printf '\n```\n\n' >> "$OUTFILE"
  echo "</details>" >> "$OUTFILE"
  echo >> "$OUTFILE"
done

if [[ $missing_text -gt 0 || $missing_svg -gt 0 ]]; then
  echo "---" >> "$OUTFILE"
  echo >> "$OUTFILE"
  echo "**Missing snapshots**" >> "$OUTFILE"
  echo >> "$OUTFILE"
  echo "- Text snapshots missing: $missing_text" >> "$OUTFILE"
  echo "- SVG snapshots missing: $missing_svg" >> "$OUTFILE"
  echo >> "$OUTFILE"
fi

echo "Wrote gallery to $OUTFILE"
