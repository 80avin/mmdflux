//! Graph-family engine registry.
//!
//! Maps `LayoutEngineId` to concrete engine adapters for the graph family.
//! All graph-family engines operate on `Diagram` → `GraphGeometry`.

use std::collections::HashMap;

use crate::diagram::{GraphLayoutEngine, LayoutEngineId};
use crate::diagrams::flowchart::engine::DagreLayoutEngine;
use crate::diagrams::flowchart::geometry::GraphGeometry;
use crate::graph::Diagram;

/// Concrete trait object type for graph-family engines.
type BoxedGraphEngine = Box<dyn GraphLayoutEngine<Input = Diagram, Output = GraphGeometry>>;

/// Registry of graph-family layout engines.
///
/// Engines are stored by `LayoutEngineId` and looked up at runtime.
/// The default registry includes dagre; additional engines are registered
/// conditionally based on feature flags.
pub struct GraphEngineRegistry {
    engines: HashMap<LayoutEngineId, BoxedGraphEngine>,
}

impl GraphEngineRegistry {
    /// Look up an engine by ID.
    pub fn get(
        &self,
        id: LayoutEngineId,
    ) -> Option<&dyn GraphLayoutEngine<Input = Diagram, Output = GraphGeometry>> {
        self.engines.get(&id).map(|e| e.as_ref())
    }

    /// Check whether an engine is registered and available.
    pub fn is_available(&self, id: LayoutEngineId) -> bool {
        self.engines.contains_key(&id)
    }

    /// Register an engine adapter.
    pub fn register(&mut self, id: LayoutEngineId, engine: BoxedGraphEngine) {
        self.engines.insert(id, engine);
    }
}

impl Default for GraphEngineRegistry {
    fn default() -> Self {
        let mut registry = Self {
            engines: HashMap::new(),
        };
        registry.register(LayoutEngineId::Dagre, Box::new(DagreLayoutEngine::text()));

        #[cfg(feature = "engine-elk")]
        {
            use super::elk::ElkLayoutEngine;
            registry.register(LayoutEngineId::Elk, Box::new(ElkLayoutEngine));
        }

        registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_registry_has_dagre() {
        let registry = GraphEngineRegistry::default();
        assert!(registry.is_available(LayoutEngineId::Dagre));
    }

    #[test]
    fn dagre_engine_name_from_registry() {
        let registry = GraphEngineRegistry::default();
        let engine = registry.get(LayoutEngineId::Dagre).unwrap();
        assert_eq!(engine.name(), "dagre");
    }

    #[test]
    fn unregistered_engine_returns_none() {
        let registry = GraphEngineRegistry::default();
        assert!(registry.get(LayoutEngineId::Cose).is_none());
    }
}
