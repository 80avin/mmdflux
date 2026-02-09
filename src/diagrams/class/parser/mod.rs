//! Class diagram parser.
//!
//! Hand-written line-oriented parser for Mermaid class diagram syntax.
//! Supports MVP scope: class declarations (with optional body), and
//! relationships (association, inheritance, composition, aggregation, dependency).

pub mod ast;

use ast::{ClassDecl, ClassModel, ClassRelation, ClassRelationType};

/// Ensure a class exists in the classes list, merging members if it already does.
///
/// Uses `class_index` to track position of each class in `classes` for O(1) lookup.
/// If the class already exists, new members are appended. If not, a new entry is created.
fn ensure_class(
    classes: &mut Vec<ClassDecl>,
    class_index: &mut std::collections::HashMap<String, usize>,
    name: String,
    members: Vec<String>,
) {
    if let Some(&idx) = class_index.get(&name) {
        classes[idx].members.extend(members);
    } else {
        class_index.insert(name.clone(), classes.len());
        classes.push(ClassDecl { name, members });
    }
}

/// Parse a class diagram from Mermaid input text.
///
/// Expects the input to start with `classDiagram` (case-insensitive).
pub fn parse_class_diagram(
    input: &str,
) -> Result<ClassModel, Box<dyn std::error::Error + Send + Sync>> {
    let mut classes: Vec<ClassDecl> = Vec::new();
    let mut relations: Vec<ClassRelation> = Vec::new();
    let mut class_index: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    let mut lines = input.lines().peekable();

    // Skip frontmatter
    if let Some(first) = lines.peek()
        && first.trim() == "---"
    {
        lines.next();
        for line in lines.by_ref() {
            if line.trim() == "---" {
                break;
            }
        }
    }

    // Skip leading comments and whitespace, then consume header
    let mut found_header = false;
    while let Some(line) = lines.peek() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("%%") {
            lines.next();
            continue;
        }
        if trimmed.to_lowercase().starts_with("classdiagram") {
            found_header = true;
            lines.next();
            break;
        }
        return Err(format!("Expected 'classDiagram' header, got: {trimmed}").into());
    }

    if !found_header {
        return Err("Missing 'classDiagram' header".into());
    }

    // Parse body lines
    let mut in_class_body: Option<String> = None;
    let mut current_members: Vec<String> = Vec::new();

    for line in lines {
        let trimmed = line.trim();

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with("%%") {
            continue;
        }

        // Handle class body close
        if trimmed == "}" {
            if let Some(class_name) = in_class_body.take() {
                ensure_class(
                    &mut classes,
                    &mut class_index,
                    class_name,
                    std::mem::take(&mut current_members),
                );
            }
            continue;
        }

        // Inside a class body — collect members
        if in_class_body.is_some() {
            current_members.push(trimmed.to_string());
            continue;
        }

        // Try: `class ClassName {`  (body start)
        if let Some(rest) = strip_keyword(trimmed, "class") {
            let rest = rest.trim();
            if let Some(name) = rest.strip_suffix('{') {
                let name = name.trim().to_string();
                in_class_body = Some(name);
                current_members.clear();
                continue;
            }

            // Try: `class ClassName`  (bare declaration)
            // May have optional annotation like `class ClassName~T~`
            let name = parse_class_name(rest);
            if !name.is_empty() {
                ensure_class(&mut classes, &mut class_index, name, Vec::new());
            }
            continue;
        }

        // Try: relationship line
        if let Some(rel) = try_parse_relation(trimmed) {
            // Ensure both sides are tracked as classes
            for name in [&rel.from, &rel.to] {
                ensure_class(&mut classes, &mut class_index, name.clone(), Vec::new());
            }
            relations.push(rel);
            continue;
        }

        // Try: `ClassName : member` or `ClassName: member` (inline member)
        if let Some(colon_pos) = trimmed.find(':') {
            let left = trimmed[..colon_pos].trim();
            let member = trimmed[colon_pos + 1..].trim();
            if !left.is_empty() && !member.is_empty() && parse_class_name(left) == left {
                ensure_class(
                    &mut classes,
                    &mut class_index,
                    left.to_string(),
                    vec![member.to_string()],
                );
                continue;
            }
        }

        // Permissive: skip unrecognized lines (style, note, click, etc.)
    }

    // Handle unclosed class body
    if let Some(class_name) = in_class_body.take() {
        ensure_class(
            &mut classes,
            &mut class_index,
            class_name,
            std::mem::take(&mut current_members),
        );
    }

    Ok(ClassModel { classes, relations })
}

/// Strip a case-insensitive keyword prefix followed by whitespace.
fn strip_keyword<'a>(line: &'a str, keyword: &str) -> Option<&'a str> {
    let lower = line.to_lowercase();
    if lower.starts_with(keyword) {
        let rest = &line[keyword.len()..];
        if rest.is_empty() || rest.starts_with(char::is_whitespace) {
            return Some(rest.trim_start());
        }
    }
    None
}

/// Parse a class name, stripping optional generic annotations like `~T~`.
fn parse_class_name(s: &str) -> String {
    // Take identifier chars (alphanumeric, underscore)
    let name: String = s
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    name
}

/// Relationship patterns to try, ordered by specificity (longest first).
///
/// Mermaid class diagram relationship syntax:
///   `A <|-- B`     inheritance
///   `A *-- B`      composition
///   `A o-- B`      aggregation
///   `A <.. B`      dependency (reverse)
///   `A ..> B`      dependency
///   `A .. B`       dependency (no arrow)
///   `A --> B`      association (directed)
///   `A -- B`       association (undirected)
///
/// Optional labels: `A --> B : label text`
fn try_parse_relation(line: &str) -> Option<ClassRelation> {
    // Patterns ordered by specificity
    static PATTERNS: &[(&str, ClassRelationType, bool)] = &[
        // 4-char operators
        ("<|--", ClassRelationType::Inheritance, true), // reversed: B inherits from A
        ("--|>", ClassRelationType::Inheritance, false), // A inherits from B
        ("<|..", ClassRelationType::Inheritance, true), // dotted inheritance (reversed)
        ("..|>", ClassRelationType::Inheritance, false), // dotted inheritance
        ("*--", ClassRelationType::Composition, true),  // reversed
        ("--*", ClassRelationType::Composition, false),
        ("o--", ClassRelationType::Aggregation, true), // reversed
        ("--o", ClassRelationType::Aggregation, false),
        // 3-char operators
        ("-->", ClassRelationType::DirectedAssociation, false),
        ("<--", ClassRelationType::DirectedAssociation, true),
        ("..>", ClassRelationType::DirectedDependency, false),
        ("<..", ClassRelationType::DirectedDependency, true),
        // 2-char operators (must be last)
        ("--", ClassRelationType::Association, false),
        ("..", ClassRelationType::Dependency, false),
    ];

    for &(pattern, rel_type, reversed) in PATTERNS {
        if let Some(pos) = line.find(pattern) {
            let left = line[..pos].trim();
            let right_with_label = line[pos + pattern.len()..].trim();

            // Left side must be a valid class name
            let from_name = parse_class_name(left);
            if from_name.is_empty() || from_name.len() != left.len() {
                continue;
            }

            // Right side: "ClassName" or "ClassName : label"
            let (to_raw, label) = if let Some(colon_pos) = right_with_label.find(" : ") {
                (
                    &right_with_label[..colon_pos],
                    Some(right_with_label[colon_pos + 3..].trim().to_string()),
                )
            } else {
                (right_with_label, None)
            };

            let to_name = parse_class_name(to_raw.trim());
            if to_name.is_empty() {
                continue;
            }

            return Some(ClassRelation {
                from: from_name,
                to: to_name,
                relation_type: rel_type,
                label,
                marker_start: reversed,
            });
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_class_diagram() {
        let model = parse_class_diagram("classDiagram\n").unwrap();
        assert!(model.classes.is_empty());
        assert!(model.relations.is_empty());
    }

    #[test]
    fn parse_single_class() {
        let model = parse_class_diagram("classDiagram\nclass User").unwrap();
        assert_eq!(model.classes.len(), 1);
        assert_eq!(model.classes[0].name, "User");
    }

    #[test]
    fn parse_multiple_classes() {
        let input = "classDiagram\nclass User\nclass Order\nclass Product";
        let model = parse_class_diagram(input).unwrap();
        assert_eq!(model.classes.len(), 3);
        assert_eq!(model.classes[0].name, "User");
        assert_eq!(model.classes[1].name, "Order");
        assert_eq!(model.classes[2].name, "Product");
    }

    #[test]
    fn parse_class_with_body() {
        let input = "classDiagram\nclass User {\n  +String name\n  +login()\n}";
        let model = parse_class_diagram(input).unwrap();
        assert_eq!(model.classes.len(), 1);
        assert_eq!(model.classes[0].name, "User");
        assert_eq!(model.classes[0].members.len(), 2);
        assert_eq!(model.classes[0].members[0], "+String name");
        assert_eq!(model.classes[0].members[1], "+login()");
    }

    #[test]
    fn parse_inheritance_relation() {
        let input = "classDiagram\nAnimal <|-- Dog";
        let model = parse_class_diagram(input).unwrap();
        assert_eq!(model.relations.len(), 1);
        assert_eq!(model.relations[0].from, "Animal");
        assert_eq!(model.relations[0].to, "Dog");
        assert!(model.relations[0].marker_start);
        assert_eq!(
            model.relations[0].relation_type,
            ClassRelationType::Inheritance
        );
    }

    #[test]
    fn parse_composition_relation() {
        let input = "classDiagram\nCar *-- Engine";
        let model = parse_class_diagram(input).unwrap();
        assert_eq!(model.relations.len(), 1);
        assert_eq!(
            model.relations[0].relation_type,
            ClassRelationType::Composition
        );
    }

    #[test]
    fn parse_aggregation_relation() {
        let input = "classDiagram\nLibrary o-- Book";
        let model = parse_class_diagram(input).unwrap();
        assert_eq!(model.relations.len(), 1);
        assert_eq!(
            model.relations[0].relation_type,
            ClassRelationType::Aggregation
        );
    }

    #[test]
    fn parse_dependency_relation() {
        let input = "classDiagram\nService ..> Repository";
        let model = parse_class_diagram(input).unwrap();
        assert_eq!(model.relations.len(), 1);
        assert_eq!(model.relations[0].from, "Service");
        assert_eq!(model.relations[0].to, "Repository");
        assert_eq!(
            model.relations[0].relation_type,
            ClassRelationType::DirectedDependency
        );
    }

    #[test]
    fn parse_association_directed() {
        let input = "classDiagram\nA --> B";
        let model = parse_class_diagram(input).unwrap();
        assert_eq!(model.relations.len(), 1);
        assert_eq!(
            model.relations[0].relation_type,
            ClassRelationType::DirectedAssociation
        );
    }

    #[test]
    fn parse_association_undirected() {
        let input = "classDiagram\nA -- B";
        let model = parse_class_diagram(input).unwrap();
        assert_eq!(model.relations.len(), 1);
        assert_eq!(
            model.relations[0].relation_type,
            ClassRelationType::Association
        );
    }

    #[test]
    fn parse_relation_with_label() {
        let input = "classDiagram\nA --> B : uses";
        let model = parse_class_diagram(input).unwrap();
        assert_eq!(model.relations[0].label, Some("uses".to_string()));
    }

    #[test]
    fn parse_relation_creates_implicit_classes() {
        let input = "classDiagram\nA --> B";
        let model = parse_class_diagram(input).unwrap();
        assert_eq!(model.classes.len(), 2);
    }

    #[test]
    fn parse_class_declared_and_in_relation() {
        let input = "classDiagram\nclass A\nA --> B";
        let model = parse_class_diagram(input).unwrap();
        // A declared explicitly, B implicitly via relation
        assert_eq!(model.classes.len(), 2);
    }

    #[test]
    fn parse_skips_comments() {
        let input = "classDiagram\n%% comment\nclass User";
        let model = parse_class_diagram(input).unwrap();
        assert_eq!(model.classes.len(), 1);
    }

    #[test]
    fn parse_missing_header_errors() {
        let result = parse_class_diagram("class User\nA --> B");
        assert!(result.is_err());
    }

    #[test]
    fn parse_case_insensitive_header() {
        let model = parse_class_diagram("CLASSDIAGRAM\nclass User").unwrap();
        assert_eq!(model.classes.len(), 1);
    }

    #[test]
    fn parse_class_with_generic_annotation() {
        let input = "classDiagram\nclass List~T~";
        let model = parse_class_diagram(input).unwrap();
        assert_eq!(model.classes[0].name, "List");
    }

    #[test]
    fn parse_inline_member_with_space() {
        let input = "classDiagram\nAnimal : +int age";
        let model = parse_class_diagram(input).unwrap();
        assert_eq!(model.classes.len(), 1);
        assert_eq!(model.classes[0].name, "Animal");
        assert_eq!(model.classes[0].members, vec!["+int age"]);
    }

    #[test]
    fn parse_inline_member_without_space() {
        let input = "classDiagram\nAnimal: +isMammal()";
        let model = parse_class_diagram(input).unwrap();
        assert_eq!(model.classes[0].members, vec!["+isMammal()"]);
    }

    #[test]
    fn parse_multiple_inline_members() {
        let input = "classDiagram\nAnimal : +int age\nAnimal : +String gender\nAnimal: +mate()";
        let model = parse_class_diagram(input).unwrap();
        assert_eq!(model.classes.len(), 1);
        assert_eq!(model.classes[0].members.len(), 3);
    }

    #[test]
    fn parse_relation_before_class_body_preserves_members() {
        let input = "classDiagram\nAnimal <|-- Dog\nclass Dog {\n  +bark()\n}";
        let model = parse_class_diagram(input).unwrap();
        let dog = model.classes.iter().find(|c| c.name == "Dog").unwrap();
        assert_eq!(dog.members, vec!["+bark()"]);
    }

    #[test]
    fn parse_inline_members_with_relations() {
        let input = "\
classDiagram
    Animal <|-- Duck
    Animal : +int age
    class Duck{
      +swim()
    }";
        let model = parse_class_diagram(input).unwrap();
        let animal = model.classes.iter().find(|c| c.name == "Animal").unwrap();
        assert_eq!(animal.members, vec!["+int age"]);
        let duck = model.classes.iter().find(|c| c.name == "Duck").unwrap();
        assert_eq!(duck.members, vec!["+swim()"]);
    }

    #[test]
    fn parse_full_example() {
        let input = "\
classDiagram
    class Animal {
        +String name
        +makeSound()
    }
    class Dog
    Animal <|-- Dog
    Dog --> Bone : chews";
        let model = parse_class_diagram(input).unwrap();
        assert_eq!(model.classes.len(), 3); // Animal, Dog, Bone
        assert_eq!(model.relations.len(), 2);
        assert_eq!(model.classes[0].name, "Animal");
        assert_eq!(model.classes[0].members.len(), 2);
    }
}
