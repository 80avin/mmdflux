use mmdflux::registry::default_registry;

#[test]
fn default_registry_detects_flowchart() {
    let registry = default_registry();

    // Various flowchart syntax forms
    assert_eq!(registry.detect("graph TD\nA-->B"), Some("flowchart"));
    assert_eq!(registry.detect("graph LR\nA-->B"), Some("flowchart"));
    assert_eq!(registry.detect("flowchart TD\nA-->B"), Some("flowchart"));
    assert_eq!(registry.detect("flowchart\nA-->B"), Some("flowchart"));
}

#[test]
fn default_registry_detects_pie() {
    let registry = default_registry();
    assert_eq!(registry.detect("pie\n\"A\": 50"), Some("pie"));
    assert_eq!(registry.detect("pie title My Pie\n\"A\": 50"), Some("pie"));
}

#[test]
fn default_registry_detects_info() {
    let registry = default_registry();
    assert_eq!(registry.detect("info"), Some("info"));
}

#[test]
fn default_registry_detects_packet() {
    let registry = default_registry();
    assert_eq!(registry.detect("packet-beta"), Some("packet"));
}

#[test]
fn default_registry_detects_class() {
    let registry = default_registry();
    assert_eq!(registry.detect("classDiagram\nclass User"), Some("class"));
}

#[test]
fn default_registry_includes_class_definition() {
    let registry = default_registry();
    assert!(registry.get("class").is_some());
}

#[test]
fn default_registry_class_is_graph_family() {
    let registry = default_registry();
    let def = registry.get("class").unwrap();
    assert_eq!(def.family, mmdflux::diagram::DiagramFamily::Graph);
}

#[test]
fn default_registry_flowchart_first() {
    // Flowchart should be checked before other graph-like patterns
    let registry = default_registry();

    // Even though this has "pie" in it, it starts with "graph"
    assert_eq!(registry.detect("graph TD\npie-->chart"), Some("flowchart"));
}
