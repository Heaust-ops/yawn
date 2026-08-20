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
    texture_sources: HashMap<String, HashMap<u32, web_sys::ImageBitmap>>,
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
        let mut resources = GpuResources::activate(
            &graph,
            gpu,
            data,
            self.active.as_ref().map(|active| &active.resources),
        )?;
        for (name, levels) in &self.texture_sources {
            for (mip_level, image) in levels {
                if resources.needs_upload(name, *mip_level) {
                    resources.upload_texture(name, *mip_level, image, gpu)?;
                }
            }
        }
        self.active = Some(Loadout { graph, resources });
        Ok(())
    }

    pub fn upload_texture(
        &mut self,
        name: String,
        mip_level: u32,
        image: web_sys::ImageBitmap,
        gpu: &Wgpu,
    ) -> Result<(), String> {
        if let Some(active) = &mut self.active {
            if active.resources.texture_slots.contains_key(&name) {
                active
                    .resources
                    .upload_texture(&name, mip_level, &image, gpu)?;
            }
        }
        let levels = self.texture_sources.entry(name).or_default();
        if let Some(previous) = levels.insert(mip_level, image) {
            previous.close();
        }
        Ok(())
    }

    pub fn delete_texture(&mut self, name: &str) {
        if let Some(levels) = self.texture_sources.remove(name) {
            for image in levels.into_values() {
                image.close();
            }
        }
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
        self.refresh(gpu, data)
    }

    pub fn refresh(&mut self, gpu: &Wgpu, data: &RenderData) -> Result<(), String> {
        let Some(active) = self.active.as_ref() else {
            return Ok(());
        };
        let graph = active.graph.clone();
        let mut resources = GpuResources::activate(
            &graph,
            gpu,
            data,
            self.active.as_ref().map(|active| &active.resources),
        )?;
        for (name, levels) in &self.texture_sources {
            for (mip_level, image) in levels {
                if resources.needs_upload(name, *mip_level) {
                    resources.upload_texture(name, *mip_level, image, gpu)?;
                }
            }
        }
        self.active = Some(Loadout { graph, resources });
        Ok(())
    }
}
