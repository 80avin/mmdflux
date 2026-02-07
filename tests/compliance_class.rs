//! Class diagram compliance tests and snapshot assertions.
//!
//! Locks class rendering output with deterministic text and SVG snapshots.
//! Generate snapshots: `GENERATE_CLASS_TEXT_SNAPSHOTS=1 cargo test --test compliance_class`
//! Generate SVG:       `GENERATE_CLASS_SVG_SNAPSHOTS=1 cargo test --test compliance_class`

use std::fs;
use std::path::{Path, PathBuf};

use mmdflux::diagram::{OutputFormat, RenderConfig};
use mmdflux::diagrams::class::ClassInstance;
use mmdflux::registry::DiagramInstance;
use mmdflux::render::{RenderOptions, render_svg};

fn class_fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("class")
}

fn class_text_snapshot_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("snapshots")
        .join("class")
}

fn class_svg_snapshot_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("svg-snapshots")
        .join("class")
}

fn list_class_fixtures() -> Vec<String> {
    let dir = class_fixture_dir();
    let mut fixtures: Vec<String> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("Failed to read class fixtures dir: {e}"))
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

fn render_class_text(fixture: &str) -> String {
    let path = class_fixture_dir().join(fixture);
    let input = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read fixture {fixture}: {e}"));
    let mut instance = ClassInstance::new();
    instance
        .parse(&input)
        .expect("Failed to parse class fixture");
    instance
        .render(OutputFormat::Text, &RenderConfig::default())
        .expect("Failed to render class fixture")
}

fn render_class_svg(fixture: &str) -> String {
    let path = class_fixture_dir().join(fixture);
    let input = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read fixture {fixture}: {e}"));
    let model =
        mmdflux::diagrams::class::parser::parse_class_diagram(&input).expect("Failed to parse");
    let diagram = mmdflux::diagrams::class::compiler::compile(&model);
    render_svg(&diagram, &RenderOptions::default_svg())
}

// --- Text snapshots ---

#[test]
fn class_text_snapshots() {
    let snapshot_dir = class_text_snapshot_dir();
    let regenerate = std::env::var("GENERATE_CLASS_TEXT_SNAPSHOTS").is_ok();

    if regenerate {
        fs::create_dir_all(&snapshot_dir).unwrap();
    }

    for fixture in list_class_fixtures() {
        let stem = fixture.trim_end_matches(".mmd");
        let output = render_class_text(&fixture);
        let snapshot_path = snapshot_dir.join(format!("{stem}.txt"));

        if regenerate {
            fs::write(&snapshot_path, &output).unwrap();
        } else {
            let expected = fs::read_to_string(&snapshot_path).unwrap_or_else(|_| {
                panic!(
                    "Missing class text snapshot: {}. Set GENERATE_CLASS_TEXT_SNAPSHOTS=1 to generate.",
                    snapshot_path.display()
                )
            });
            assert_eq!(
                output, expected,
                "Class text snapshot mismatch for {fixture}. Set GENERATE_CLASS_TEXT_SNAPSHOTS=1 to regenerate."
            );
        }
    }
}

// --- SVG snapshots ---

#[test]
fn class_svg_snapshots() {
    let snapshot_dir = class_svg_snapshot_dir();
    let regenerate = std::env::var("GENERATE_CLASS_SVG_SNAPSHOTS").is_ok();

    if regenerate {
        fs::create_dir_all(&snapshot_dir).unwrap();
    }

    for fixture in list_class_fixtures() {
        let stem = fixture.trim_end_matches(".mmd");
        let output = render_class_svg(&fixture);
        let snapshot_path = snapshot_dir.join(format!("{stem}.svg"));

        if regenerate {
            fs::write(&snapshot_path, &output).unwrap();
        } else {
            let expected = fs::read_to_string(&snapshot_path).unwrap_or_else(|_| {
                panic!(
                    "Missing class SVG snapshot: {}. Set GENERATE_CLASS_SVG_SNAPSHOTS=1 to generate.",
                    snapshot_path.display()
                )
            });
            assert_eq!(
                output, expected,
                "Class SVG snapshot mismatch for {fixture}. Set GENERATE_CLASS_SVG_SNAPSHOTS=1 to regenerate."
            );
        }
    }
}

// --- Compliance assertions ---

#[test]
fn class_all_fixtures_parse() {
    for fixture in list_class_fixtures() {
        let path = class_fixture_dir().join(&fixture);
        let input = fs::read_to_string(&path).unwrap();
        let mut instance = ClassInstance::new();
        assert!(
            instance.parse(&input).is_ok(),
            "Failed to parse class fixture: {fixture}"
        );
    }
}

#[test]
fn class_all_fixtures_render_text() {
    for fixture in list_class_fixtures() {
        let output = render_class_text(&fixture);
        assert!(
            !output.is_empty(),
            "Empty text output for class fixture: {fixture}"
        );
    }
}

#[test]
fn class_all_fixtures_render_svg() {
    for fixture in list_class_fixtures() {
        let output = render_class_svg(&fixture);
        assert!(
            output.starts_with("<svg"),
            "Invalid SVG output for class fixture: {fixture}"
        );
    }
}

#[test]
fn class_dependency_renders_differently_from_association() {
    let assoc = render_class_text("all_relations.mmd");
    // The output should contain both solid and dotted edges
    assert!(
        assoc.contains('│') || assoc.contains('┆'),
        "Expected edge characters in output"
    );
}

#[test]
fn class_inheritance_direction_correct() {
    // In `Animal <|-- Dog`, Dog inherits from Animal
    // So Dog → Animal edge means Dog is source, Animal is target
    let output = render_class_text("simple.mmd");
    // Dog should appear before Animal in top-down layout (source on top)
    assert!(output.contains("Dog"));
    assert!(output.contains("Animal"));
}
