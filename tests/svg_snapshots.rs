use std::fs;
use std::path::{Path, PathBuf};

use mmdflux::render::{RenderOptions, render_svg};
use mmdflux::{build_diagram, parse_flowchart};

fn list_fixtures() -> Vec<String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");
    let mut fixtures: Vec<String> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("Failed to read fixtures dir: {e}"))
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension().is_some_and(|e| e == "mmd") {
                Some(path.file_name()?.to_str()?.to_string())
            } else {
                None
            }
        })
        .collect();
    fixtures.sort();
    fixtures
}

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
fn svg_snapshot_all_fixtures() {
    for fixture in list_fixtures() {
        assert_snapshot(&fixture);
    }
}
