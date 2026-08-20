use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderGraph {
    pub id: String,
    #[serde(default)]
    pub resources: ResourceDeclarations,
    #[serde(default)]
    pub pipelines: PipelineDeclarations,
    pub passes: Vec<Pass>,
    #[serde(skip)]
    pub executions: Vec<Execution>,
}

#[derive(Clone)]
pub enum Execution {
    Compute(usize),
    Render(Vec<usize>),
}

#[derive(Clone, Default, Deserialize)]
pub struct ResourceDeclarations {
    #[serde(default)]
    pub buffers: Vec<Buffer>,
    #[serde(default)]
    pub textures: Vec<Texture>,
    #[serde(default)]
    pub samplers: Vec<Sampler>,
}

#[derive(Clone, Deserialize)]
pub struct Buffer {
    pub id: String,
    pub array: String,
    #[serde(default)]
    pub usage: Vec<String>,
    #[serde(default = "frame_sync")]
    pub sync: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Texture {
    pub id: String,
    #[serde(default)]
    pub size: Vec<Extent>,
    pub format: String,
    #[serde(default)]
    pub usage: Vec<String>,
    #[serde(default = "one")]
    pub mip_level_count: u32,
    #[serde(default = "one")]
    pub sample_count: u32,
    #[serde(default = "dimension")]
    pub dimension: String,
    #[serde(default = "yes")]
    pub transient: bool,
    #[serde(skip)]
    pub slot: usize,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Extent {
    Pixels(u32),
    Canvas(String),
}

#[derive(Clone, Deserialize)]
pub struct Sampler {
    pub id: String,
    #[serde(default)]
    pub descriptor: Value,
}

#[derive(Clone, Default, Deserialize)]
pub struct PipelineDeclarations {
    #[serde(default)]
    pub render: Vec<RenderPipeline>,
    #[serde(default)]
    pub compute: Vec<ComputePipeline>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderPipeline {
    pub id: String,
    pub code: String,
    #[serde(default)]
    pub vertex: VertexStage,
    #[serde(default)]
    pub fragment: FragmentStage,
    #[serde(default)]
    pub primitive: Value,
    #[serde(default)]
    pub depth_stencil: Value,
    #[serde(default)]
    pub multisample: Value,
}

#[derive(Clone, Deserialize)]
pub struct ComputePipeline {
    pub id: String,
    pub code: String,
    #[serde(default = "compute_entry")]
    pub entry: String,
}

#[derive(Clone, Deserialize)]
pub struct VertexStage {
    #[serde(default = "vertex_entry")]
    pub entry: String,
    #[serde(default)]
    pub buffers: Vec<VertexBuffer>,
}

impl Default for VertexStage {
    fn default() -> Self {
        Self {
            entry: vertex_entry(),
            buffers: Vec::new(),
        }
    }
}

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VertexBuffer {
    pub array_stride: u64,
    #[serde(default = "vertex_step")]
    pub step_mode: String,
    #[serde(default)]
    pub attributes: Vec<VertexAttribute>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VertexAttribute {
    pub format: String,
    pub offset: u64,
    pub shader_location: u32,
}

#[derive(Clone, Deserialize)]
pub struct FragmentStage {
    #[serde(default = "fragment_entry")]
    pub entry: String,
    #[serde(default)]
    pub targets: Vec<FragmentTarget>,
}

impl Default for FragmentStage {
    fn default() -> Self {
        Self {
            entry: fragment_entry(),
            targets: Vec::new(),
        }
    }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FragmentTarget {
    pub format: String,
    #[serde(default)]
    pub blend: Value,
    pub write_mask: Option<u32>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pass {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub pipeline: String,
    #[serde(default)]
    pub after: Vec<String>,
    #[serde(skip)]
    pub dependencies: Vec<usize>,
    #[serde(default)]
    pub bindings: Vec<Binding>,
    #[serde(default)]
    pub color: Vec<ColorAttachment>,
    pub depth: Option<DepthAttachment>,
    #[serde(default)]
    pub vertex_buffers: Vec<VertexBinding>,
    pub index_buffer: Option<IndexBinding>,
    #[serde(default)]
    pub draw: Draw,
    #[serde(default = "dispatch")]
    pub dispatch: [u32; 3],
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
pub struct Binding {
    #[serde(default)]
    pub group: u32,
    pub binding: u32,
    pub resource: String,
    #[serde(default)]
    pub offset: u64,
    pub size: Option<u64>,
}

#[derive(Clone, Deserialize)]
pub struct ColorAttachment {
    pub resource: String,
    #[serde(default)]
    pub clear: Vec<f32>,
    #[serde(default = "clear_op")]
    pub load: String,
    #[serde(default = "store_op")]
    pub store: String,
}

#[derive(Clone, Deserialize)]
pub struct DepthAttachment {
    pub resource: String,
    #[serde(default = "one_f32")]
    pub clear: f32,
    #[serde(default = "clear_op")]
    pub load: String,
    #[serde(default = "store_op")]
    pub store: String,
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
pub struct VertexBinding {
    #[serde(default)]
    pub slot: u32,
    pub resource: String,
    #[serde(default)]
    pub offset: u64,
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
pub struct IndexBinding {
    pub resource: String,
    #[serde(default = "index_format")]
    pub format: String,
    #[serde(default)]
    pub offset: u64,
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Draw {
    #[serde(default = "three")]
    pub vertices: u32,
    #[serde(default)]
    pub indices: u32,
    #[serde(default = "one")]
    pub instances: u32,
    #[serde(default)]
    pub first_vertex: u32,
    #[serde(default)]
    pub first_index: u32,
    #[serde(default)]
    pub base_vertex: i32,
    #[serde(default)]
    pub first_instance: u32,
}

impl Default for Draw {
    fn default() -> Self {
        Self {
            vertices: 3,
            indices: 0,
            instances: 1,
            first_vertex: 0,
            first_index: 0,
            base_vertex: 0,
            first_instance: 0,
        }
    }
}

impl RenderGraph {
    pub fn prepare(&mut self) -> Result<(), &'static str> {
        if self.id.is_empty() || self.passes.is_empty() {
            return Err("GRAPH_SHAPE");
        }
        let mut ids = HashMap::new();
        for (index, pass) in self.passes.iter().enumerate() {
            if pass.id.is_empty() || ids.insert(pass.id.clone(), index).is_some() {
                return Err("GRAPH_PASS");
            }
        }
        let dependencies = self
            .passes
            .iter()
            .map(|pass| {
                pass.after
                    .iter()
                    .map(|id| ids.get(id).copied().ok_or("GRAPH_DEPENDENCY"))
                    .collect::<Result<HashSet<_>, _>>()
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut emitted = vec![false; self.passes.len()];
        let mut order = Vec::with_capacity(self.passes.len());
        while order.len() < self.passes.len() {
            let ready = (0..self.passes.len()).find(|&index| {
                !emitted[index]
                    && dependencies[index]
                        .iter()
                        .all(|dependency| emitted[*dependency])
            });
            let index = ready.ok_or("GRAPH_CYCLE")?;
            emitted[index] = true;
            order.push(index);
        }
        let original = self.passes.clone();
        self.passes = order
            .into_iter()
            .map(|index| original[index].clone())
            .collect();
        let sorted_ids: HashMap<_, _> = self
            .passes
            .iter()
            .enumerate()
            .map(|(index, pass)| (pass.id.clone(), index))
            .collect();
        for pass in &mut self.passes {
            pass.dependencies = pass.after.iter().map(|id| sorted_ids[id]).collect();
        }
        self.plan_resources()?;
        self.plan_execution();
        Ok(())
    }

    fn plan_resources(&mut self) -> Result<(), &'static str> {
        let mut used = HashSet::new();
        let mut lifetimes = HashMap::new();
        let mut render_pipelines = HashSet::new();
        let mut compute_pipelines = HashSet::new();
        for (frame, pass) in self.passes.iter().enumerate() {
            match pass.kind.as_str() {
                "render" => _ = render_pipelines.insert(pass.pipeline.clone()),
                "compute" => _ = compute_pipelines.insert(pass.pipeline.clone()),
                _ => return Err("GRAPH_PASS"),
            }
            for id in pass.resources() {
                used.insert(id.to_owned());
                lifetimes
                    .entry(id.to_owned())
                    .and_modify(|range: &mut (usize, usize)| range.1 = frame)
                    .or_insert((frame, frame));
            }
        }
        self.resources
            .buffers
            .retain(|value| used.contains(&value.id));
        self.resources
            .samplers
            .retain(|value| used.contains(&value.id));
        self.pipelines
            .render
            .retain(|value| render_pipelines.contains(&value.id));
        self.pipelines
            .compute
            .retain(|value| compute_pipelines.contains(&value.id));

        let mut textures = self
            .resources
            .textures
            .drain(..)
            .filter_map(|texture| {
                lifetimes
                    .get(&texture.id)
                    .copied()
                    .map(|range| (texture, range))
            })
            .collect::<Vec<_>>();
        textures.sort_by_key(|value| value.1 .0);
        let mut slots: Vec<(String, usize, bool)> = Vec::new();
        for (texture, (first, last)) in &mut textures {
            let key = texture.key()?;
            let reusable = texture
                .transient
                .then(|| {
                    slots
                        .iter()
                        .position(|slot| slot.2 && slot.0 == key && slot.1 < *first)
                })
                .flatten();
            texture.slot = match reusable {
                Some(slot) => {
                    slots[slot].1 = *last;
                    slot
                }
                None => {
                    slots.push((key, *last, texture.transient));
                    slots.len() - 1
                }
            };
        }
        self.resources.textures = textures.into_iter().map(|value| value.0).collect();
        self.validate_ids()
    }

    fn plan_execution(&mut self) {
        let mut executions = Vec::new();
        for index in 0..self.passes.len() {
            let merge = executions.last().is_some_and(|execution| match execution {
                Execution::Render(passes) => self.can_merge_render(*passes.last().unwrap(), index),
                Execution::Compute(_) => false,
            });
            if merge {
                let Some(Execution::Render(passes)) = executions.last_mut() else {
                    unreachable!()
                };
                passes.push(index);
            } else if self.passes[index].kind == "render" {
                executions.push(Execution::Render(vec![index]));
            } else {
                executions.push(Execution::Compute(index));
            }
        }
        self.executions = executions;
    }

    fn can_merge_render(&self, previous: usize, next: usize) -> bool {
        let previous = &self.passes[previous];
        let next = &self.passes[next];
        if next.kind != "render"
            || previous.color.len() != next.color.len()
            || self.sample_count(&previous.pipeline) != self.sample_count(&next.pipeline)
            || previous
                .color
                .iter()
                .zip(&next.color)
                .any(|(previous, next)| {
                    previous.resource != next.resource
                        || previous.store != "store"
                        || next.load != "load"
                })
        {
            return false;
        }
        let same_depth = match (&previous.depth, &next.depth) {
            (None, None) => true,
            (Some(previous), Some(next)) => {
                previous.resource == next.resource
                    && previous.store == "store"
                    && next.load == "load"
            }
            _ => false,
        };
        same_depth
            && !next.bindings.iter().any(|binding| {
                next.color
                    .iter()
                    .any(|attachment| attachment.resource == binding.resource)
                    || next
                        .depth
                        .as_ref()
                        .is_some_and(|attachment| attachment.resource == binding.resource)
            })
    }

    fn sample_count(&self, pipeline: &str) -> Option<u64> {
        self.pipelines
            .render
            .iter()
            .find(|value| value.id == pipeline)
            .map(|value| {
                value
                    .multisample
                    .get("count")
                    .and_then(Value::as_u64)
                    .unwrap_or(1)
            })
    }

    fn validate_ids(&self) -> Result<(), &'static str> {
        let mut resources = HashSet::new();
        for id in self
            .resources
            .buffers
            .iter()
            .map(|value| &value.id)
            .chain(self.resources.textures.iter().map(|value| &value.id))
            .chain(self.resources.samplers.iter().map(|value| &value.id))
        {
            if id.is_empty() || !resources.insert(id) {
                return Err("GRAPH_RESOURCE");
            }
        }
        if self
            .resources
            .buffers
            .iter()
            .any(|buffer| !matches!(buffer.sync.as_str(), "frame" | "loadout"))
        {
            return Err("GRAPH_BUFFER_SYNC");
        }
        let mut pipelines = HashSet::new();
        for id in self
            .pipelines
            .render
            .iter()
            .map(|value| &value.id)
            .chain(self.pipelines.compute.iter().map(|value| &value.id))
        {
            if id.is_empty() || !pipelines.insert(id) {
                return Err("GRAPH_PIPELINE");
            }
        }
        Ok(())
    }
}

impl Pass {
    fn resources(&self) -> Vec<&str> {
        self.bindings
            .iter()
            .map(|value| value.resource.as_str())
            .chain(self.color.iter().map(|value| value.resource.as_str()))
            .chain(self.depth.iter().map(|value| value.resource.as_str()))
            .chain(
                self.vertex_buffers
                    .iter()
                    .map(|value| value.resource.as_str()),
            )
            .chain(
                self.index_buffer
                    .iter()
                    .map(|value| value.resource.as_str()),
            )
            .collect()
    }
}

impl Texture {
    pub(crate) fn key(&self) -> Result<String, &'static str> {
        let mut usage = self.usage.clone();
        usage.sort_unstable();
        usage.dedup();
        serde_json::to_string(&(
            &self.size,
            &self.format,
            usage,
            self.mip_level_count,
            self.sample_count,
            &self.dimension,
        ))
        .map_err(|_| "GRAPH_RESOURCE")
    }
}

fn one() -> u32 {
    1
}
fn three() -> u32 {
    3
}
fn one_f32() -> f32 {
    1.0
}
fn yes() -> bool {
    true
}
fn dimension() -> String {
    "2d".into()
}
fn vertex_entry() -> String {
    "vertex".into()
}
fn fragment_entry() -> String {
    "fragment".into()
}
fn compute_entry() -> String {
    "main".into()
}
fn vertex_step() -> String {
    "vertex".into()
}
fn clear_op() -> String {
    "clear".into()
}
fn store_op() -> String {
    "store".into()
}
fn index_format() -> String {
    "uint32".into()
}
fn dispatch() -> [u32; 3] {
    [1, 1, 1]
}
fn frame_sync() -> String {
    "frame".into()
}
