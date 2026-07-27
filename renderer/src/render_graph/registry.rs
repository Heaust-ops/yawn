use std::collections::HashMap;

use super::{parse_and_compile, CompiledGraph, GraphError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompiledGraphId {
    pub slot: u32,
    pub generation: u32,
}

impl From<CompiledGraphId> for [u32; 2] {
    fn from(id: CompiledGraphId) -> Self {
        [id.slot, id.generation]
    }
}

#[derive(Debug)]
struct Slot {
    generation: u32,
    value: Option<CompiledGraph>,
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
        let graph = parse_and_compile(bytes)?;
        if self
            .latest_revisions
            .get(&graph.graph_id)
            .is_some_and(|latest| graph.revision <= *latest)
        {
            return Err(GraphError::new(
                "GRAPH_REVISION_CONFLICT",
                "revision must increase",
            ));
        }
        let index = if let Some(index) = self
            .slots
            .iter()
            .position(|slot| slot.value.is_none() && !slot.retired)
        {
            index
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
            slot: u32::try_from(index)
                .map_err(|_| GraphError::new("GRAPH_LIMIT_EXCEEDED", "registry slot overflow"))?,
            generation: self.slots[index].generation,
        };
        let summary = graph.summary(id.into());
        self.latest_revisions
            .insert(graph.graph_id.clone(), graph.revision);
        self.slots[index].value = Some(graph);
        Ok((id, summary))
    }

    pub fn get(&self, id: CompiledGraphId) -> Result<&CompiledGraph, GraphError> {
        self.slots
            .get(id.slot as usize)
            .filter(|slot| slot.generation == id.generation)
            .and_then(|slot| slot.value.as_ref())
            .ok_or_else(|| GraphError::new("STALE_GRAPH_ID", "stale compiled graph id"))
    }

    pub fn contains(&self, id: CompiledGraphId) -> bool {
        self.get(id).is_ok()
    }

    pub fn drop_graph(&mut self, id: CompiledGraphId) -> Result<(), GraphError> {
        let slot = self
            .slots
            .get_mut(id.slot as usize)
            .filter(|slot| slot.generation == id.generation && slot.value.is_some())
            .ok_or_else(|| GraphError::new("STALE_GRAPH_ID", "stale compiled graph id"))?;
        slot.value = None;
        if slot.generation == u32::MAX {
            slot.retired = true
        } else {
            slot.generation += 1
        }
        Ok(())
    }
}
