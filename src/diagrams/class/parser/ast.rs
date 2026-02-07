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
    /// Source class name.
    pub from: String,
    /// Target class name.
    pub to: String,
    /// Relationship type.
    pub relation_type: ClassRelationType,
    /// Optional label on the relationship.
    pub label: Option<String>,
}

/// Types of class relationships (MVP scope).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassRelationType {
    /// `--` or `-->`  Plain association.
    Association,
    /// `<|--`  Inheritance/generalization.
    Inheritance,
    /// `*--`  Composition.
    Composition,
    /// `o--`  Aggregation.
    Aggregation,
    /// `..>` or `..`  Dependency/realization.
    Dependency,
}
