#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/tests/_common.sh
source "$SCRIPT_DIR/_common.sh"

SWEEP_RUN_ID="${RUN_ID}-unified-vs-full-sweep"
OUT_DIR="$REPO_ROOT/scripts/tests/out/$SWEEP_RUN_ID"
mkdir -p "$OUT_DIR"

FLOW_STYLES_DEFAULT=("basis" "linear" "rounded" "orthogonal")
CLASS_STYLES_DEFAULT=("basis")
UNIFIED_FEEDBACK_BASELINE_HEADER=$'fixture\tstyle\tstatus\tdiff_lines\tfull_viewbox_width\tfull_viewbox_height\tunified_viewbox_width\tunified_viewbox_height\tviewbox_width_delta\tviewbox_height_delta'

split_list() {
  local raw="${1:-}"
  if [[ -z "$raw" ]]; then
    return 0
  fi

  local normalized
  normalized="$(printf '%s' "$raw" | tr ',' ' ')"
  local IFS=$' \t\n'
  # shellcheck disable=SC2206
  local items=($normalized)
  printf '%s\n' "${items[@]}"
}

collect_fixtures() {
  local family="$1"
  local filter_raw="$2"
  local dir="$REPO_ROOT/tests/fixtures/$family"

  if [[ -n "$filter_raw" ]]; then
    while IFS= read -r item; do
      [[ -z "$item" ]] && continue
      local name="$item"
      if [[ "$name" != *.mmd ]]; then
        name="${name}.mmd"
      fi
      local path="$dir/$name"
      if [[ ! -f "$path" ]]; then
        echo "Missing fixture: $path" >&2
        exit 1
      fi
      printf '%s\n' "$path"
    done < <(split_list "$filter_raw")
    return 0
  fi

  find "$dir" -maxdepth 1 -type f -name '*.mmd' | sort
}

collect_styles() {
  local env_value="$1"
  shift
  local defaults=("$@")

  if [[ -n "$env_value" ]]; then
    split_list "$env_value"
    return 0
  fi

  printf '%s\n' "${defaults[@]}"
}

render_svg() {
  local mode="$1"
  local style="$2"
  local fixture_path="$3"
  local out_file="$4"

  "$MMDFLUX_BIN" \
    --format svg \
    --geometry-level routed \
    --routing-mode "$mode" \
    --svg-edge-path-style "$style" \
    "$fixture_path" >"$out_file"
}

extract_viewbox_dimensions() {
  local svg_file="$1"
  local viewbox
  viewbox="$(grep -m1 -o 'viewBox="[^"]*"' "$svg_file" | sed -E 's/viewBox="([^"]*)"/\1/' || true)"
  if [[ -z "$viewbox" ]]; then
    printf '0 0\n'
    return 0
  fi

  local _x _y width height
  read -r _x _y width height <<<"$viewbox"
  if [[ -z "${width:-}" || -z "${height:-}" ]]; then
    printf '0 0\n'
    return 0
  fi

  printf '%s %s\n' "$width" "$height"
}

format_delta() {
  local baseline="$1"
  local candidate="$2"
  awk -v baseline="$baseline" -v candidate="$candidate" 'BEGIN { printf "%.2f", (candidate - baseline) }'
}

render_family_style() {
  local family="$1"
  local style="$2"
  shift 2
  local fixtures=("$@")
  local report="$OUT_DIR/${family}.svg.${style}.report.tsv"

  : >"$report"
  for fixture_path in "${fixtures[@]}"; do
    local fixture_name
    fixture_name="$(basename "$fixture_path")"
    local base_name="${fixture_name%.mmd}"
    local slug="${family}_${base_name}_svg_${style}"
    local full_svg="$OUT_DIR/${slug}.full.svg"
    local unified_svg="$OUT_DIR/${slug}.unified.svg"
    local diff_file="$OUT_DIR/${slug}.diff"
    local status="same"
    local diff_lines="0"
    local full_viewbox_width="0"
    local full_viewbox_height="0"
    local unified_viewbox_width="0"
    local unified_viewbox_height="0"
    local viewbox_width_delta="0.00"
    local viewbox_height_delta="0.00"

    render_svg "full-compute" "$style" "$fixture_path" "$full_svg"
    render_svg "unified-preview" "$style" "$fixture_path" "$unified_svg"
    read -r full_viewbox_width full_viewbox_height <<<"$(extract_viewbox_dimensions "$full_svg")"
    read -r unified_viewbox_width unified_viewbox_height <<<"$(extract_viewbox_dimensions "$unified_svg")"
    viewbox_width_delta="$(format_delta "$full_viewbox_width" "$unified_viewbox_width")"
    viewbox_height_delta="$(format_delta "$full_viewbox_height" "$unified_viewbox_height")"

    if diff -u "$full_svg" "$unified_svg" >"$diff_file"; then
      status="same"
      diff_lines="0"
    else
      status="diff"
      diff_lines="$(wc -l <"$diff_file" | tr -d ' ')"
    fi

    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
      "$fixture_name" \
      "$status" \
      "$diff_lines" \
      "$full_viewbox_width" \
      "$full_viewbox_height" \
      "$unified_viewbox_width" \
      "$unified_viewbox_height" \
      "$viewbox_width_delta" \
      "$viewbox_height_delta" >>"$report"
  done
}

generate_unified_feedback_baseline() {
  local baseline="$OUT_DIR/unified-feedback-baseline.tsv"

  printf '%s\n' "$UNIFIED_FEEDBACK_BASELINE_HEADER" >"$baseline"
  for style in "${FLOW_STYLES[@]}"; do
    local report="$OUT_DIR/flowchart.svg.${style}.report.tsv"
    [[ -f "$report" ]] || continue
    awk -F $'\t' -v style="$style" '
      BEGIN { OFS = "\t" }
      NF >= 9 { print $1, style, $2, $3, $4, $5, $6, $7, $8, $9 }
    ' "$report" >>"$baseline"
  done

  printf '%s\n' "$baseline"
}

style_badge_class() {
  case "$1" in
    basis) printf 'style-basis' ;;
    linear) printf 'style-linear' ;;
    rounded) printf 'style-rounded' ;;
    orthogonal) printf 'style-orthogonal' ;;
    *) printf 'style-basis' ;;
  esac
}

family_badge_class() {
  case "$1" in
    flowchart) printf 'family-flowchart' ;;
    class) printf 'family-class' ;;
    *) printf 'family-flowchart' ;;
  esac
}

html_escape() {
  printf '%s' "$1" | sed -e 's/&/\&amp;/g' -e 's/</\&lt;/g' -e 's/>/\&gt;/g'
}

generate_gallery() {
  local out_html="$OUT_DIR/routing-svg-diff-gallery-v2.html"

  cat >"$out_html" <<'HTML_HEADER'
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>SVG Diff Gallery (Full-Compute vs Unified-Preview)</title>
  <style>
    :root { --bg:#0b1020; --fg:#e8ebf3; --muted:#98a1b3; --card:#141a2b; --ok:#2f9e44; --bad:#e03131; --border:#2a334c; }
    * { box-sizing:border-box; }
    body { margin:0; background:var(--bg); color:var(--fg); font:14px/1.45 ui-sans-serif, -apple-system, Segoe UI, Roboto, Helvetica, Arial, sans-serif; }
    main { max-width:1700px; margin:0 auto; padding:18px; }
    h1 { margin:0 0 8px; font-size:24px; }
    .sub { margin:0 0 14px; color:var(--muted); }
    .controls { display:flex; flex-wrap:wrap; gap:12px; align-items:center; margin:0 0 14px; }
    .controls button { background:#1d2640; color:var(--fg); border:1px solid var(--border); border-radius:8px; padding:6px 10px; cursor:pointer; }
    .controls label { color:var(--muted); user-select:none; }
    .controls input[type='text'] { background:#111827; color:var(--fg); border:1px solid var(--border); border-radius:8px; padding:6px 8px; min-width:260px; }
    .group { margin:18px 0; padding:12px; background:var(--card); border:1px solid var(--border); border-radius:10px; }
    .group h2 { margin:0 0 4px; font-size:18px; }
    .meta { margin:0 0 10px; color:var(--muted); font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, monospace; }
    .family-badge { font-size:11px; font-weight:700; text-transform:uppercase; letter-spacing:.4px; border-radius:999px; padding:2px 7px; border:1px solid #334155; color:#cbd5e1; background:#0f172a; }
    .family-flowchart { border-color:#0ea5e9; color:#bae6fd; background:#0c4a6e55; }
    .family-class { border-color:#f97316; color:#fed7aa; background:#7c2d1255; }
    .style-badge { font-size:11px; font-weight:700; text-transform:uppercase; letter-spacing:.4px; border-radius:999px; padding:2px 7px; }
    .style-basis { background:#1e3a8a55; color:#bfdbfe; border:1px solid #1e40af; }
    .style-linear { background:#36531455; color:#d9f99d; border:1px solid #4d7c0f; }
    .style-rounded { background:#78350f55; color:#fde68a; border:1px solid #a16207; }
    .style-orthogonal { background:#4c1d9555; color:#ddd6fe; border:1px solid #6d28d9; }
    details.fixture { margin:8px 0; border:1px solid var(--border); border-radius:8px; overflow:hidden; }
    details.fixture > summary { list-style:none; cursor:pointer; display:flex; gap:10px; align-items:center; flex-wrap:wrap; background:#10172a; padding:8px 10px; }
    details.fixture > summary::-webkit-details-marker { display:none; }
    .name { font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, monospace; font-weight:600; }
    .pill { font-size:11px; text-transform:uppercase; border-radius:999px; padding:2px 7px; }
    .pill.same { background:rgba(47,158,68,.2); color:#8ce99a; }
    .pill.diff { background:rgba(224,49,49,.2); color:#ffa8a8; }
    .lines { color:var(--muted); margin-left:auto; }
    .links { display:flex; gap:10px; }
    .links a { color:#9ec5ff; text-decoration:none; font-size:12px; }
    .compare { display:grid; grid-template-columns:1fr 1fr; gap:10px; background:#0f1526; padding:10px; }
    .pane { border:1px solid var(--border); border-radius:8px; overflow:hidden; background:#fff; color:#111; }
    .pane h3 { margin:0; padding:6px 8px; font-size:12px; text-transform:uppercase; letter-spacing:.4px; background:#f3f4f8; border-bottom:1px solid #d8dbe6; color:#333; }
    .pane object { display:block; width:100%; height:400px; }
    .hidden { display:none !important; }
    @media (max-width: 1000px) { .compare { grid-template-columns:1fr; } .pane object { height:320px; } }
  </style>
</head>
<body>
  <main>
    <h1>SVG Diff Gallery: Full-Compute vs Unified-Preview</h1>
    <p class="sub">No main-branch pane. Use filters to triage visual issues and document findings.</p>
    <div class="controls">
      <button onclick="openShown(true)">Open shown</button>
      <button onclick="openShown(false)">Close shown</button>
      <label><input id="hideSame" type="checkbox" checked onchange="applyFilters()"> Hide same</label>
      <label><input id="basisOnly" type="checkbox" onchange="applyFilters()"> Basis only</label>
      <label><input id="familyFlowchart" type="checkbox" checked onchange="applyFilters()"> flowchart</label>
      <label><input id="familyClass" type="checkbox" checked onchange="applyFilters()"> class</label>
      <label><input id="styleBasis" type="checkbox" checked onchange="applyFilters()"> basis</label>
      <label><input id="styleLinear" type="checkbox" checked onchange="applyFilters()"> linear</label>
      <label><input id="styleRounded" type="checkbox" checked onchange="applyFilters()"> rounded</label>
      <label><input id="styleOrthogonal" type="checkbox" checked onchange="applyFilters()"> orthogonal</label>
      <input id="search" type="text" placeholder="Filter fixture name (e.g. multi_subgraph)" oninput="applyFilters()" />
    </div>
HTML_HEADER

  for family in flowchart class; do
    local family_styles=()
    if [[ "$family" == "class" ]]; then
      family_styles=("${CLASS_STYLES[@]}")
    else
      family_styles=("${FLOW_STYLES[@]}")
    fi

    for style in "${family_styles[@]}"; do
      local report="$OUT_DIR/${family}.svg.${style}.report.tsv"
      [[ -f "$report" ]] || continue

      local fixtures
      fixtures="$(wc -l <"$report" | tr -d ' ')"
      [[ "$fixtures" -gt 0 ]] || continue
      local diff_count
      diff_count="$(awk -F $'\t' '$2=="diff"{c++} END{print c+0}' "$report")"
      local same_count=$((fixtures - diff_count))
      local family_upper
      family_upper="$(printf '%s' "$family" | tr '[:lower:]' '[:upper:]')"
      local style_class
      style_class="$(style_badge_class "$style")"

      cat >>"$out_html" <<HTML_GROUP
<section class="group" data-style-group="$style" data-family-group="$family">
  <h2>${family_upper} SVG <span class="style-badge ${style_class}">${style}</span></h2>
  <p class="meta">fixtures=${fixtures} | diff=${diff_count} | same=${same_count}</p>
HTML_GROUP

      while IFS=$'\t' read -r fixture_name status diff_lines _full_w _full_h _unified_w _unified_h _delta_w _delta_h; do
        local fixture_base="${fixture_name%.mmd}"
        local fixture_slug="${family}_${fixture_base}_svg_${style}"
        local full_file="${fixture_slug}.full.svg"
        local unified_file="${fixture_slug}.unified.svg"
        local diff_file="${fixture_slug}.diff"
        local status_class="$status"
        local style_badge
        style_badge="$(style_badge_class "$style")"
        local family_badge
        family_badge="$(family_badge_class "$family")"
        local fixture_escaped
        fixture_escaped="$(html_escape "$fixture_name")"

        cat >>"$out_html" <<HTML_FIXTURE
  <details class="fixture ${status_class}" data-style="${style}" data-family="${family}" data-fixture="${fixture_escaped}">
    <summary>
      <span class="name">${fixture_escaped}</span>
      <span class="pill ${status_class}">${status_class}</span>
      <span class="family-badge ${family_badge}">${family}</span>
      <span class="style-badge ${style_badge}">${style}</span>
      <span class="lines">diff lines: ${diff_lines}</span>
      <span class="links">
        <a href="${diff_file}" target="_blank">diff</a>
        <a href="${full_file}" target="_blank">full</a>
        <a href="${unified_file}" target="_blank">unified</a>
      </span>
    </summary>
    <div class="compare">
      <div class="pane">
        <h3>full-compute</h3>
        <object data="${full_file}" type="image/svg+xml"></object>
      </div>
      <div class="pane">
        <h3>unified-preview</h3>
        <object data="${unified_file}" type="image/svg+xml"></object>
      </div>
    </div>
  </details>
HTML_FIXTURE
      done <"$report"

      printf '</section>\n' >>"$out_html"
    done
  done

  cat >>"$out_html" <<'HTML_FOOTER'
  </main>
  <script>
    function openShown(open) {
      document.querySelectorAll('details.fixture').forEach(d => {
        if (!d.classList.contains('hidden')) d.open = open;
      });
    }

    function applyFilters() {
      const hideSame = document.getElementById('hideSame').checked;
      const basisOnly = document.getElementById('basisOnly').checked;
      const search = document.getElementById('search').value.trim().toLowerCase();

      const allowedFamilies = new Set();
      if (document.getElementById('familyFlowchart').checked) allowedFamilies.add('flowchart');
      if (document.getElementById('familyClass').checked) allowedFamilies.add('class');

      const allowedStyles = new Set();
      if (document.getElementById('styleBasis').checked) allowedStyles.add('basis');
      if (document.getElementById('styleLinear').checked) allowedStyles.add('linear');
      if (document.getElementById('styleRounded').checked) allowedStyles.add('rounded');
      if (document.getElementById('styleOrthogonal').checked) allowedStyles.add('orthogonal');
      if (basisOnly) {
        allowedStyles.clear();
        allowedStyles.add('basis');
      }

      document.querySelectorAll('details.fixture').forEach(d => {
        const same = d.classList.contains('same');
        const style = d.dataset.style;
        const family = d.dataset.family;
        const fixture = d.dataset.fixture.toLowerCase();

        const visible = (!hideSame || !same)
          && allowedFamilies.has(family)
          && allowedStyles.has(style)
          && (search.length === 0 || fixture.includes(search));

        d.classList.toggle('hidden', !visible);
      });

      document.querySelectorAll('section.group').forEach(section => {
        const style = section.dataset.styleGroup;
        const family = section.dataset.familyGroup;
        const sectionAllowed = allowedFamilies.has(family) && allowedStyles.has(style);
        const hasVisible = Array.from(section.querySelectorAll('details.fixture')).some(d => !d.classList.contains('hidden'));
        section.classList.toggle('hidden', !sectionAllowed || !hasVisible);
      });
    }

    applyFilters();
  </script>
</body>
</html>
HTML_FOOTER

  printf '%s\n' "$out_html"
}

print_section "Building mmdflux"
ensure_mmdflux_bin
echo "Output dir: $OUT_DIR"

old_ifs="$IFS"
IFS=$'\n'
# shellcheck disable=SC2207
FLOW_FIXTURES=($(collect_fixtures "flowchart" "${FLOW_FIXTURES:-}"))
# shellcheck disable=SC2207
CLASS_FIXTURES=($(collect_fixtures "class" "${CLASS_FIXTURES:-}"))
# shellcheck disable=SC2207
FLOW_STYLES=($(collect_styles "${FLOW_STYLES:-}" "${FLOW_STYLES_DEFAULT[@]}"))
# shellcheck disable=SC2207
CLASS_STYLES=($(collect_styles "${CLASS_STYLES:-}" "${CLASS_STYLES_DEFAULT[@]}"))
IFS="$old_ifs"

print_section "Rendering flowchart fixture/style matrix"
for style in "${FLOW_STYLES[@]}"; do
  echo "flowchart style=$style fixtures=${#FLOW_FIXTURES[@]}"
  render_family_style "flowchart" "$style" "${FLOW_FIXTURES[@]}"
done

print_section "Rendering class fixture/style matrix"
for style in "${CLASS_STYLES[@]}"; do
  echo "class style=$style fixtures=${#CLASS_FIXTURES[@]}"
  render_family_style "class" "$style" "${CLASS_FIXTURES[@]}"
done

print_section "Generating gallery"
GALLERY_PATH="$(generate_gallery)"

print_section "Generating unified feedback baseline"
BASELINE_PATH="$(generate_unified_feedback_baseline)"

echo
echo "Unified-vs-full SVG sweep complete."
echo "Artifacts: $OUT_DIR"
echo "Gallery: $GALLERY_PATH"
echo "Unified feedback baseline: $BASELINE_PATH"
