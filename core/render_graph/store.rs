use std::collections::HashMap;

use crate::gpu::Wgpu;
use crate::gpu_resource::GpuResources;
use crate::graph::RenderGraph;
use crate::render_data::RenderData;

pub struct Loadout {
    pub graph: RenderGraph,
    pub resources: GpuResources,
}

#[derive(Default)]
pub struct Store {
    graphs: HashMap<String, RenderGraph>,
    active: Option<Loadout>,
}

impl Store {
    pub fn save(&mut self, graph: RenderGraph) -> String {
        let id = graph.id.clone();
        self.graphs.insert(id.clone(), graph);
        id
    }

    pub fn switch(&mut self, id: &str, gpu: &Wgpu, data: &RenderData) -> Result<(), String> {
        let graph = self.graphs.get(id).ok_or("GRAPH_UNKNOWN")?.clone();
        let resources = GpuResources::activate(&graph, gpu, data)?;
        self.active = Some(Loadout { graph, resources });
        Ok(())
    }

    pub fn active_mut(&mut self) -> Option<&mut Loadout> {
        self.active.as_mut()
    }

    pub fn uses_rows(&self, name: &str) -> bool {
        self.active.as_ref().is_some_and(|active| {
            active
                .graph
                .resources
                .buffers
                .iter()
                .any(|buffer| buffer.array == name)
        })
    }

    pub fn refresh_rows(
        &mut self,
        name: &str,
        gpu: &Wgpu,
        data: &RenderData,
    ) -> Result<(), String> {
        if !self.uses_rows(name) {
            return Ok(());
        }
        let graph = self.active.as_ref().unwrap().graph.clone();
        let resources = GpuResources::activate(&graph, gpu, data)?;
        self.active = Some(Loadout { graph, resources });
        Ok(())
    }
}
