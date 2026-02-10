use mmdflux_wasm::{detect, render, version};
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

fn error_debug(error: wasm_bindgen::JsError) -> String {
    format!("{error:?}")
}

#[wasm_bindgen_test]
fn renders_flowchart_text() {
    let output = render("graph TD\nA-->B", "text", "{}").expect("render should succeed");
    assert!(output.contains("A"));
    assert!(output.contains("B"));
}

#[wasm_bindgen_test]
fn renders_flowchart_svg() {
    let output = render("graph TD\nA-->B", "svg", "{}").expect("svg render should succeed");
    assert!(output.contains("<svg"));
}

#[wasm_bindgen_test]
fn detect_returns_flowchart_id() {
    assert_eq!(detect("graph TD\nA-->B"), Some("flowchart".to_string()));
}

#[wasm_bindgen_test]
fn detect_returns_none_for_unknown_input() {
    assert_eq!(detect("this is not mermaid"), None);
}

#[wasm_bindgen_test]
fn rejects_unknown_format() {
    let error = render("graph TD\nA-->B", "nope", "{}").expect_err("unknown format must error");
    assert!(error_debug(error).contains("unknown output format"));
}

#[wasm_bindgen_test]
fn rejects_unknown_diagram_type() {
    let error = render("not mermaid at all", "text", "{}").expect_err("unknown diagram must fail");
    assert!(error_debug(error).contains("unknown diagram type"));
}

#[wasm_bindgen_test]
fn rejects_invalid_config_json() {
    let error = render("graph TD\nA-->B", "text", "{").expect_err("invalid config must fail");
    assert!(error_debug(error).contains("invalid config_json"));
}

#[wasm_bindgen_test]
fn version_matches_package_version() {
    assert_eq!(version(), env!("CARGO_PKG_VERSION"));
}
