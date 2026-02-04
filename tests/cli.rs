//! CLI integration tests for mmdflux binary.

use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;

fn mmdflux() -> Command {
    cargo_bin_cmd!("mmdflux")
}

// =============================================================================
// Debug Flag Tests
// =============================================================================

#[test]
fn cli_debug_shows_detected_diagram_type() {
    mmdflux()
        .arg("--debug")
        .write_stdin("graph TD\nA-->B")
        .assert()
        .success()
        .stderr(predicate::str::contains("Detected diagram type: flowchart"));
}

#[test]
fn cli_debug_shows_pie_type() {
    mmdflux()
        .arg("--debug")
        .write_stdin("pie\n\"A\": 50")
        .assert()
        .success()
        .stderr(predicate::str::contains("Detected diagram type: pie"));
}

#[test]
fn cli_debug_shows_info_type() {
    mmdflux()
        .arg("--debug")
        .write_stdin("info")
        .assert()
        .success()
        .stderr(predicate::str::contains("Detected diagram type: info"));
}

#[test]
fn cli_debug_shows_packet_type() {
    mmdflux()
        .arg("--debug")
        .write_stdin("packet-beta\n0-7: \"Header\"")
        .assert()
        .success()
        .stderr(predicate::str::contains("Detected diagram type: packet"));
}

// =============================================================================
// SVG Format Error Tests
// =============================================================================

#[test]
fn cli_svg_format_errors_for_flowchart() {
    mmdflux()
        .args(["--format", "svg"])
        .write_stdin("graph TD\nA-->B")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "flowchart diagrams do not support svg output",
        ));
}

#[test]
fn cli_svg_format_errors_for_pie() {
    mmdflux()
        .args(["--format", "svg"])
        .write_stdin("pie\n\"A\": 50")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "pie diagrams do not support svg output",
        ));
}

#[test]
fn cli_svg_format_errors_for_info() {
    mmdflux()
        .args(["--format", "svg"])
        .write_stdin("info")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "info diagrams do not support svg output",
        ));
}

#[test]
fn cli_svg_format_errors_for_packet() {
    mmdflux()
        .args(["--format", "svg"])
        .write_stdin("packet-beta")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "packet diagrams do not support svg output",
        ));
}

// =============================================================================
// Basic CLI Functionality
// =============================================================================

#[test]
fn cli_renders_flowchart_to_stdout() {
    mmdflux()
        .write_stdin("graph TD\nA[Start]-->B[End]")
        .assert()
        .success()
        .stdout(predicate::str::contains("Start"))
        .stdout(predicate::str::contains("End"));
}

#[test]
fn cli_renders_ascii_mode() {
    mmdflux()
        .args(["--format", "ascii"])
        .write_stdin("graph TD\nA-->B")
        .assert()
        .success()
        // ASCII mode uses + for corners, not Unicode box-drawing
        .stdout(predicate::str::contains("+"));
}

#[test]
fn cli_unknown_diagram_type_errors() {
    mmdflux()
        .write_stdin("sequenceDiagram\nA->>B: hello")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Unknown diagram type"));
}

// =============================================================================
// Simple Diagram Shim Output Tests
// =============================================================================

#[test]
fn cli_pie_renders_with_header() {
    mmdflux()
        .write_stdin("pie\n\"Apples\": 50\n\"Oranges\": 50")
        .assert()
        .success()
        .stdout(predicate::str::contains("[Pie Chart]"))
        .stdout(predicate::str::contains("Apples"));
}

#[test]
fn cli_info_renders_version() {
    mmdflux()
        .write_stdin("info")
        .assert()
        .success()
        .stdout(predicate::str::contains("mmdflux"));
}

#[test]
fn cli_packet_renders_with_header() {
    mmdflux()
        .write_stdin("packet-beta\n0-7: \"Header\"")
        .assert()
        .success()
        .stdout(predicate::str::contains("[Packet Diagram]"))
        .stdout(predicate::str::contains("Header"));
}
