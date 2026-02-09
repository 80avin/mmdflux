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
        let label = if class.members.is_empty() {
            class.name.clone()
        } else {
            // Include members in the label, separated by newlines.
            // A separator marker between name and members renders as a horizontal rule.
            let mut parts = vec![class.name.clone(), Node::SEPARATOR.to_string()];
            parts.extend(class.members.iter().cloned());
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
        ClassRelationType::Association => (Stroke::Solid, Arrow::None, Arrow::Normal),
        ClassRelationType::Inheritance => (Stroke::Solid, Arrow::None, Arrow::Normal),
        ClassRelationType::Composition => (Stroke::Solid, Arrow::None, Arrow::Normal),
        ClassRelationType::Aggregation => (Stroke::Solid, Arrow::None, Arrow::Normal),
        ClassRelationType::Dependency => (Stroke::Dotted, Arrow::None, Arrow::Normal),
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
    fn compiler_class_with_members_label() {
        let input = "classDiagram\nclass User {\n  +String name\n  +login()\n}";
        let diagram = compile_class(input);
        let label = &diagram.nodes["User"].label;
        assert!(label.contains("User"));
        assert!(label.contains("+String name"));
        assert!(label.contains("+login()"));
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
