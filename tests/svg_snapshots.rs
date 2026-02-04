use std::fs;
use std::path::{Path, PathBuf};

use mmdflux::render::{RenderOptions, render_svg};
use mmdflux::{build_diagram, parse_flowchart};

fn load_fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("Failed to read fixture {name}: {e}"))
}

fn render_svg_fixture(name: &str) -> String {
    let input = load_fixture(name);
    let flowchart = parse_flowchart(&input).expect("Failed to parse fixture");
    let diagram = build_diagram(&flowchart);
    render_svg(&diagram, &RenderOptions::default_svg())
}

fn snapshot_path(stem: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("svg-snapshots")
        .join(format!("{stem}.svg"))
}

fn assert_snapshot(fixture: &str) {
    let stem = fixture.trim_end_matches(".mmd");
    let output = render_svg_fixture(fixture);
    let path = snapshot_path(stem);

    if std::env::var("GENERATE_SVG_SNAPSHOTS").is_ok() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, &output).unwrap();
    }

    let expected = fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("Missing snapshot: {}", path.display()));
    assert_eq!(output, expected, "Snapshot mismatch for {fixture}");
}

#[test]
fn svg_snapshot_simple() {
    assert_snapshot("simple.mmd");
}

#[test]
fn svg_snapshot_edge_styles() {
    assert_snapshot("edge_styles.mmd");
}

#[test]
fn svg_snapshot_nested_subgraph() {
    assert_snapshot("nested_subgraph.mmd");
}

#[test]
fn svg_snapshot_simple_cycle() {
    assert_snapshot("simple_cycle.mmd");
}

#[test]
fn svg_snapshot_left_right() {
    assert_snapshot("left_right.mmd");
}

#[test]
fn svg_snapshot_right_left() {
    assert_snapshot("right_left.mmd");
}

#[test]
fn svg_snapshot_bottom_top() {
    assert_snapshot("bottom_top.mmd");
}
