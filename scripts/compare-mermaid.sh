#!/usr/bin/env bash
# Compare mmdflux ASCII output with Mermaid (mmdc) SVG output for all fixtures.
#
# Usage:
#   ./scripts/compare-mermaid.sh              # all fixtures
#   ./scripts/compare-mermaid.sh double_skip  # single fixture by name
#
# Output goes to /tmp/mmdflux-compare/
# Each fixture gets:
#   <name>.txt (mmdflux text)
#   <name>.mmdflux.svg (mmdflux svg)
#   <name>.mermaid.svg (mermaid svg)
# An index.html is generated for easy side-by-side viewing.

set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
FIXTURES="$REPO/tests/fixtures"
OUTDIR="/tmp/mmdflux-compare"
MMDFLUX="$REPO/target/debug/mmdflux"

mkdir -p "$OUTDIR"

# Build mmdflux if needed
if [[ ! -x "$MMDFLUX" ]]; then
    echo "Building mmdflux..."
    cargo build --quiet --manifest-path "$REPO/Cargo.toml"
fi

# Collect fixture list
if [[ $# -gt 0 ]]; then
    # Filter to requested fixtures
    files=()
    for name in "$@"; do
        f="$FIXTURES/${name}.mmd"
        if [[ -f "$f" ]]; then
            files+=("$f")
        else
            echo "Warning: fixture not found: $f" >&2
        fi
    done
else
    files=("$FIXTURES"/*.mmd)
fi

echo "Comparing ${#files[@]} fixtures..."
echo "Output: $OUTDIR"
echo ""

# Generate outputs
for f in "${files[@]}"; do
    name="$(basename "$f" .mmd)"
    echo -n "  $name ... "

    # mmdflux text output (unicode)
    "$MMDFLUX" "$f" > "$OUTDIR/${name}.txt" 2>/dev/null || true

    # mmdflux SVG output
    "$MMDFLUX" --format svg "$f" > "$OUTDIR/${name}.mmdflux.svg" 2>/dev/null || true

    # Mermaid SVG output
    mmdc -i "$f" -o "$OUTDIR/${name}.mermaid.svg" -b transparent --quiet 2>/dev/null || {
        echo "mmdc failed"
        continue
    }

    echo "done"
done

# Generate HTML comparison page
cat > "$OUTDIR/index.html" <<'HEADER'
<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>mmdflux vs Mermaid Comparison</title>
<style>
  body { font-family: system-ui, sans-serif; margin: 20px; background: #f5f5f5; }
  h1 { margin-bottom: 8px; }
  .subtitle { color: #666; margin-bottom: 24px; }
  .fixture {
    background: white; border: 1px solid #ddd; border-radius: 8px;
    margin-bottom: 24px; padding: 16px;
  }
  .fixture h2 {
    margin: 0 0 4px 0; font-size: 18px;
    cursor: pointer;
  }
  .fixture .filename { color: #888; font-size: 13px; margin-bottom: 12px; }
  .compare {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
    gap: 16px;
    align-items: start;
  }
  .panel { min-width: 0; }
  .panel h3 { margin: 0 0 8px 0; font-size: 14px; color: #555; }
  pre {
    background: #1e1e1e; color: #d4d4d4; padding: 12px; border-radius: 4px;
    overflow: auto; font-size: 13px; line-height: 1.4; max-height: 360px;
    white-space: pre; font-family: 'SF Mono', 'Menlo', 'Monaco', monospace;
  }
  .mermaid-svg {
    border: 1px solid #eee; border-radius: 4px; padding: 8px;
    background: white; text-align: center;
  }
  .mermaid-svg img { max-width: 100%; height: auto; }
  .source pre { background: #f8f8f8; color: #333; font-size: 12px; max-height: 360px; }
</style>
</head>
<body>
<h1>mmdflux vs Mermaid Comparison</h1>
HEADER

echo "<p class=\"subtitle\">Generated: $(date '+%Y-%m-%d %H:%M:%S') &mdash; ${#files[@]} fixtures</p>" >> "$OUTDIR/index.html"

for f in "${files[@]}"; do
    name="$(basename "$f" .mmd)"
    txt_file="$OUTDIR/${name}.txt"
    mmdflux_svg_file="$OUTDIR/${name}.mmdflux.svg"
    mermaid_svg_file="$OUTDIR/${name}.mermaid.svg"

    # Read mmdflux output (HTML-escape it)
    if [[ -f "$txt_file" ]]; then
        ascii_output="$(sed 's/&/\&amp;/g; s/</\&lt;/g; s/>/\&gt;/g' "$txt_file")"
    else
        ascii_output="(no output)"
    fi

    # Read mermaid source
    mmd_source="$(sed 's/&/\&amp;/g; s/</\&lt;/g; s/>/\&gt;/g' "$f")"

    cat >> "$OUTDIR/index.html" <<FIXTURE
<div class="fixture">
  <h2>$name</h2>
  <div class="filename">tests/fixtures/${name}.mmd</div>
  <div class="compare">
    <div class="panel">
      <h3>mmdflux (Text)</h3>
      <pre>${ascii_output}</pre>
    </div>
    <div class="panel">
      <h3>mmdflux (SVG)</h3>
      <div class="mermaid-svg">
        <img src="${name}.mmdflux.svg" alt="${name} mmdflux svg output">
      </div>
    </div>
    <div class="panel">
      <h3>Mermaid (SVG)</h3>
      <div class="mermaid-svg">
        <img src="${name}.mermaid.svg" alt="${name} mermaid output">
      </div>
    </div>
    <div class="panel source">
      <h3>Mermaid Source</h3>
      <pre>${mmd_source}</pre>
    </div>
  </div>
</div>
FIXTURE
done

cat >> "$OUTDIR/index.html" <<'FOOTER'
</body>
</html>
FOOTER

echo ""
echo "Done! Open the comparison page:"
echo "  open $OUTDIR/index.html"
