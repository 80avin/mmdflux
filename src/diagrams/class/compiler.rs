//! Compiler from class diagram AST to canonical `graph::Diagram`.
//!
//! Maps classes to graph nodes and relationships to graph edges,
//! preserving class diagram semantics through edge styles and arrows.

use super::parser::ast::{ClassModel, ClassRelationType};
use crate::graph::{Arrow, Diagram, Direction, Edge, Node, Shape, Stroke};

/// Compile a `ClassModel` into a canonical `graph::Diagram`.
///
/// Class diagrams use top-down layout by default. Each class becomes a
/// rectangle node whose label includes member lines (if any). Relationships
/// map to edges with style/arrow metadata based on their type.
pub fn compile(model: &ClassModel) -> Diagram {
    let mut diagram = Diagram::new(Direction::TopDown);

    for class in &model.classes {
        let mut header: Vec<String> = class
            .annotations
            .iter()
            .map(|a| format!("<<{a}>>"))
            .collect();
        header.push(class.name.clone());

        let label = if class.members.is_empty() {
            if class.annotations.is_empty() {
                class.name.clone()
            } else {
                header.join("\n")
            }
        } else {
            // 3-compartment UML: name / attributes / operations.
            // Mermaid heuristic: contains ')' → method, otherwise attribute.
            let (methods, attrs): (Vec<_>, Vec<_>) =
                class.members.iter().partition(|m| m.contains(')'));
            let mut parts = header;
            parts.push(Node::SEPARATOR.to_string());
            parts.extend(attrs.into_iter().cloned());
            parts.push(Node::SEPARATOR.to_string());
            parts.extend(methods.into_iter().cloned());
            parts.join("\n")
        };

        diagram.add_node(
            Node::new(&class.name)
                .with_label(label)
                .with_shape(Shape::Rectangle),
        );
    }

    for rel in &model.relations {
        let (stroke, arrow_start, arrow_end) = relation_style(rel.relation_type);
        // When the operator points left (e.g. `<|--`), the marker belongs on
        // the start (left/from) end instead of the default end position.
        let (arrow_start, arrow_end) = if rel.marker_start {
            (arrow_end, arrow_start)
        } else {
            (arrow_start, arrow_end)
        };
        let mut edge = Edge::new(&rel.from, &rel.to)
            .with_stroke(stroke)
            .with_arrows(arrow_start, arrow_end);

        if let Some(label) = &rel.label {
            edge = edge.with_label(label);
        }

        diagram.add_edge(edge);
    }

    diagram
}

/// Map a class relationship type to edge style.
fn relation_style(rel: ClassRelationType) -> (Stroke, Arrow, Arrow) {
    match rel {
        ClassRelationType::Association => (Stroke::Solid, Arrow::None, Arrow::None),
        ClassRelationType::DirectedAssociation => (Stroke::Solid, Arrow::None, Arrow::Normal),
        ClassRelationType::Inheritance => (Stroke::Solid, Arrow::None, Arrow::OpenTriangle),
        ClassRelationType::Realization => (Stroke::Dotted, Arrow::None, Arrow::OpenTriangle),
        ClassRelationType::Composition => (Stroke::Solid, Arrow::None, Arrow::Diamond),
        ClassRelationType::Aggregation => (Stroke::Solid, Arrow::None, Arrow::OpenDiamond),
        ClassRelationType::Dependency => (Stroke::Dotted, Arrow::None, Arrow::None),
        ClassRelationType::DirectedDependency => (Stroke::Dotted, Arrow::None, Arrow::Normal),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagrams::class::parser::parse_class_diagram;

    fn compile_class(input: &str) -> Diagram {
        let model = parse_class_diagram(input).unwrap();
        compile(&model)
    }

    #[test]
    fn compiler_emits_nodes() {
        let diagram = compile_class("classDiagram\nclass A\nclass B");
        assert!(diagram.nodes.contains_key("A"));
        assert!(diagram.nodes.contains_key("B"));
        assert_eq!(diagram.nodes.len(), 2);
    }

    #[test]
    fn compiler_emits_edges() {
        let diagram = compile_class("classDiagram\nA --> B");
        assert_eq!(diagram.edges.len(), 1);
        assert_eq!(diagram.edges[0].from, "A");
        assert_eq!(diagram.edges[0].to, "B");
    }

    #[test]
    fn compiler_nodes_and_edges() {
        let diagram = compile_class("classDiagram\nclass A\nclass B\nA --> B");
        assert!(diagram.nodes.contains_key("A"));
        assert!(diagram.nodes.contains_key("B"));
        assert_eq!(diagram.edges.len(), 1);
    }

    #[test]
    fn compiler_default_direction_is_top_down() {
        let diagram = compile_class("classDiagram\nclass A");
        assert_eq!(diagram.direction, Direction::TopDown);
    }

    #[test]
    fn compiler_node_shape_is_rectangle() {
        let diagram = compile_class("classDiagram\nclass User");
        assert_eq!(diagram.nodes["User"].shape, Shape::Rectangle);
    }

    #[test]
    fn compiler_inheritance_edge_style() {
        let diagram = compile_class("classDiagram\nAnimal <|-- Dog");
        assert_eq!(diagram.edges[0].stroke, Stroke::Solid);
    }

    #[test]
    fn compiler_realization_edge_is_dotted() {
        let diagram = compile_class("classDiagram\nLogger <|.. ConsoleLogger");
        assert_eq!(diagram.edges[0].stroke, Stroke::Dotted);
        assert_eq!(diagram.edges[0].arrow_start, Arrow::OpenTriangle);
    }

    #[test]
    fn compiler_dependency_edge_is_dotted() {
        let diagram = compile_class("classDiagram\nA ..> B");
        assert_eq!(diagram.edges[0].stroke, Stroke::Dotted);
    }

    #[test]
    fn compiler_edge_label_preserved() {
        let diagram = compile_class("classDiagram\nA --> B : uses");
        assert_eq!(diagram.edges[0].label, Some("uses".to_string()));
    }

    #[test]
    fn compiler_class_with_members_has_three_compartments() {
        let input = "classDiagram\nclass User {\n  +String name\n  +String email\n  +login()\n  +logout()\n}";
        let diagram = compile_class(input);
        let label = &diagram.nodes["User"].label;
        let lines: Vec<&str> = label.lines().collect();
        // name / separator / attrs... / separator / methods...
        assert_eq!(lines[0], "User");
        assert_eq!(lines[1], Node::SEPARATOR);
        assert_eq!(lines[2], "+String name");
        assert_eq!(lines[3], "+String email");
        assert_eq!(lines[4], Node::SEPARATOR);
        assert_eq!(lines[5], "+login()");
        assert_eq!(lines[6], "+logout()");
    }

    #[test]
    fn compiler_annotation_is_rendered_above_class_name() {
        let input = "classDiagram\nclass Logger {\n  <<interface>>\n  +log(message)\n}";
        let diagram = compile_class(input);
        let label = &diagram.nodes["Logger"].label;
        let lines: Vec<&str> = label.lines().collect();
        // annotation + name share top compartment, then attrs/methods sections
        assert_eq!(lines[0], "<<interface>>");
        assert_eq!(lines[1], "Logger");
        assert_eq!(lines[2], Node::SEPARATOR);
        // empty attrs compartment
        assert_eq!(lines[3], Node::SEPARATOR);
        assert_eq!(lines[4], "+log(message)");
    }

    #[test]
    fn compiler_annotation_without_members_preserves_header() {
        let input = "classDiagram\nclass Logger <<interface>>";
        let diagram = compile_class(input);
        let label = &diagram.nodes["Logger"].label;
        let lines: Vec<&str> = label.lines().collect();
        assert_eq!(lines, vec!["<<interface>>", "Logger"]);
    }

    #[test]
    fn compiler_methods_only_has_empty_attrs_compartment() {
        let input = "classDiagram\nclass Foo {\n  +doStuff()\n}";
        let diagram = compile_class(input);
        let label = &diagram.nodes["Foo"].label;
        let lines: Vec<&str> = label.lines().collect();
        assert_eq!(lines[0], "Foo");
        assert_eq!(lines[1], Node::SEPARATOR);
        // empty attrs compartment
        assert_eq!(lines[2], Node::SEPARATOR);
        assert_eq!(lines[3], "+doStuff()");
    }

    #[test]
    fn compiler_attrs_only_has_empty_methods_compartment() {
        let input = "classDiagram\nclass Foo {\n  +String name\n}";
        let diagram = compile_class(input);
        let label = &diagram.nodes["Foo"].label;
        let lines: Vec<&str> = label.lines().collect();
        assert_eq!(lines[0], "Foo");
        assert_eq!(lines[1], Node::SEPARATOR);
        assert_eq!(lines[2], "+String name");
        assert_eq!(lines[3], Node::SEPARATOR);
        // empty methods compartment
        assert_eq!(lines.len(), 4);
    }

    #[test]
    fn compiler_implicit_classes_from_relations() {
        let diagram = compile_class("classDiagram\nA --> B");
        assert_eq!(diagram.nodes.len(), 2);
    }

    #[test]
    fn compiler_edge_indices_sequential() {
        let diagram = compile_class("classDiagram\nA --> B\nB --> C\nC --> A");
        assert_eq!(diagram.edges[0].index, 0);
        assert_eq!(diagram.edges[1].index, 1);
        assert_eq!(diagram.edges[2].index, 2);
    }
}
