//! Edge normalization for hierarchical graph layout.
//!
//! This module implements the normalization step of the Sugiyama framework,
//! which breaks long edges (spanning multiple ranks) into chains of short
//! edges (spanning exactly 1 rank each) by inserting dummy nodes.
//!
//! The key benefit is that after normalization, all edges span exactly one
//! rank, which enables:
//! - Proper crossing reduction (dummies participate like real nodes)
//! - Waypoint generation for edge routing
//! - Label placement on isolated edge segments

use super::types::NodeId;

/// The type of dummy node inserted during normalization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DummyType {
    /// Regular dummy node with zero dimensions.
    /// Used to break long edges into single-rank segments.
    Edge,
    /// Dummy node that carries an edge label.
    /// Has non-zero dimensions based on the label text.
    EdgeLabel,
}

/// Label position relative to the edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LabelPos {
    /// Label positioned to the left of the edge.
    Left,
    /// Label centered on the edge.
    #[default]
    Center,
    /// Label positioned to the right of the edge.
    Right,
}

/// Metadata for a dummy node inserted during normalization.
#[derive(Debug, Clone)]
pub struct DummyNode {
    /// The type of this dummy node.
    pub dummy_type: DummyType,
    /// Index of the original edge this dummy belongs to.
    pub edge_index: usize,
    /// The rank (layer) this dummy occupies.
    pub rank: i32,
    /// Width of the dummy (0 for Edge, label width for EdgeLabel).
    pub width: f64,
    /// Height of the dummy (0 for Edge, label height for EdgeLabel).
    pub height: f64,
    /// Label position (only meaningful for EdgeLabel dummies).
    pub label_pos: LabelPos,
}

impl DummyNode {
    /// Create a new regular edge dummy with zero dimensions.
    pub fn edge(edge_index: usize, rank: i32) -> Self {
        Self {
            dummy_type: DummyType::Edge,
            edge_index,
            rank,
            width: 0.0,
            height: 0.0,
            label_pos: LabelPos::default(),
        }
    }

    /// Create a new edge label dummy with the given dimensions.
    pub fn edge_label(
        edge_index: usize,
        rank: i32,
        width: f64,
        height: f64,
        label_pos: LabelPos,
    ) -> Self {
        Self {
            dummy_type: DummyType::EdgeLabel,
            edge_index,
            rank,
            width,
            height,
            label_pos,
        }
    }

    /// Returns true if this is a label-carrying dummy.
    pub fn is_label(&self) -> bool {
        self.dummy_type == DummyType::EdgeLabel
    }
}

/// A chain of dummy nodes representing a normalized long edge.
///
/// The chain starts at the source node and ends at the target node,
/// with dummy nodes at each intermediate rank.
#[derive(Debug, Clone)]
pub struct DummyChain {
    /// Index of the original edge in the input graph.
    pub edge_index: usize,
    /// IDs of the dummy nodes in this chain, in order from source to target.
    /// Does not include the original source/target nodes.
    pub dummy_ids: Vec<NodeId>,
    /// Index of the label dummy within dummy_ids (if any).
    pub label_dummy_index: Option<usize>,
}

impl DummyChain {
    /// Create a new empty dummy chain for an edge.
    pub fn new(edge_index: usize) -> Self {
        Self {
            edge_index,
            dummy_ids: Vec::new(),
            label_dummy_index: None,
        }
    }

    /// Returns true if this chain contains a label dummy.
    pub fn has_label(&self) -> bool {
        self.label_dummy_index.is_some()
    }
}

/// Information about edge label dimensions, used during normalization.
#[derive(Debug, Clone, Default)]
pub struct EdgeLabelInfo {
    /// Width of the label in layout units.
    pub width: f64,
    /// Height of the label in layout units.
    pub height: f64,
    /// Preferred position of the label.
    pub label_pos: LabelPos,
}

impl EdgeLabelInfo {
    /// Create new edge label info with the given dimensions.
    pub fn new(width: f64, height: f64) -> Self {
        Self {
            width,
            height,
            label_pos: LabelPos::default(),
        }
    }

    /// Set the label position.
    pub fn with_pos(mut self, pos: LabelPos) -> Self {
        self.label_pos = pos;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dummy_node_edge() {
        let dummy = DummyNode::edge(0, 2);
        assert_eq!(dummy.dummy_type, DummyType::Edge);
        assert_eq!(dummy.edge_index, 0);
        assert_eq!(dummy.rank, 2);
        assert_eq!(dummy.width, 0.0);
        assert_eq!(dummy.height, 0.0);
        assert!(!dummy.is_label());
    }

    #[test]
    fn test_dummy_node_edge_label() {
        let dummy = DummyNode::edge_label(1, 3, 10.0, 5.0, LabelPos::Center);
        assert_eq!(dummy.dummy_type, DummyType::EdgeLabel);
        assert_eq!(dummy.edge_index, 1);
        assert_eq!(dummy.rank, 3);
        assert_eq!(dummy.width, 10.0);
        assert_eq!(dummy.height, 5.0);
        assert_eq!(dummy.label_pos, LabelPos::Center);
        assert!(dummy.is_label());
    }

    #[test]
    fn test_dummy_chain() {
        let mut chain = DummyChain::new(0);
        assert_eq!(chain.edge_index, 0);
        assert!(chain.dummy_ids.is_empty());
        assert!(!chain.has_label());

        chain.dummy_ids.push(NodeId::from("_d0"));
        chain.dummy_ids.push(NodeId::from("_d1"));
        chain.label_dummy_index = Some(1);

        assert_eq!(chain.dummy_ids.len(), 2);
        assert!(chain.has_label());
    }

    #[test]
    fn test_edge_label_info() {
        let info = EdgeLabelInfo::new(20.0, 10.0).with_pos(LabelPos::Left);
        assert_eq!(info.width, 20.0);
        assert_eq!(info.height, 10.0);
        assert_eq!(info.label_pos, LabelPos::Left);
    }

    #[test]
    fn test_label_pos_default() {
        let pos = LabelPos::default();
        assert_eq!(pos, LabelPos::Center);
    }

    #[test]
    fn test_dummy_chain_multiple_dummies() {
        // Simulate an edge spanning 4 ranks (needs 3 dummies)
        let mut chain = DummyChain::new(5);
        chain.dummy_ids.push(NodeId::from("_d0"));
        chain.dummy_ids.push(NodeId::from("_d1")); // This is the label dummy
        chain.dummy_ids.push(NodeId::from("_d2"));
        chain.label_dummy_index = Some(1);

        assert_eq!(chain.edge_index, 5);
        assert_eq!(chain.dummy_ids.len(), 3);
        assert!(chain.has_label());
        assert_eq!(
            chain.dummy_ids[chain.label_dummy_index.unwrap()],
            NodeId::from("_d1")
        );
    }

    #[test]
    fn test_dummy_type_equality() {
        assert_eq!(DummyType::Edge, DummyType::Edge);
        assert_eq!(DummyType::EdgeLabel, DummyType::EdgeLabel);
        assert_ne!(DummyType::Edge, DummyType::EdgeLabel);
    }

    #[test]
    fn test_edge_label_info_default() {
        let info = EdgeLabelInfo::default();
        assert_eq!(info.width, 0.0);
        assert_eq!(info.height, 0.0);
        assert_eq!(info.label_pos, LabelPos::Center);
    }

    #[test]
    fn test_dummy_node_clone() {
        let dummy = DummyNode::edge_label(2, 5, 15.0, 8.0, LabelPos::Right);
        let cloned = dummy.clone();

        assert_eq!(cloned.dummy_type, DummyType::EdgeLabel);
        assert_eq!(cloned.edge_index, 2);
        assert_eq!(cloned.rank, 5);
        assert_eq!(cloned.width, 15.0);
        assert_eq!(cloned.height, 8.0);
        assert_eq!(cloned.label_pos, LabelPos::Right);
    }
}
