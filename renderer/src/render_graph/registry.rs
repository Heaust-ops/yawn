use super::{parse_and_compile_any, CompiledGraph, CompiledGraphV2, GraphError};
use std::collections::HashMap;
#[derive(Debug, Clone)]
pub enum RegisteredGraph {
    V1(CompiledGraph),
    V2(CompiledGraphV2),
}
impl RegisteredGraph {
    fn identity(&self) -> (&str, u32) {
        match self {
            Self::V1(g) => (g.graph_id.as_str(), g.revision),
            Self::V2(g) => (g.graph_id.as_str(), g.revision),
        }
    }
    fn summary(&self, id: [u32; 2]) -> serde_json::Value {
        match self {
            Self::V1(g) => g.summary(id),
            Self::V2(g) => g.summary(id),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompiledGraphId {
    pub slot: u32,
    pub generation: u32,
}
impl From<CompiledGraphId> for [u32; 2] {
    fn from(x: CompiledGraphId) -> Self {
        [x.slot, x.generation]
    }
}
#[derive(Debug)]
struct Slot {
    generation: u32,
    value: Option<RegisteredGraph>,
    retired: bool,
}
#[derive(Debug)]
pub struct Registry {
    slots: Vec<Slot>,
    capacity: u32,
    latest_revisions: HashMap<String, u32>,
}
impl Default for Registry {
    fn default() -> Self {
        Self::new(16)
    }
}
impl Registry {
    pub fn new(capacity: u32) -> Self {
        Self {
            slots: vec![],
            capacity,
            latest_revisions: HashMap::new(),
        }
    }
    pub fn compile(
        &mut self,
        bytes: &[u8],
    ) -> Result<(CompiledGraphId, serde_json::Value), GraphError> {
        let graph = parse_and_compile_any(bytes)?;
        let (graph_id, revision) = graph.identity();
        if self
            .latest_revisions
            .get(graph_id)
            .is_some_and(|latest| revision <= *latest)
        {
            return Err(GraphError::new(
                "GRAPH_REVISION_CONFLICT",
                "revision must increase",
            ));
        }
        let i = if let Some(i) = self
            .slots
            .iter()
            .position(|s| s.value.is_none() && !s.retired)
        {
            i
        } else {
            if u32::try_from(self.slots.len()).map_or(true, |len| len >= self.capacity) {
                return Err(GraphError::new(
                    "GRAPH_REGISTRY_FULL",
                    "compiled graph registry is full",
                ));
            }
            self.slots.push(Slot {
                generation: 1,
                value: None,
                retired: false,
            });
            self.slots.len() - 1
        };
        let id = CompiledGraphId {
            slot: u32::try_from(i)
                .map_err(|_| GraphError::new("GRAPH_LIMIT_EXCEEDED", "registry slot overflow"))?,
            generation: self.slots[i].generation,
        };
        let summary = graph.summary(id.into());
        self.latest_revisions.insert(graph_id.to_owned(), revision);
        self.slots[i].value = Some(graph);
        Ok((id, summary))
    }
    pub fn get(&self, id: CompiledGraphId) -> Result<&CompiledGraph, GraphError> {
        match self
            .slots
            .get(id.slot as usize)
            .filter(|s| s.generation == id.generation)
            .and_then(|s| s.value.as_ref())
            .ok_or_else(|| GraphError::new("STALE_GRAPH_ID", "stale compiled graph id"))?
        {
            RegisteredGraph::V1(g) => Ok(g),
            RegisteredGraph::V2(_) => Err(GraphError::new(
                "GRAPH_EXECUTION_UNSUPPORTED",
                "schemaVersion 2 activation is unavailable until Phase 4",
            )),
        }
    }
    pub fn get_registered(&self, id: CompiledGraphId) -> Result<&RegisteredGraph, GraphError> {
        self.slots
            .get(id.slot as usize)
            .filter(|s| s.generation == id.generation)
            .and_then(|s| s.value.as_ref())
            .ok_or_else(|| GraphError::new("STALE_GRAPH_ID", "stale compiled graph id"))
    }
    pub fn contains(&self, id: CompiledGraphId) -> bool {
        self.get_registered(id).is_ok()
    }
    pub fn drop_graph(&mut self, id: CompiledGraphId) -> Result<(), GraphError> {
        let s = self
            .slots
            .get_mut(id.slot as usize)
            .filter(|s| s.generation == id.generation && s.value.is_some())
            .ok_or_else(|| GraphError::new("STALE_GRAPH_ID", "stale compiled graph id"))?;
        s.value = None;
        if s.generation == u32::MAX {
            s.retired = true
        } else {
            s.generation += 1
        }
        Ok(())
    }
}
