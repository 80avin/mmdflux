//! Graph-family engine registry.
//!
//! Maps engine IDs to concrete engine adapters for the graph family.
//! All graph-family engines operate on `Diagram` → `GraphGeometry`.
//!
//! Two lookup paths coexist during transition (removed in Phase 5):
//! - Legacy: `get(LayoutEngineId)` → `&dyn GraphLayoutEngine`
//! - New: `get_solver(EngineAlgorithmId)` → `&dyn GraphEngine`

use std::collections::HashMap;

use crate::diagram::{EngineAlgorithmId, GraphEngine, GraphLayoutEngine, LayoutEngineId};
use crate::diagrams::flowchart::engine::{
    DagreLayoutEngine, FluxLayeredEngine, MermaidLayeredEngine,
};
use crate::diagrams::flowchart::geometry::GraphGeometry;
use crate::graph::Diagram;

/// Concrete trait object type for legacy graph-family engines.
type BoxedGraphEngine = Box<dyn GraphLayoutEngine<Input = Diagram, Output = GraphGeometry>>;

/// Concrete trait object type for new graph solvers.
type BoxedGraphSolver = Box<dyn GraphEngine>;

/// Registry of graph-family layout engines.
///
/// Maintains two maps during Phase 3–4 transition:
/// - `engines`: legacy `LayoutEngineId` → `GraphLayoutEngine` (removed in Phase 5)
/// - `solvers`: new `EngineAlgorithmId` → `GraphEngine`
pub struct GraphEngineRegistry {
    engines: HashMap<LayoutEngineId, BoxedGraphEngine>,
    solvers: HashMap<EngineAlgorithmId, BoxedGraphSolver>,
}

impl GraphEngineRegistry {
    /// Look up a legacy engine by `LayoutEngineId`.
    pub fn get(
        &self,
        id: LayoutEngineId,
    ) -> Option<&dyn GraphLayoutEngine<Input = Diagram, Output = GraphGeometry>> {
        self.engines.get(&id).map(|e| e.as_ref())
    }

    /// Check whether a legacy engine is registered and available.
    pub fn is_available(&self, id: LayoutEngineId) -> bool {
        self.engines.contains_key(&id)
    }

    /// Register a legacy engine adapter.
    pub fn register(&mut self, id: LayoutEngineId, engine: BoxedGraphEngine) {
        self.engines.insert(id, engine);
    }

    /// Look up a solver by combined `EngineAlgorithmId`.
    pub fn get_solver(&self, id: EngineAlgorithmId) -> Option<&dyn GraphEngine> {
        self.solvers.get(&id).map(|e| e.as_ref())
    }

    /// Register a solver by combined `EngineAlgorithmId`.
    pub fn register_solver(&mut self, id: EngineAlgorithmId, engine: BoxedGraphSolver) {
        self.solvers.insert(id, engine);
    }
}

impl Default for GraphEngineRegistry {
    fn default() -> Self {
        use crate::diagram::{AlgorithmId, EngineId};

        let mut registry = Self {
            engines: HashMap::new(),
            solvers: HashMap::new(),
        };

        // Legacy registrations (kept until Phase 5).
        registry.register(LayoutEngineId::Dagre, Box::new(DagreLayoutEngine::text()));

        // New combined-ID solver registrations.
        registry.register_solver(
            EngineAlgorithmId::new(EngineId::Flux, AlgorithmId::Layered),
            Box::new(FluxLayeredEngine::text()),
        );
        registry.register_solver(
            EngineAlgorithmId::new(EngineId::Mermaid, AlgorithmId::Layered),
            Box::new(MermaidLayeredEngine::text()),
        );

        #[cfg(feature = "engine-elk")]
        {
            use super::elk::ElkLayoutEngine;
            registry.register(LayoutEngineId::Elk, Box::new(ElkLayoutEngine));
            // TODO: ELK solver adapters for elk-layered and elk-mrtree (Phase 3 follow-up)
        }

        registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagram::{AlgorithmId, EngineId};

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

    #[test]
    fn default_registry_has_flux_layered_solver() {
        let registry = GraphEngineRegistry::default();
        let id = EngineAlgorithmId::new(EngineId::Flux, AlgorithmId::Layered);
        let solver = registry.get_solver(id);
        assert!(solver.is_some());
        assert_eq!(solver.unwrap().id().to_string(), "flux-layered");
    }

    #[test]
    fn default_registry_has_mermaid_layered_solver() {
        let registry = GraphEngineRegistry::default();
        let id = EngineAlgorithmId::new(EngineId::Mermaid, AlgorithmId::Layered);
        let solver = registry.get_solver(id);
        assert!(solver.is_some());
        assert_eq!(solver.unwrap().id().to_string(), "mermaid-layered");
    }
}
