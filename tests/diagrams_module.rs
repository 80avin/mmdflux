use mmdflux::diagrams::{class, flowchart, sequence};

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

// --- Class diagram module ---

#[test]
fn class_module_exports_definition() {
    let def = class::definition();
    assert_eq!(def.id, "class");
}

#[test]
fn class_detector_works() {
    assert!(class::detect("classDiagram\nclass User"));
    assert!(!class::detect("graph TD\nA-->B"));
    assert!(!class::detect("pie\n\"A\": 50"));
}

#[test]
fn class_detector_case_insensitive() {
    assert!(class::detect("CLASSDIAGRAM\nclass User"));
    assert!(class::detect("ClassDiagram\nclass User"));
}

#[test]
fn class_detector_skips_comments() {
    assert!(class::detect("%% comment\nclassDiagram\nclass User"));
}

#[test]
fn flowchart_not_detected_as_class() {
    assert!(!class::detect("graph TD\nclassA --> classB"));
}

#[test]
fn class_not_detected_as_flowchart() {
    assert!(!flowchart::detect("classDiagram\nclass User"));
}

// --- Sequence diagram module ---

#[test]
fn sequence_module_exports_definition() {
    let def = sequence::definition();
    assert_eq!(def.id, "sequence");
}

#[test]
fn sequence_detector_works() {
    assert!(sequence::detect("sequenceDiagram\nparticipant A"));
    assert!(!sequence::detect("graph TD\nA-->B"));
    assert!(!sequence::detect("classDiagram\nclass User"));
}

#[test]
fn sequence_detector_case_insensitive() {
    assert!(sequence::detect("SEQUENCEDIAGRAM\nparticipant A"));
    assert!(sequence::detect("SequenceDiagram\nparticipant A"));
}

#[test]
fn sequence_detector_skips_comments() {
    assert!(sequence::detect(
        "%% comment\nsequenceDiagram\nparticipant A"
    ));
}

#[test]
fn sequence_not_detected_as_flowchart() {
    assert!(!flowchart::detect("sequenceDiagram\nparticipant A"));
}

#[test]
fn sequence_not_detected_as_class() {
    assert!(!class::detect("sequenceDiagram\nparticipant A"));
}
