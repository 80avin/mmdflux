//! AST types for class diagrams.

/// Parsed class diagram model.
#[derive(Debug, Clone)]
pub struct ClassModel {
    /// Class declarations (in order of appearance).
    pub classes: Vec<ClassDecl>,
    /// Relationships between classes.
    pub relations: Vec<ClassRelation>,
}

/// A class declaration.
#[derive(Debug, Clone)]
pub struct ClassDecl {
    /// Class name/identifier.
    pub name: String,
    /// Body members (fields and methods), if any.
    pub members: Vec<String>,
}

/// A relationship between two classes.
#[derive(Debug, Clone)]
pub struct ClassRelation {
    /// Left-hand class name (always first in declaration order).
    pub from: String,
    /// Right-hand class name (always second in declaration order).
    pub to: String,
    /// Relationship type.
    pub relation_type: ClassRelationType,
    /// Optional label on the relationship.
    pub label: Option<String>,
    /// When true, the relationship marker belongs on the `from` (left) end
    /// rather than the `to` (right) end. Set for left-pointing operators
    /// like `<|--`, `*--`, `o--`.
    pub marker_start: bool,
}

/// Types of class relationships (MVP scope).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassRelationType {
    /// `--`  Plain association (no arrow).
    Association,
    /// `-->`  Directed association (with arrow).
    DirectedAssociation,
    /// `<|--`  Inheritance/generalization.
    Inheritance,
    /// `*--`  Composition.
    Composition,
    /// `o--`  Aggregation.
    Aggregation,
    /// `..`  Dependency (no arrow).
    Dependency,
    /// `..>`  Directed dependency (with arrow).
    DirectedDependency,
}
