use mmdflux::diagrams::flowchart;

#[test]
fn flowchart_module_exports_definition() {
    let def = flowchart::definition();
    assert_eq!(def.id, "flowchart");
}

#[test]
fn flowchart_detector_works() {
    assert!(flowchart::detect("graph TD\nA-->B"));
    assert!(flowchart::detect("flowchart LR\nA-->B"));
    assert!(!flowchart::detect("pie\n\"A\": 50"));
}

#[test]
fn flowchart_detector_skips_comments() {
    // Detector should skip %% comment lines
    assert!(flowchart::detect("%% comment\ngraph TD\nA-->B"));
    assert!(flowchart::detect(
        "%% line 1\n%% line 2\nflowchart LR\nA-->B"
    ));
}

#[test]
fn flowchart_detector_case_insensitive() {
    // Detector should be case-insensitive
    assert!(flowchart::detect("GRAPH TD\nA-->B"));
    assert!(flowchart::detect("Graph LR\nA-->B"));
    assert!(flowchart::detect("FLOWCHART TD\nA-->B"));
    assert!(flowchart::detect("Flowchart LR\nA-->B"));
}
