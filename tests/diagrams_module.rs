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
