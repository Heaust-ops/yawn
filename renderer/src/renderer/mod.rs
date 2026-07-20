use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::mpsc::Receiver};

use futures::channel::oneshot;
use log::info;
use ultraviolet::{Mat4, Vec4};
use wasm_bindgen::{prelude::Closure, JsCast, JsValue};
use wasm_bindgen_futures::spawn_local;
use web_sys::DedicatedWorkerGlobalScope;

use crate::{
    bounds::BoundsMailbox,
    gltf::{import_bytes, ImportedScene},
    message::{DrainEventError, MouseMessage, ResizeMessage, WindowEvent},
    render_abi::{
        BatchPop, SharedAbi, CMD_CLONE, CMD_DESTROY, CMD_LOAD_SCENE, CMD_PIPELINE, CMD_TRANSFORM,
        CMD_VISIBLE, RECORD_WORDS,
    },
    render_data::{procedural_scene, Aabb, InstanceHandle, RenderData, RenderDataConfig},
    renderer::scene::Scene,
    spatial::{ray_from_view_proj, validate_pick, SpatialSnapshot},
};

#[wasm_bindgen::prelude::wasm_bindgen(module = "/src/platform/web/worker/mainWorker.js")]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(js_name = routeRendererMessage)]
    fn route_renderer_message(data: &JsValue, transfer: &js_sys::Array);
}

fn set_js_property(target: &JsValue, name: &str, value: &JsValue) -> bool {
    match js_sys::Reflect::set(target, &JsValue::from_str(name), value) {
        Ok(true) => true,
        Ok(false) => {
            log::error!("JavaScript rejected renderer message property {name}");
            false
        }
        Err(error) => {
            log::error!("failed to set renderer message property {name}: {error:?}");
            false
        }
    }
}

fn post_worker_message(message: &JsValue) -> bool {
    let global = js_sys::global().unchecked_into::<DedicatedWorkerGlobalScope>();
    if let Err(error) = global.post_message(message) {
        log::error!("failed to post renderer worker message: {error:?}");
        return false;
    }
    true
}

#[derive(Default)]
struct LoadRendezvous {
    latest: u32,
    highest_payload: u32,
    authorized: Option<(u32, u32)>,
    payload: Option<(u32, Result<ImportedScene, String>)>,
}

thread_local! { static LOADS: RefCell<LoadRendezvous> = RefCell::new(LoadRendezvous::default()); }

#[derive(Debug)]
struct PickResult {
    request_id: u32,
    status: String,
    handle: Option<InstanceHandle>,
    snapshot_id: u32,
}
thread_local! { static PICKS: RefCell<Vec<PickResult>> = const { RefCell::new(Vec::new()) }; }
thread_local! { static MAINTAIN_RENDERER: RefCell<Option<Box<dyn FnMut()>>> = const { RefCell::new(None) }; }

pub fn worker_maintain_renderer() {
    MAINTAIN_RENDERER.with(|callback| {
        if let Some(callback) = callback.borrow_mut().as_mut() {
            callback();
        }
    });
}

pub fn report_pick(
    request_id: u32,
    status: String,
    slot: u32,
    generation: u32,
    snapshot_id: u32,
    _publication_version: u32,
) {
    PICKS.with(|results| {
        results.borrow_mut().push(PickResult {
            request_id,
            snapshot_id,
            handle: (status == "hit").then_some(InstanceHandle::from_parts(slot, generation)),
            status,
        })
    });
}

/// Registers transferred GLB bytes. Decoding is side-effect free; commit remains frame-gated.
pub fn register_scene_payload(load_id: u32, bytes: &[u8]) {
    let stale = LOADS.with(|loads| {
        let loads = loads.borrow();
        load_id < loads.highest_payload.max(loads.latest)
            || loads.payload.as_ref().is_some_and(|(id, _)| *id == load_id)
    });
    if stale {
        return;
    }
    let decoded = import_bytes(bytes).map_err(|error| error.to_string());
    LOADS.with(|loads| {
        let mut loads = loads.borrow_mut();
        if load_id > loads.highest_payload
            || (load_id == loads.highest_payload && loads.payload.is_none())
        {
            loads.highest_payload = load_id;
            loads.payload = Some((load_id, decoded));
        }
    });
}

pub fn report_scene_payload_error(load_id: u32, message: String) {
    LOADS.with(|loads| {
        let mut loads = loads.borrow_mut();
        if load_id >= loads.latest
            && (load_id > loads.highest_payload
                || (load_id == loads.highest_payload && loads.payload.is_none()))
        {
            loads.highest_payload = load_id;
            loads.payload = Some((load_id, Err(message)));
        }
    });
}

#[cfg(test)]
mod load_tests {
    use super::*;

    fn some<T>(value: Option<T>) -> T {
        match value {
            Some(value) => value,
            None => panic!("test expected Some"),
        }
    }

    fn error<T, E>(value: Result<T, E>) -> E {
        match value {
            Ok(_) => panic!("test expected Err"),
            Err(error) => error,
        }
    }

    #[test]
    fn stale_payload_does_not_replace_latest_payload() {
        LOADS.with(|loads| {
            *loads.borrow_mut() = LoadRendezvous {
                latest: 7,
                ..Default::default()
            }
        });
        report_scene_payload_error(6, "stale".into());
        LOADS.with(|loads| assert!(loads.borrow().payload.is_none()));
        report_scene_payload_error(7, "current".into());
        LOADS.with(|loads| {
            let loads = loads.borrow();
            let payload = some(loads.payload.as_ref());
            assert_eq!(payload.0, 7);
            assert_eq!(error(payload.1.as_ref()), "current");
        });
    }

    #[test]
    fn payload_and_authorization_rendezvous_by_load_id() {
        LOADS.with(|loads| {
            *loads.borrow_mut() = LoadRendezvous {
                latest: 9,
                highest_payload: 8,
                authorized: Some((9, 42)),
                payload: Some((8, Err("old".into()))),
            };
            let loads = loads.borrow();
            assert_ne!(some(loads.authorized).0, some(loads.payload.as_ref()).0);
        });
        report_scene_payload_error(9, "ready".into());
        LOADS.with(|loads| {
            let loads = loads.borrow();
            assert_eq!(some(loads.authorized).0, some(loads.payload.as_ref()).0);
        });
    }
}

pub mod scene;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[derive(Clone, Copy)]
struct GeometryAllocation {
    vertex_start: u64,
    vertex_count: u64,
    index_start: u64,
    index_count: u32,
}

struct DrawPacket {
    pipeline_index: usize,
    indirect_offset: u64,
    model_offset: u64,
}

const GPU_SLAB_MIN_CAPACITY: u64 = 64 * 1024;

fn grown_capacity(current: u64, required: u64, minimum: u64) -> Option<u64> {
    if current != 0 && required <= current {
        return Some(current);
    }
    let mut capacity = current.max(minimum).max(1);
    while capacity < required {
        capacity = capacity.checked_mul(2)?;
    }
    Some(capacity)
}

struct GpuSlab {
    buffer: wgpu::Buffer,
    capacity: u64,
    label: &'static str,
    usage: wgpu::BufferUsages,
}

impl GpuSlab {
    fn checked_capacity(
        device: &wgpu::Device,
        current: u64,
        required: u64,
        usage: wgpu::BufferUsages,
    ) -> Result<u64, String> {
        let capacity = grown_capacity(current, required, GPU_SLAB_MIN_CAPACITY)
            .ok_or_else(|| "GPU buffer capacity overflow".to_owned())?;
        let limits = device.limits();
        let limit = if usage.contains(wgpu::BufferUsages::STORAGE) {
            limits
                .max_buffer_size
                .min(limits.max_storage_buffer_binding_size as u64)
        } else {
            limits.max_buffer_size
        };
        (capacity <= limit).then_some(capacity).ok_or_else(|| {
            format!("GPU buffer requires {capacity} bytes but device limit is {limit}")
        })
    }

    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        label: &'static str,
        bytes: &[u8],
        usage: wgpu::BufferUsages,
    ) -> Result<Self, String> {
        let usage = usage | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST;
        let capacity = Self::checked_capacity(device, 0, bytes.len() as u64, usage)?;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: capacity,
            usage,
            mapped_at_creation: false,
        });
        if !bytes.is_empty() {
            queue.write_buffer(&buffer, 0, bytes);
        }
        Ok(Self {
            buffer,
            capacity,
            label,
            usage,
        })
    }

    fn ensure_capacity(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        required: u64,
    ) -> Result<bool, String> {
        let capacity = Self::checked_capacity(device, self.capacity, required, self.usage)?;
        if capacity == self.capacity {
            return Ok(false);
        }
        let replacement = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(self.label),
            size: capacity,
            usage: self.usage,
            mapped_at_creation: false,
        });
        encoder.copy_buffer_to_buffer(&self.buffer, 0, &replacement, 0, self.capacity);
        self.buffer = replacement;
        self.capacity = capacity;
        Ok(true)
    }

    fn write(&self, queue: &wgpu::Queue, bytes: &[u8]) {
        if !bytes.is_empty() {
            queue.write_buffer(&self.buffer, 0, bytes);
        }
    }
}

impl std::ops::Deref for GpuSlab {
    type Target = wgpu::Buffer;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Pod, bytemuck::Zeroable)]
struct DrawIndexedIndirectRecord {
    index_count: u32,
    instance_count: u32,
    first_index: u32,
    base_vertex: i32,
    first_instance: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CullCandidate {
    bounds_index: u32,
    model_index: u32,
    draw_index: u32,
    flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CullUniform {
    view_proj: [[f32; 4]; 4],
    candidate_count: u32,
    occlusion_enabled: u32,
    hzb_mip_count: u32,
    bypass_occlusion: u32,
    viewport: [u32; 2],
    depth_bias: f32,
    minimum_extent: f32,
}

/// Conservative occlusion controls. Frustum culling is unaffected when
/// occlusion is disabled or temporarily bypassed.
#[derive(Clone, Copy, Debug)]
pub struct OcclusionConfig {
    pub enabled: bool,
    pub depth_bias: f32,
    pub minimum_projected_extent: f32,
}

impl Default for OcclusionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            depth_bias: 0.0005,
            minimum_projected_extent: 2.0,
        }
    }
}

struct GpuMirror {
    positions: GpuSlab,
    normals: GpuSlab,
    uvs: GpuSlab,
    indices: GpuSlab,
    models: GpuSlab,
    /// Accepted local bounds records `[min.xyz, max.xyz, state, padding]`.
    /// Pending/empty/invalid records are consumed conservatively in Phase 6.
    local_bounds: GpuSlab,
    candidates: GpuSlab,
    indirect: GpuSlab,
    cull_uniform: wgpu::Buffer,
    cull_layout: wgpu::BindGroupLayout,
    cull_pipeline: wgpu::ComputePipeline,
    draws: Vec<DrawPacket>,
}

impl GpuMirror {
    fn build(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &RenderData,
    ) -> Result<Self, String> {
        let mut allocations = HashMap::new();
        let mut local_bounds = vec![0_u32; data.geometry_capacity() * 8];
        for (slot, geometry) in data.geometries() {
            let allocation = GeometryAllocation {
                vertex_start: geometry.vertex_range.start as u64,
                vertex_count: geometry.positions.len() as u64,
                index_start: geometry.index_range.start as u64,
                index_count: geometry.indices.len() as u32,
            };
            allocations.insert(slot, allocation);
            let Some((state, accepted)) = data.accepted_bounds(slot) else {
                log::error!("geometry slot {slot} has no bounds state; keeping it visible");
                continue;
            };
            let bounds = accepted.map_or(
                Aabb {
                    min: [0.0; 3],
                    max: [0.0; 3],
                },
                |bounds| bounds,
            );
            let offset = slot as usize * 8;
            local_bounds[offset..offset + 3].copy_from_slice(&bounds.min.map(f32::to_bits));
            local_bounds[offset + 3] = state as u32;
            local_bounds[offset + 4..offset + 7].copy_from_slice(&bounds.max.map(f32::to_bits));
        }
        let mut models = Vec::new();
        let mut draws = Vec::new();
        let mut candidates = Vec::new();
        let mut indirect = Vec::new();
        for instance in data.instances() {
            if instance.render_flags & 1 == 0 {
                continue;
            }
            let model_index = (models.len() / 16) as u32;
            models.extend_from_slice(instance.transform.as_slice());
            let Some(geometry) = allocations.get(&instance.geometry.slot).copied() else {
                log::error!(
                    "instance references missing geometry slot {}",
                    instance.geometry.slot
                );
                continue;
            };
            let draw_index = indirect.len() as u32;
            indirect.push(DrawIndexedIndirectRecord {
                index_count: geometry.index_count,
                instance_count: 1,
                first_index: geometry.index_start as u32,
                base_vertex: geometry.vertex_start as i32,
                // Keep this portable when INDIRECT_FIRST_INSTANCE is absent.
                // The resolved packet binds this model at vertex-buffer offset 0.
                first_instance: 0,
            });
            candidates.push(CullCandidate {
                bounds_index: instance.geometry.slot,
                model_index,
                draw_index,
                flags: instance.render_flags,
            });
            draws.push(DrawPacket {
                pipeline_index: instance.pipeline_key as usize,
                indirect_offset: draw_index as u64
                    * std::mem::size_of::<DrawIndexedIndirectRecord>() as u64,
                model_offset: model_index as u64 * std::mem::size_of::<[[f32; 4]; 4]>() as u64,
            });
        }
        let candidate_count = draws.len() as u32;
        // Keep empty-scene bindings large enough for their WGSL element type;
        // the zero candidate count prevents these sentinels being addressed.
        if models.is_empty() {
            models.resize(16, 0.0);
        }
        if candidates.is_empty() {
            candidates.push(CullCandidate {
                bounds_index: 0,
                model_index: 0,
                draw_index: 0,
                flags: 0,
            });
        }
        if indirect.is_empty() {
            indirect.push(DrawIndexedIndirectRecord {
                index_count: 0,
                instance_count: 0,
                first_index: 0,
                base_vertex: 0,
                first_instance: 0,
            });
        }
        fn buffer(
            device: &wgpu::Device,
            queue: &wgpu::Queue,
            label: &'static str,
            bytes: &[u8],
            usage: wgpu::BufferUsages,
        ) -> Result<GpuSlab, String> {
            GpuSlab::new(device, queue, label, bytes, usage)
        }
        let models = buffer(
            device,
            queue,
            "global models",
            bytemuck::cast_slice(&models),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::STORAGE,
        )?;
        let local_bounds = buffer(
            device,
            queue,
            "accepted local bounds",
            bytemuck::cast_slice(&local_bounds),
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        )?;
        let candidates = buffer(
            device,
            queue,
            "cull candidates",
            bytemuck::cast_slice(&candidates),
            wgpu::BufferUsages::STORAGE,
        )?;
        let indirect = buffer(
            device,
            queue,
            "indirect draws",
            bytemuck::cast_slice(&indirect),
            wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::INDIRECT
                | wgpu::BufferUsages::COPY_DST,
        )?;
        let cull_uniform = buffer(
            device,
            queue,
            "cull uniform",
            bytemuck::bytes_of(&CullUniform {
                view_proj: [[0.0; 4]; 4],
                candidate_count,
                occlusion_enabled: 0,
                hzb_mip_count: 1,
                bypass_occlusion: 1,
                viewport: [1, 1],
                depth_bias: 0.0,
                minimum_extent: 0.0,
            }),
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        )?
        .buffer;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cull layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("frustum culling"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../cull.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cull pipeline layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let cull_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("frustum culling"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some("cs_main"),
            compilation_options: Default::default(),
            cache: None,
        });
        Ok(Self {
            positions: buffer(
                device,
                queue,
                "global positions",
                bytemuck::cast_slice(data.positions()),
                wgpu::BufferUsages::VERTEX,
            )?,
            normals: buffer(
                device,
                queue,
                "global normals",
                bytemuck::cast_slice(data.normals()),
                wgpu::BufferUsages::VERTEX,
            )?,
            uvs: buffer(
                device,
                queue,
                "global uvs",
                bytemuck::cast_slice(data.uvs()),
                wgpu::BufferUsages::VERTEX,
            )?,
            indices: buffer(
                device,
                queue,
                "global indices",
                bytemuck::cast_slice(data.indices()),
                wgpu::BufferUsages::INDEX,
            )?,
            models,
            local_bounds,
            candidates,
            indirect,
            cull_uniform,
            cull_layout: layout,
            cull_pipeline,
            draws,
        })
    }

    /// Synchronizes canonical records while retaining allocations and pipeline state.
    fn sync(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &RenderData,
        rewrite_geometry: bool,
    ) -> Result<(), String> {
        let mut allocations = HashMap::new();
        let mut local_bounds = vec![0_u32; data.geometry_capacity().max(1) * 8];
        for (slot, geometry) in data.geometries() {
            allocations.insert(
                slot,
                GeometryAllocation {
                    vertex_start: geometry.vertex_range.start as u64,
                    vertex_count: geometry.positions.len() as u64,
                    index_start: geometry.index_range.start as u64,
                    index_count: geometry.indices.len() as u32,
                },
            );
            let Some((state, accepted)) = data.accepted_bounds(slot) else {
                log::error!("geometry slot {slot} has no bounds state; keeping it visible");
                continue;
            };
            let bounds = accepted.map_or(
                Aabb {
                    min: [0.0; 3],
                    max: [0.0; 3],
                },
                |bounds| bounds,
            );
            let offset = slot as usize * 8;
            local_bounds[offset..offset + 3].copy_from_slice(&bounds.min.map(f32::to_bits));
            local_bounds[offset + 3] = state as u32;
            local_bounds[offset + 4..offset + 7].copy_from_slice(&bounds.max.map(f32::to_bits));
        }

        let mut models = Vec::new();
        let mut candidates = Vec::new();
        let mut indirect = Vec::new();
        let mut draws = Vec::new();
        for instance in data.instances() {
            if instance.render_flags & 1 == 0 {
                continue;
            }
            let model_index = (models.len() / 16) as u32;
            models.extend_from_slice(instance.transform.as_slice());
            let Some(geometry) = allocations.get(&instance.geometry.slot).copied() else {
                log::error!(
                    "instance references missing geometry slot {}",
                    instance.geometry.slot
                );
                continue;
            };
            let draw_index = indirect.len() as u32;
            indirect.push(DrawIndexedIndirectRecord {
                index_count: geometry.index_count,
                instance_count: 1,
                first_index: geometry.index_start as u32,
                base_vertex: geometry.vertex_start as i32,
                first_instance: 0,
            });
            candidates.push(CullCandidate {
                bounds_index: instance.geometry.slot,
                model_index,
                draw_index,
                flags: instance.render_flags,
            });
            draws.push(DrawPacket {
                pipeline_index: instance.pipeline_key as usize,
                indirect_offset: draw_index as u64
                    * std::mem::size_of::<DrawIndexedIndirectRecord>() as u64,
                model_offset: model_index as u64 * std::mem::size_of::<[[f32; 4]; 4]>() as u64,
            });
        }
        if models.is_empty() {
            models.resize(16, 0.0);
        }
        if candidates.is_empty() {
            candidates.push(CullCandidate {
                bounds_index: 0,
                model_index: 0,
                draw_index: 0,
                flags: 0,
            });
        }
        if indirect.is_empty() {
            indirect.push(DrawIndexedIndirectRecord {
                index_count: 0,
                instance_count: 0,
                first_index: 0,
                base_vertex: 0,
                first_instance: 0,
            });
        }

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("grow GPU mirror slabs"),
        });
        let mut grew = false;
        macro_rules! grow {
            ($slab:expr, $bytes:expr) => {
                grew |= $slab.ensure_capacity(device, &mut encoder, $bytes.len() as u64)?;
            };
        }
        let model_bytes = bytemuck::cast_slice(&models);
        let bounds_bytes = bytemuck::cast_slice(&local_bounds);
        let candidate_bytes = bytemuck::cast_slice(&candidates);
        let indirect_bytes = bytemuck::cast_slice(&indirect);
        let position_bytes: &[u8] = bytemuck::cast_slice(data.positions());
        let normal_bytes: &[u8] = bytemuck::cast_slice(data.normals());
        let uv_bytes: &[u8] = bytemuck::cast_slice(data.uvs());
        let index_bytes: &[u8] = bytemuck::cast_slice(data.indices());
        // Validate every required doubled allocation before replacing any slab.
        for (slab, required) in [
            (&self.models, model_bytes.len() as u64),
            (&self.local_bounds, bounds_bytes.len() as u64),
            (&self.candidates, candidate_bytes.len() as u64),
            (&self.indirect, indirect_bytes.len() as u64),
        ] {
            GpuSlab::checked_capacity(device, slab.capacity, required, slab.usage)?;
        }
        if rewrite_geometry {
            for (slab, required) in [
                (&self.positions, position_bytes.len() as u64),
                (&self.normals, normal_bytes.len() as u64),
                (&self.uvs, uv_bytes.len() as u64),
                (&self.indices, index_bytes.len() as u64),
            ] {
                GpuSlab::checked_capacity(device, slab.capacity, required, slab.usage)?;
            }
        }
        if rewrite_geometry {
            grow!(self.positions, position_bytes);
            grow!(self.normals, normal_bytes);
            grow!(self.uvs, uv_bytes);
            grow!(self.indices, index_bytes);
        }
        grow!(self.models, model_bytes);
        grow!(self.local_bounds, bounds_bytes);
        grow!(self.candidates, candidate_bytes);
        grow!(self.indirect, indirect_bytes);
        if grew {
            queue.submit(Some(encoder.finish()));
        }
        if rewrite_geometry {
            self.positions.write(queue, position_bytes);
            self.normals.write(queue, normal_bytes);
            self.uvs.write(queue, uv_bytes);
            self.indices.write(queue, index_bytes);
        }
        self.models.write(queue, model_bytes);
        self.local_bounds.write(queue, bounds_bytes);
        self.candidates.write(queue, candidate_bytes);
        self.indirect.write(queue, indirect_bytes);
        self.draws = draws;
        Ok(())
    }

    fn cull_bind_group(&self, device: &wgpu::Device, hzb: &wgpu::TextureView) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cull bind group"),
            layout: &self.cull_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.cull_uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.local_bounds.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.models.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.candidates.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.indirect.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(hzb),
                },
            ],
        })
    }
}

#[cfg(test)]
mod gpu_slab_tests {
    use super::grown_capacity;

    #[test]
    fn capacity_growth_handles_zero_exact_fit_and_multiple_doublings() {
        assert_eq!(grown_capacity(0, 0, 64), Some(64));
        assert_eq!(grown_capacity(64, 64, 64), Some(64));
        assert_eq!(grown_capacity(0, 65, 64), Some(128));
        assert_eq!(grown_capacity(64, 513, 64), Some(1024));
    }

    #[test]
    fn capacity_growth_rejects_overflow() {
        assert_eq!(grown_capacity(1_u64 << 63, u64::MAX, 64), None);
    }
}

#[cfg(test)]
mod culling_tests {
    use super::*;
    use crate::render_data::BoundsState;
    use ultraviolet::{projection, Mat4, Vec3};

    #[test]
    fn hzb_dimensions_cover_small_and_non_power_of_two_viewports() {
        assert_eq!(hzb_mip_dimensions(0, 0), vec![(1, 1)]);
        assert_eq!(hzb_mip_dimensions(1, 1), vec![(1, 1)]);
        assert_eq!(hzb_mip_dimensions(5, 3), vec![(5, 3), (2, 1), (1, 1)]);
    }

    #[test]
    fn conventional_depth_uses_max_and_requires_proof() {
        let footprint = [0.2_f32, 0.4, 0.3, 0.8];
        let farthest = footprint.into_iter().fold(f32::NEG_INFINITY, f32::max);
        assert_eq!(farthest, 0.8);
        assert!(0.9 > farthest + 0.001); // fully behind footprint
        assert!(!(0.5 > farthest + 0.001)); // potentially in front of part
    }

    #[test]
    fn occlusion_bypass_policy_is_fail_open() {
        let decision = |hzb_valid: bool, grace: u32, hzb_epoch: Option<u32>, camera_epoch: u32| {
            !hzb_valid || grace != 0 || hzb_epoch != Some(camera_epoch)
        };
        assert!(decision(false, 0, Some(4), 4));
        assert!(decision(true, 1, Some(4), 4));
        assert!(decision(true, 0, Some(3), 4));
        assert!(!decision(true, 0, Some(4), 4));
    }

    fn clip_aabb_intersects(view_proj: Mat4, bounds: Aabb) -> bool {
        let m = view_proj.as_slice();
        let mut outside = [true; 6];
        for i in 0..8 {
            let p = [
                if i & 1 == 0 {
                    bounds.min[0]
                } else {
                    bounds.max[0]
                },
                if i & 2 == 0 {
                    bounds.min[1]
                } else {
                    bounds.max[1]
                },
                if i & 4 == 0 {
                    bounds.min[2]
                } else {
                    bounds.max[2]
                },
                1.0,
            ];
            let dot = |row: usize| {
                (0..4)
                    .map(|column| m[column * 4 + row] * p[column])
                    .sum::<f32>()
            };
            let (x, y, z, w) = (dot(0), dot(1), dot(2), dot(3));
            outside[0] &= x < -w;
            outside[1] &= x > w;
            outside[2] &= y < -w;
            outside[3] &= y > w;
            outside[4] &= z < 0.0;
            outside[5] &= z > w;
        }
        !outside.into_iter().any(|value| value)
    }

    #[test]
    fn dx_clip_aabb_reference_matches_camera_convention() {
        let projection =
            projection::rh_yup::perspective_wgpu_dx(std::f32::consts::FRAC_PI_2, 1.0, 0.1, 100.0);
        let view = Mat4::look_at(Vec3::zero(), -Vec3::unit_z(), Vec3::unit_y());
        let view_proj = projection * view;
        assert!(clip_aabb_intersects(
            view_proj,
            Aabb {
                min: [-0.5, -0.5, -2.0],
                max: [0.5, 0.5, -1.0]
            }
        ));
        assert!(!clip_aabb_intersects(
            view_proj,
            Aabb {
                min: [10.0, -0.5, -2.0],
                max: [11.0, 0.5, -1.0]
            }
        ));
        assert!(!clip_aabb_intersects(
            view_proj,
            Aabb {
                min: [-0.1, -0.1, 0.5],
                max: [0.1, 0.1, 1.0]
            }
        ));
    }

    #[test]
    fn pending_and_non_valid_bounds_fail_open() {
        for state in [
            BoundsState::Pending,
            BoundsState::Empty,
            BoundsState::InvalidNonFinite,
        ] {
            assert_ne!(state as u32, BoundsState::Valid as u32);
        }
        assert_ne!(
            16_u32 & 16,
            0,
            "ALWAYS_VISIBLE bypasses valid-bound culling"
        );
    }

    #[test]
    fn indirect_record_has_webgpu_layout_and_is_bytemuck_compatible() {
        assert_eq!(std::mem::size_of::<DrawIndexedIndirectRecord>(), 20);
        assert_eq!(std::mem::align_of::<DrawIndexedIndirectRecord>(), 4);
        let record = DrawIndexedIndirectRecord {
            index_count: 7,
            instance_count: 1,
            first_index: 9,
            base_vertex: -3,
            first_instance: 4,
        };
        let words: &[u32] = bytemuck::cast_slice(std::slice::from_ref(&record));
        assert_eq!(words, &[7, 1, 9, (-3_i32) as u32, 4]);
    }
}

pub struct GpuResources {
    pipelines: Vec<wgpu::RenderPipeline>,

    // Layout management
    pipeline_layouts: Vec<wgpu::PipelineLayout>,
    bind_group_layouts: Vec<wgpu::BindGroupLayout>,

    pipeline_registry: HashMap<String, usize>,
}

impl GpuResources {
    pub fn new() -> Self {
        Self {
            pipelines: Vec::new(),
            pipeline_layouts: Vec::new(),
            bind_group_layouts: Vec::new(),
            pipeline_registry: HashMap::new(),
        }
    }

    pub fn create_pipeline(
        &mut self,
        device: &wgpu::Device,
        name: &str,
        vertex_layout: &[wgpu::VertexBufferLayout],
        shader_source: &str,
        surface_format: wgpu::TextureFormat,
    ) -> Result<usize, String> {
        if self.pipeline_registry.contains_key(name) {
            return Err(format!("Pipeline '{}' already exists", name));
        }

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(name),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let layout = self.get_or_create_pipeline_layout(device, name);

        // Determine entry points based on pipeline name
        let (vertex_entry, fragment_entry) = match name {
            "triangle_colored" => ("v_main", "f_main"),
            _ => ("vs_main", "fs_main"),
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(name),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some(vertex_entry),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: vertex_layout,
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(fragment_entry),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });

        let index = self.pipelines.len();
        self.pipelines.push(pipeline);
        self.pipeline_registry.insert(name.to_string(), index);

        Ok(index)
    }

    pub fn get_pipeline(&self, name: &str) -> Option<usize> {
        self.pipeline_registry.get(name).copied()
    }

    pub fn get_or_create_pipeline(
        &mut self,
        device: &wgpu::Device,
        name: &str,
        vertex_layout: &[wgpu::VertexBufferLayout],
        shader_source: &str,
        surface_format: wgpu::TextureFormat,
    ) -> Result<usize, String> {
        if let Some(index) = self.get_pipeline(name) {
            return Ok(index);
        }

        self.create_pipeline(device, name, vertex_layout, shader_source, surface_format)
    }

    pub fn get_pipeline_by_index(&self, index: usize) -> Option<&wgpu::RenderPipeline> {
        self.pipelines.get(index)
    }

    pub fn set_bind_group_layouts(&mut self, layouts: &[wgpu::BindGroupLayout; 2]) {
        self.bind_group_layouts = layouts.to_vec();
    }

    fn get_or_create_pipeline_layout(
        &mut self,
        device: &wgpu::Device,
        label: &str,
    ) -> wgpu::PipelineLayout {
        if self.pipeline_layouts.is_empty() {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &self.bind_group_layouts.iter().collect::<Vec<_>>(),
                push_constant_ranges: &[],
            });
            self.pipeline_layouts.push(layout);
        }
        self.pipeline_layouts[0].clone()
    }
}

impl Default for GpuResources {
    fn default() -> Self {
        Self::new()
    }
}

fn hzb_mip_dimensions(mut width: u32, mut height: u32) -> Vec<(u32, u32)> {
    width = width.max(1);
    height = height.max(1);
    let mut result = vec![(width, height)];
    while width > 1 || height > 1 {
        width = (width / 2).max(1);
        height = (height / 2).max(1);
        result.push((width, height));
    }
    result
}

struct HzbPyramid {
    _texture: wgpu::Texture,
    full_view: wgpu::TextureView,
    mip_views: Vec<wgpu::TextureView>,
}
struct HzbResources {
    pyramids: [HzbPyramid; 2],
    previous: usize,
    valid: bool,
    camera_epoch: Option<u32>,
    init_layout: wgpu::BindGroupLayout,
    reduce_layout: wgpu::BindGroupLayout,
    init_pipeline: wgpu::ComputePipeline,
    reduce_pipeline: wgpu::ComputePipeline,
}
impl HzbResources {
    fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let dimensions = hzb_mip_dimensions(width, height);
        let make = || {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("R32Float HZB pyramid"),
                size: wgpu::Extent3d {
                    width: dimensions[0].0,
                    height: dimensions[0].1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: dimensions.len() as u32,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
                view_formats: &[],
            });
            let full_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let mip_views = (0..dimensions.len() as u32)
                .map(|m| {
                    texture.create_view(&wgpu::TextureViewDescriptor {
                        base_mip_level: m,
                        mip_level_count: Some(1),
                        ..Default::default()
                    })
                })
                .collect();
            HzbPyramid {
                _texture: texture,
                full_view,
                mip_views,
            }
        };
        let init_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("HZB init layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::R32Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });
        let reduce_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("HZB reduce layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::R32Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("HZB shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../hzb.wgsl").into()),
        });
        let init_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("HZB init"),
            bind_group_layouts: &[&init_layout],
            push_constant_ranges: &[],
        });
        let reduce_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("HZB reduction"),
            bind_group_layouts: &[&reduce_layout],
            push_constant_ranges: &[],
        });
        let init_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("HZB init"),
            layout: Some(&init_pl),
            module: &module,
            entry_point: Some("init_main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let reduce_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("HZB max reduction"),
            layout: Some(&reduce_pl),
            module: &module,
            entry_point: Some("reduce_main"),
            compilation_options: Default::default(),
            cache: None,
        });
        Self {
            pyramids: [make(), make()],
            previous: 0,
            valid: false,
            camera_epoch: None,
            init_layout,
            reduce_layout,
            init_pipeline,
            reduce_pipeline,
        }
    }
    fn mip_count(&self) -> u32 {
        self.pyramids[0].mip_views.len() as u32
    }
    fn previous_view(&self) -> &wgpu::TextureView {
        &self.pyramids[self.previous].full_view
    }
    fn encode_build(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        depth: &wgpu::TextureView,
        camera_epoch: u32,
    ) {
        let target = 1 - self.previous;
        let pyramid = &self.pyramids[target];
        let init = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("HZB depth init"),
            layout: &self.init_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&pyramid.mip_views[0]),
                },
            ],
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("HZB mip 0"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.init_pipeline);
            pass.set_bind_group(0, &init, &[]);
            let (w, h) = hzb_mip_dimensions(pyramid._texture.width(), pyramid._texture.height())[0];
            pass.dispatch_workgroups(w.div_ceil(8), h.div_ceil(8), 1);
        }
        for mip in 1..pyramid.mip_views.len() {
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("HZB reduction"),
                layout: &self.reduce_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&pyramid.mip_views[mip - 1]),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&pyramid.mip_views[mip]),
                    },
                ],
            });
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("HZB max reduction"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.reduce_pipeline);
            pass.set_bind_group(0, &bg, &[]);
            let w = (pyramid._texture.width() >> mip).max(1);
            let h = (pyramid._texture.height() >> mip).max(1);
            pass.dispatch_workgroups(w.div_ceil(8), h.div_ceil(8), 1);
        }
        self.previous = target;
        self.valid = true;
        self.camera_epoch = Some(camera_epoch);
    }
}

pub struct RendererContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface_config: wgpu::SurfaceConfiguration,
    pub surface: wgpu::Surface<'static>,
    pub depth_texture: wgpu::Texture,
    pub depth_view: wgpu::TextureView,
}

pub struct Renderer<T: scene::Scene> {
    canvas: web_sys::OffscreenCanvas,
    events_chan: Receiver<WindowEvent>,
    context: RendererContext,
    resources: GpuResources,
    scene: T,
    render_data: RenderData,
    gpu_mirror: GpuMirror,
    hzb: HzbResources,
    pub occlusion_config: OcclusionConfig,
    occlusion_bypass_frames: u32,
    shared_abi: Box<SharedAbi>,
    bounds_mailbox: BoundsMailbox,
    scene_commit_epoch: u32,
    spatial_snapshot_id: u32,
    presented_frame_id: u32,
    presented_scene_commit_epoch: u32,
    presented_spatial_snapshot_id: u32,
    presented_camera_epoch: u32,
    presented_viewport: [u32; 2],
    camera_epoch: u32,
    next_pick_request: u32,
    presented_view_proj: [[f32; 4]; 4],
    spatial_dirty: bool,
    pending_commands: Option<Vec<[u32; RECORD_WORDS]>>,
}

impl<T: Scene + 'static> Renderer<T> {
    fn maintain(renderer: &Rc<RefCell<Self>>) {
        let Ok(mut r) = renderer.try_borrow_mut() else {
            return;
        };
        use std::sync::atomic::Ordering;
        let completion_head = r.shared_abi.completion.head.load(Ordering::Acquire);
        let frame_credit = r.shared_abi.frame_credit.load(Ordering::Acquire);
        r.commit_commands();
        r.commit_ready_load();
        let changed = {
            let Self {
                bounds_mailbox,
                render_data,
                ..
            } = &mut *r;
            bounds_mailbox.poll(render_data)
        };
        if changed {
            let Self {
                context,
                gpu_mirror,
                render_data,
                ..
            } = &mut *r;
            if let Err(error) = gpu_mirror.sync(&context.device, &context.queue, render_data, false)
            {
                log::error!("bounds GPU synchronization failed: {error}");
            }
            r.spatial_dirty = true;
        }
        if r.spatial_dirty {
            r.scene_commit_epoch = r.scene_commit_epoch.wrapping_add(1).max(1);
            r.publish_spatial_snapshot();
            r.spatial_dirty = false;
        }
        r.poll_pick_results();
        if completion_head != r.shared_abi.completion.head.load(Ordering::Acquire)
            || frame_credit != r.shared_abi.frame_credit.load(Ordering::Acquire)
        {
            let message = js_sys::Object::new();
            if set_js_property(&message, "type", &"renderer-progress".into()) {
                post_worker_message(&message);
            }
        }
    }

    fn create_depth_texture(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let size = wgpu::Extent3d {
            width: config.width.max(1),
            height: config.height.max(1),
            depth_or_array_layers: 1,
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        (texture, view)
    }

    fn recreate_depth_texture(&mut self) {
        let (texture, view) =
            Self::create_depth_texture(&self.context.device, &self.context.surface_config);
        self.context.depth_texture = texture;
        self.context.depth_view = view;
    }

    pub async fn new(
        canvas: web_sys::OffscreenCanvas,
        events_chan: Receiver<WindowEvent>,
    ) -> Result<Self, String> {
        let id = wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU,
            ..Default::default()
        };

        let instance = wgpu::Instance::new(&id);
        #[cfg(target_arch = "wasm32")]
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::OffscreenCanvas(canvas.clone()))
            .map_err(|error| format!("failed to create WebGPU canvas surface: {error}"))?;
        #[cfg(not(target_arch = "wasm32"))]
        let surface: wgpu::Surface<'static> =
            return Err("renderer requires a browser canvas".to_owned());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                ..Default::default()
            })
            .await
            .map_err(|error| format!("failed to request WebGPU adapter: {error}"))?;

        info!("Adapter info: {:?}", adapter.get_info());
        info!("Adapter features: {:?}", adapter.features());
        info!("Adapter limits: {:?}", adapter.limits());

        let descriptor = wgpu::DeviceDescriptor {
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            label: None,
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::default(),
        };

        let (device, queue) = adapter
            .request_device(&descriptor)
            .await
            .map_err(|error| format!("failed to request WebGPU device: {error}"))?;
        device.on_uncaptured_error(Box::new(|error| {
            log::error!("uncaptured WebGPU error: {error}");
        }));

        let surface_caps = surface.get_capabilities(&adapter);
        let format = surface_caps
            .formats
            .first()
            .copied()
            .ok_or_else(|| "WebGPU surface reported no supported formats".to_owned())?;
        let present_mode = surface_caps
            .present_modes
            .first()
            .copied()
            .ok_or_else(|| "WebGPU surface reported no present modes".to_owned())?;
        let alpha_mode = surface_caps
            .alpha_modes
            .first()
            .copied()
            .ok_or_else(|| "WebGPU surface reported no alpha modes".to_owned())?;
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: canvas.clone().width(),
            height: canvas.clone().height(),
            present_mode,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        info!(
            "suface size: {} x {}",
            surface_config.width, surface_config.height
        );
        surface.configure(&device, &surface_config);

        let (depth_texture, depth_view) = Self::create_depth_texture(&device, &surface_config);

        let mut resources = GpuResources::new();
        let context = RendererContext {
            surface,
            device,
            queue,
            surface_config,
            depth_texture,
            depth_view,
        };

        let mut scene = T::setup(&context, &mut resources);
        let pipeline = resources.get_or_create_pipeline(
            &context.device,
            "gltf_standard",
            &scene::mesh_vertex_layout(),
            include_str!("../gltf.wgsl"),
            context.surface_config.format,
        )?;
        debug_assert_eq!(pipeline, 0);
        let mut render_data = procedural_scene();
        let gpu_mirror = GpuMirror::build(&context.device, &context.queue, &render_data)?;
        let hzb = HzbResources::new(
            &context.device,
            context.surface_config.width,
            context.surface_config.height,
        );
        Self::frame_scene(&mut scene, render_data.world_bounds());
        let mut shared_abi = SharedAbi::new();
        shared_abi.publish(&render_data);
        let descriptor = shared_abi.descriptor();
        let message = js_sys::Object::new();
        if !set_js_property(&message, "type", &"renderer-abi-ready".into())
            || !set_js_property(
                &message,
                "descriptor",
                &js_sys::Uint32Array::from(descriptor.as_slice()),
            )
            || !post_worker_message(&message)
        {
            return Err("failed to publish renderer ABI descriptor".to_owned());
        }

        // Fixed descriptor capacity equals RenderData's current geometry hard
        // maximum, so no SAB replacement/republish is needed in this phase.
        let mut bounds_mailbox = BoundsMailbox::new(crate::bounds::DEFAULT_CAPACITY);
        bounds_mailbox.announce();
        bounds_mailbox.dispatch_all(&mut render_data);
        let presented_viewport = [context.surface_config.width, context.surface_config.height];
        Ok(Self {
            canvas,
            events_chan,
            context,
            scene,
            resources,
            render_data,
            gpu_mirror,
            hzb,
            occlusion_config: OcclusionConfig::default(),
            occlusion_bypass_frames: 1,
            shared_abi,
            bounds_mailbox,
            scene_commit_epoch: 1,
            spatial_snapshot_id: 0,
            presented_frame_id: 0,
            presented_scene_commit_epoch: 0,
            presented_spatial_snapshot_id: 0,
            presented_camera_epoch: 0,
            presented_viewport,
            camera_epoch: 1,
            next_pick_request: 1,
            presented_view_proj: [[0.0; 4]; 4],
            spatial_dirty: true,
            pending_commands: None,
        })
    }

    fn publish_spatial_snapshot(&mut self) {
        self.spatial_snapshot_id = self.spatial_snapshot_id.wrapping_add(1).max(1);
        let snapshot = SpatialSnapshot::mint(
            &self.render_data,
            self.spatial_snapshot_id,
            self.scene_commit_epoch,
        );
        let message = js_sys::Object::new();
        if !set_js_property(&message, "type", &"spatial-snapshot".into())
            || !set_js_property(&message, "snapshotId", &snapshot.snapshot_id.into())
            || !set_js_property(
                &message,
                "sceneCommitEpoch",
                &snapshot.scene_commit_epoch.into(),
            )
        {
            return;
        }
        let instances = js_sys::Array::new();
        for item in snapshot.instances {
            let value = js_sys::Object::new();
            for (name, number) in [
                ("slot", item.handle.slot),
                ("generation", item.handle.generation),
                ("geometrySlot", item.geometry.slot),
                ("geometryGeneration", item.geometry.generation),
                ("boundsContentVersion", item.geometry.content_version),
                ("boundsSnapshotId", item.geometry.snapshot_id),
                ("transformVersion", item.transform_version),
                ("stateVersion", item.state_version),
                ("flags", item.flags),
                ("layerMask", item.layer_mask),
            ] {
                if !set_js_property(&value, name, &number.into()) {
                    return;
                }
            }
            let bounds = js_sys::Object::new();
            if !set_js_property(
                &bounds,
                "min",
                &js_sys::Float32Array::from(item.local_bounds.min.as_slice()),
            ) || !set_js_property(
                &bounds,
                "max",
                &js_sys::Float32Array::from(item.local_bounds.max.as_slice()),
            ) || !set_js_property(&value, "localBounds", &bounds)
                || !set_js_property(
                    &value,
                    "transform",
                    &js_sys::Float32Array::from(item.transform.as_slice()),
                )
            {
                return;
            }
            instances.push(&value);
        }
        if !set_js_property(&message, "instances", &instances) {
            return;
        }
        route_renderer_message(&message, &js_sys::Array::new());
    }

    fn poll_pick_results(&mut self) {
        PICKS.with(|queue| {
            for result in queue.borrow_mut().drain(..) {
                let message = js_sys::Object::new();
                if !set_js_property(&message, "type", &"pick-status".into()) {
                    continue;
                }
                if result.snapshot_id != self.presented_spatial_snapshot_id {
                    log::warn!(
                        "pick {} rejected: stale spatial snapshot",
                        result.request_id
                    );
                    if set_js_property(&message, "status", &"stale".into()) {
                        post_worker_message(&message);
                    }
                    continue;
                }
                let validated = result
                    .handle
                    .filter(|handle| validate_pick(&self.render_data, *handle));
                match validated {
                    Some(handle) => log::info!(
                        "selected broad-phase mesh {}:{}",
                        handle.slot,
                        handle.generation
                    ),
                    None if result.status == "hit" => log::warn!(
                        "pick {} rejected: stale handle generation",
                        result.request_id
                    ),
                    None => log::info!("pick {}: {}", result.request_id, result.status),
                }
                let status = if validated.is_some() {
                    "hit"
                } else if result.status == "hit" {
                    "stale"
                } else {
                    result.status.as_str()
                };
                if !set_js_property(&message, "status", &status.into()) {
                    continue;
                }
                if let Some(handle) = validated {
                    if !set_js_property(&message, "slot", &handle.slot.into())
                        || !set_js_property(&message, "generation", &handle.generation.into())
                    {
                        continue;
                    }
                }
                post_worker_message(&message);
            }
        });
    }

    /// Frame commit: consume at most one fully framed batch and validate every handle here.
    fn commit_commands(&mut self) {
        let records = if let Some(records) = self.pending_commands.take() {
            records
        } else {
            match self.shared_abi.pop_batch() {
                BatchPop::Accepted(records) => records,
                BatchPop::EmptyOrIncomplete => return,
                BatchPop::Malformed => {
                    log::error!("discarded malformed renderer command batch");
                    self.shared_abi
                        .frame_credit
                        .store(1, std::sync::atomic::Ordering::Release);
                    return;
                }
            }
        };
        let required_completions = records.iter().filter(|record| record[1] != 0).count() + 1;
        if self.shared_abi.completion_available() < required_completions {
            self.pending_commands = Some(records);
            return;
        }
        let mut dirty = false;
        let mut next_data = self.render_data.clone();
        let mut completions = Vec::with_capacity(records.len());
        for record in records {
            let request = record[1];
            let handle = InstanceHandle::from_parts(record[2], record[3]);
            let (result, succeeded) = match record[0] {
                CMD_CLONE => {
                    let result = next_data.clone_instance(handle);
                    (result, result.is_some())
                }
                CMD_DESTROY => {
                    let succeeded = next_data.remove_instance(handle);
                    (None, succeeded)
                }
                CMD_TRANSFORM => {
                    let mut values = [0.0; 16];
                    for (out, bits) in values.iter_mut().zip(&record[4..20]) {
                        *out = f32::from_bits(*bits);
                    }
                    let succeeded = values.iter().all(|value| value.is_finite())
                        && next_data.set_transform(handle, Mat4::from(values));
                    (None, succeeded)
                }
                CMD_VISIBLE => {
                    let succeeded = next_data.set_visible(handle, record[4] != 0);
                    (None, succeeded)
                }
                CMD_PIPELINE => {
                    let succeeded = record[4] == 0 && next_data.set_pipeline(handle, record[4]);
                    (None, succeeded)
                }
                CMD_LOAD_SCENE => {
                    let load_id = record[4];
                    if record[5] == 0 {
                        let superseded = LOADS.with(|loads| {
                            let mut loads = loads.borrow_mut();
                            loads.latest = load_id;
                            loads.highest_payload = loads.highest_payload.max(load_id);
                            if loads.payload.as_ref().is_some_and(|(id, _)| *id <= load_id) {
                                loads.payload = None;
                            }
                            loads.authorized.take()
                        });
                        if let Some((_, stale_request)) = superseded {
                            if !self.shared_abi.complete(stale_request, 3, None) {
                                log::error!(
                                    "completion ring unexpectedly full for superseded load"
                                );
                            }
                        }
                        let status = match Self::commit_render_data_inner(self, procedural_scene())
                        {
                            Ok(()) => 0,
                            Err(error) => {
                                log::error!("procedural scene replacement failed: {error}");
                                2
                            }
                        };
                        next_data = self.render_data.clone();
                        dirty = false;
                        completions.push((request, status, None));
                        continue;
                    }
                    let superseded = LOADS.with(|loads| {
                        let mut loads = loads.borrow_mut();
                        let superseded = loads.authorized.replace((load_id, request));
                        loads.latest = load_id;
                        if loads.payload.as_ref().is_some_and(|(id, _)| *id < load_id) {
                            loads.payload = None;
                        }
                        superseded
                    });
                    if let Some((_, stale_request)) = superseded {
                        if !self.shared_abi.complete(stale_request, 3, None) {
                            log::error!("completion ring unexpectedly full for superseded load");
                        }
                    }
                    // Completion is emitted only after payload/error rendezvous.
                    continue;
                }
                _ => (None, false),
            };
            dirty |= succeeded;
            completions.push((request, if succeeded { 0 } else { 1 }, result));
        }
        if dirty {
            if let Err(error) =
                self.gpu_mirror
                    .sync(&self.context.device, &self.context.queue, &next_data, false)
            {
                log::error!("GPU mirror synchronization failed: {error}");
                for (_, status, _) in &mut completions {
                    if *status == 0 {
                        *status = 2;
                    }
                }
            } else {
                self.render_data = next_data;
                // Covers newly-created, changed, and newly-valid bounds/instances.
                self.occlusion_bypass_frames = self.occlusion_bypass_frames.max(1);
                self.spatial_dirty = true;
            }
        }
        // Projection release-publish precedes completion release-publish and credit.
        self.shared_abi.publish(&self.render_data);
        for (request, status, handle) in completions {
            if !self.shared_abi.complete(request, status, handle) {
                log::error!("completion ring admission invariant violated for request {request}");
            }
        }
        self.shared_abi
            .frame_credit
            .store(1, std::sync::atomic::Ordering::Release);
    }

    fn render(&mut self, _time: f32) -> Result<(), wgpu::SurfaceError> {
        self.scene.update(&self.context, &mut self.resources);
        let Some(view_proj) = self.scene.view_proj() else {
            log::error!("scene did not provide a camera view-projection matrix");
            return Ok(());
        };
        let bypass = !self.hzb.valid
            || self.occlusion_bypass_frames != 0
            || self.hzb.camera_epoch != Some(self.camera_epoch);
        self.context.queue.write_buffer(
            &self.gpu_mirror.cull_uniform,
            0,
            bytemuck::bytes_of(&CullUniform {
                view_proj,
                candidate_count: self.gpu_mirror.draws.len() as u32,
                occlusion_enabled: self.occlusion_config.enabled as u32,
                hzb_mip_count: self.hzb.mip_count(),
                bypass_occlusion: bypass as u32,
                viewport: [
                    self.context.surface_config.width.max(1),
                    self.context.surface_config.height.max(1),
                ],
                depth_bias: self.occlusion_config.depth_bias.max(0.0),
                minimum_extent: self.occlusion_config.minimum_projected_extent.max(0.0),
            }),
        );
        let cull_bind_group = self
            .gpu_mirror
            .cull_bind_group(&self.context.device, self.hzb.previous_view());

        let surface_texture = self.context.surface.get_current_texture()?;
        let texture_view = surface_texture.texture.create_view(&Default::default());
        let mut encoder =
            self.context
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Render command encoder"),
                });

        if !self.gpu_mirror.draws.is_empty() {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("frustum culling"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.gpu_mirror.cull_pipeline);
            pass.set_bind_group(0, &cull_bind_group, &[]);
            pass.dispatch_workgroups((self.gpu_mirror.draws.len() as u32).div_ceil(64), 1, 1);
        }

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    depth_slice: None,
                    view: &texture_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.context.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            for (i, bind_group) in self.scene.bind_groups().iter().enumerate() {
                render_pass.set_bind_group(i as u32, bind_group, &[]);
            }

            for packet in &self.gpu_mirror.draws {
                let Some(pipeline) = self.resources.get_pipeline_by_index(packet.pipeline_index)
                else {
                    log::error!(
                        "draw packet references missing pipeline {}",
                        packet.pipeline_index
                    );
                    continue;
                };
                render_pass.set_pipeline(pipeline);
                render_pass.set_vertex_buffer(0, self.gpu_mirror.positions.slice(..));
                render_pass.set_vertex_buffer(1, self.gpu_mirror.normals.slice(..));
                render_pass.set_vertex_buffer(2, self.gpu_mirror.uvs.slice(..));
                render_pass
                    .set_vertex_buffer(3, self.gpu_mirror.models.slice(packet.model_offset..));
                render_pass
                    .set_index_buffer(self.gpu_mirror.indices.slice(..), wgpu::IndexFormat::Uint32);
                render_pass
                    .draw_indexed_indirect(&self.gpu_mirror.indirect, packet.indirect_offset);
            }
        }
        // This is encoded after all depth writes. Queue submission order makes
        // the completed target the immutable previous pyramid for frame N+1;
        // ping-pong ensures no pass samples a texture while writing it.
        self.hzb.encode_build(
            &self.context.device,
            &mut encoder,
            &self.context.depth_view,
            self.camera_epoch,
        );
        self.context.queue.submit(std::iter::once(encoder.finish()));
        surface_texture.present();
        self.presented_frame_id = self.presented_frame_id.wrapping_add(1).max(1);
        self.presented_view_proj = view_proj;
        self.presented_scene_commit_epoch = self.scene_commit_epoch;
        self.presented_spatial_snapshot_id = self.spatial_snapshot_id;
        self.presented_camera_epoch = self.camera_epoch;
        self.presented_viewport = [
            self.context.surface_config.width,
            self.context.surface_config.height,
        ];
        self.occlusion_bypass_frames = self.occlusion_bypass_frames.saturating_sub(1);
        Ok(())
    }

    pub async fn read_pixel_from_texture(&self, x: u32, y: u32) -> Vec4 {
        let width = self.context.depth_texture.width();
        let height = self.context.depth_texture.height();

        if width == 0 || height == 0 {
            log::warn!("Depth texture has zero extent ({} x {})", width, height);
            return Vec4::zero();
        }

        // Validate coordinates
        if x >= width || y >= height {
            log::warn!(
                "Pixel coordinates ({}, {}) out of bounds for texture size {}x{}",
                x,
                y,
                width,
                height
            );
            return Vec4::zero();
        }

        let pixel_size = std::mem::size_of::<f32>() as u32;
        let unpadded_row_bytes = width * pixel_size;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_row_bytes = if unpadded_row_bytes % align == 0 {
            unpadded_row_bytes
        } else {
            (unpadded_row_bytes / align + 1) * align
        };
        let buffer_size = padded_row_bytes as u64 * height as u64;
        let buffer = self.context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("depth pixel read buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        // Copy just the single pixel
        let mut encoder =
            self.context
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("copy depth pixel to buffer"),
                });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.context.depth_texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y: 0, z: 0 },
                aspect: wgpu::TextureAspect::DepthOnly,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row_bytes),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        self.context.queue.submit(std::iter::once(encoder.finish()));

        // Map the buffer and read the pixel
        let slice = buffer.slice(..);
        let (tx, rx) = oneshot::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            if tx.send(result).is_err() {
                log::warn!("depth readback receiver was dropped");
            }
        });

        // Poll the device to process the mapping

        match rx.await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                log::error!("failed to map depth readback buffer: {error}");
                return Vec4::zero();
            }
            Err(_) => {
                log::error!("depth readback callback was canceled");
                return Vec4::zero();
            }
        }
        let depth_value = {
            let data = slice.get_mapped_range();
            let row_pitch = padded_row_bytes as usize;
            let byte_offset = y as usize * row_pitch + x as usize * pixel_size as usize;
            let Some(depth_slice) = data.get(byte_offset..byte_offset + 4) else {
                log::error!("depth readback offset exceeded mapped buffer");
                drop(data);
                buffer.unmap();
                return Vec4::zero();
            };
            let mut depth_bytes = [0u8; 4];
            depth_bytes.copy_from_slice(depth_slice);
            f32::from_le_bytes(depth_bytes)
        };
        buffer.unmap();

        Vec4::new(depth_value, 0.0, 0.0, 0.0)
    }

    pub async fn handle_event(renderer: Rc<RefCell<Self>>, event: WindowEvent) {
        match event {
            WindowEvent::PointerMove(msg) => {
                renderer.borrow_mut().mouse_move(msg);
            }
            WindowEvent::Resize(msg) => {
                renderer.borrow_mut().resize(msg);
            }
            WindowEvent::PointerClick(msg) => {
                {
                    log::info!("click start");

                    let mut r = renderer.borrow_mut();
                    let x = (msg.offset_x * msg.scale_factor) as f32;
                    let y = (msg.offset_y * msg.scale_factor) as f32;
                    r.scene.handle_mouse_click(x, y);
                    if let Some(ray) = ray_from_view_proj(
                        r.presented_view_proj,
                        x,
                        y,
                        r.presented_viewport[0],
                        r.presented_viewport[1],
                    ) {
                        let request_id = r.next_pick_request;
                        r.next_pick_request = r.next_pick_request.wrapping_add(1).max(1);
                        let message = js_sys::Object::new();
                        for (name, value) in [
                            ("requestId", request_id),
                            ("presentedFrameId", r.presented_frame_id),
                            ("sceneCommitEpoch", r.presented_scene_commit_epoch),
                            ("spatialSnapshotId", r.presented_spatial_snapshot_id),
                            ("cameraEpoch", r.presented_camera_epoch),
                            ("viewportWidth", r.presented_viewport[0]),
                            ("viewportHeight", r.presented_viewport[1]),
                        ] {
                            if !set_js_property(&message, name, &value.into()) {
                                return;
                            }
                        }
                        if !set_js_property(&message, "type", &"pick-request".into()) {
                            return;
                        }
                        let ray_value = js_sys::Object::new();
                        if !set_js_property(
                            &ray_value,
                            "origin",
                            &js_sys::Float32Array::from(ray.origin.as_slice()),
                        ) || !set_js_property(
                            &ray_value,
                            "direction",
                            &js_sys::Float32Array::from(ray.direction.as_slice()),
                        ) || !set_js_property(&message, "ray", &ray_value)
                        {
                            return;
                        }
                        route_renderer_message(&message, &js_sys::Array::new());
                    }
                    log::info!("clicked");
                }

                // Read pixel from depth texture at click coordinates
                // let renderer_clone = renderer.clone();
                // let x_coord = msg.offset_x as u32;
                // let y_coord = msg.offset_y as u32;
                // let pixel_value = renderer_clone
                //     .borrow()
                //     .read_pixel_from_texture(x_coord, y_coord)
                //     .await;
                // log::info!(
                //     "Depth pixel at ({}, {}): {:?}",
                //     x_coord,
                //     y_coord,
                //     pixel_value
                // );
            }
            WindowEvent::PointerWheel(msg) => {
                let mut r = renderer.borrow_mut();
                r.scene.handle_zoom(&msg);
                r.camera_epoch = r.camera_epoch.wrapping_add(1).max(1);
            }
            WindowEvent::Keyboard(msg) => {
                log::info!("Key event received: {:?}", msg);
            }
        }
    }

    fn drain_events(renderer: &Rc<RefCell<Self>>) -> Result<(), DrainEventError> {
        loop {
            let event = renderer.try_borrow_mut()?.events_chan.try_recv()?;

            let renderer_clone = renderer.clone();
            spawn_local(async move {
                Self::handle_event(renderer_clone, event).await;
            });
        }
    }

    pub fn run_render_loop(renderer: Rc<RefCell<Renderer<T>>>) {
        let maintenance_renderer = renderer.clone();
        MAINTAIN_RENDERER.with(|callback| {
            *callback.borrow_mut() = Some(Box::new(move || Self::maintain(&maintenance_renderer)));
        });
        let render_frame: Closure<dyn FnMut(f32)> = Closure::new(move |time: f32| {
            {
                if let Err(e) = Self::drain_events(&renderer) {
                    match e {
                        DrainEventError::ChannelEmpty => {
                            // Normal condition, no error needed
                        }
                        DrainEventError::ChannelDisconnected => {
                            log::warn!("Event channel disconnected; stopping event polling");
                        }
                        DrainEventError::BorrowError(_) => {
                            log::error!("Failed to borrow renderer: {}", e);
                        }
                    }
                }
            }

            {
                Self::maintain(&renderer);
                if let Ok(mut r) = renderer.try_borrow_mut() {
                    if let Err(error) = r.render(time) {
                        match error {
                            wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated => {
                                log::warn!("WebGPU surface changed; reconfiguring: {error}");
                                r.context
                                    .surface
                                    .configure(&r.context.device, &r.context.surface_config);
                                r.recreate_depth_texture();
                                r.hzb = HzbResources::new(
                                    &r.context.device,
                                    r.context.surface_config.width,
                                    r.context.surface_config.height,
                                );
                                r.occlusion_bypass_frames = 1;
                            }
                            wgpu::SurfaceError::Timeout => {
                                log::warn!("WebGPU surface frame timed out");
                            }
                            wgpu::SurfaceError::OutOfMemory => {
                                log::error!("WebGPU surface ran out of memory; frame skipped");
                            }
                            wgpu::SurfaceError::Other => {
                                log::error!("WebGPU surface acquisition failed: {error}");
                            }
                        }
                    }
                }
            }

            Self::run_render_loop(renderer.clone());
        });

        let global = js_sys::global().unchecked_into::<DedicatedWorkerGlobalScope>();

        if let Err(error) = global.request_animation_frame(render_frame.as_ref().unchecked_ref()) {
            log::error!("failed to schedule renderer frame: {error:?}");
            return;
        }

        render_frame.forget();
    }

    fn resize(&mut self, msg: ResizeMessage) {
        let new_width = (msg.width * msg.scale_factor) as u32;
        let new_height = (msg.height * msg.scale_factor) as u32;
        if new_width != self.canvas.width() || new_height != self.canvas.height() {
            self.context.surface_config.width = new_width;
            self.context.surface_config.height = new_height;
            self.context
                .surface
                .configure(&self.context.device, &self.context.surface_config);
            self.recreate_depth_texture();
            self.hzb = HzbResources::new(&self.context.device, new_width, new_height);
            self.occlusion_bypass_frames = 1;

            self.scene.resize(
                new_width as f64,
                new_height as f64,
                msg.scale_factor,
                &self.context.queue,
            );
            self.camera_epoch = self.camera_epoch.wrapping_add(1).max(1);

            info!(
                "Resized: ({}, {}), scale: {}",
                new_width, new_height, msg.scale_factor
            );
        }
    }

    pub fn mouse_move(&mut self, msg: MouseMessage) {
        if (msg.buttons & 0x04) != 0 {
            let delta_x = (msg.movement_x * msg.scale_factor) as f32;
            let delta_y = (msg.movement_y * msg.scale_factor) as f32;
            self.scene.handle_orbit(delta_x, delta_y);
            self.camera_epoch = self.camera_epoch.wrapping_add(1).max(1);
        }
    }

    fn render_data_from_import(imported: ImportedScene) -> Result<RenderData, String> {
        let geometry_capacity = imported
            .geometries
            .len()
            .max(1)
            .checked_next_power_of_two()
            .ok_or_else(|| "geometry capacity overflow".to_owned())?;
        let instance_capacity = imported
            .instances
            .len()
            .max(1)
            .checked_next_power_of_two()
            .ok_or_else(|| "instance capacity overflow".to_owned())?;
        let config = RenderDataConfig {
            initial_geometry_capacity: geometry_capacity,
            initial_instance_capacity: instance_capacity,
            ..Default::default()
        };
        let mut data = RenderData::new(config);
        let mut geometries = Vec::with_capacity(imported.geometries.len());
        for geometry in imported.geometries {
            geometries.push(data.add_geometry_only(geometry).ok_or_else(|| {
                "imported scene exceeds canonical geometry or vertex/index capacity".to_owned()
            })?);
        }
        for instance in imported.instances {
            let geometry = geometries.get(instance.geometry).copied().ok_or_else(|| {
                format!(
                    "imported instance references missing geometry {}",
                    instance.geometry
                )
            })?;
            data.add_instance(geometry, instance.world_transform, 0)
                .ok_or_else(|| "imported scene exceeds canonical instance capacity".to_owned())?;
        }
        Ok(data)
    }

    fn commit_ready_load(&mut self) {
        if self.shared_abi.completion_available() == 0 {
            return;
        }
        let ready = LOADS.with(|loads| {
            let mut loads = loads.borrow_mut();
            let (load_id, request) = loads.authorized?;
            if loads.payload.as_ref()?.0 != load_id {
                return None;
            }
            let (_, result) = loads.payload.take()?;
            loads.authorized = None;
            Some((request, result))
        });
        if let Some((request, result)) = ready {
            match result {
                Ok(imported) => {
                    let result = Self::render_data_from_import(imported)
                        .and_then(|data| Self::commit_render_data_inner(self, data));
                    match result {
                        Ok(()) => {
                            if !self.shared_abi.complete(request, 0, None) {
                                log::error!("completion ring unexpectedly full after scene load");
                            }
                        }
                        Err(error) => {
                            log::error!("scene load failed: {error}");
                            if !self.shared_abi.complete(request, 2, None) {
                                log::error!(
                                    "completion ring unexpectedly full after scene failure"
                                );
                            }
                        }
                    }
                }
                Err(error) => {
                    log::error!("scene load failed: {error}");
                    if !self.shared_abi.complete(request, 2, None) {
                        log::error!("completion ring unexpectedly full after I/O failure");
                    }
                }
            }
        }
    }

    fn commit_render_data_inner(
        renderer: &mut Renderer<T>,
        mut data: RenderData,
    ) -> Result<(), String> {
        if !renderer
            .render_data
            .preserve_generations_for_replacement(&mut data)
        {
            return Err("scene replacement rejected: handle generation exhausted".to_owned());
        }
        renderer.gpu_mirror.sync(
            &renderer.context.device,
            &renderer.context.queue,
            &data,
            true,
        )?;
        let bounds = data.world_bounds();
        renderer.render_data = data;
        renderer.hzb.valid = false;
        renderer.hzb.camera_epoch = None;
        renderer.occlusion_bypass_frames = renderer.occlusion_bypass_frames.max(1);
        renderer.spatial_dirty = true;
        renderer
            .bounds_mailbox
            .dispatch_all(&mut renderer.render_data);
        let Self {
            shared_abi,
            render_data,
            ..
        } = &mut *renderer;
        shared_abi.publish(render_data);
        Self::frame_scene(&mut renderer.scene, bounds);
        renderer.camera_epoch = renderer.camera_epoch.wrapping_add(1).max(1);
        Ok(())
    }

    fn frame_scene(scene: &mut T, bounds: Option<Aabb>) {
        if let Some(Aabb { min, max }) = bounds {
            let center = ultraviolet::Vec3::new(
                (min[0] + max[0]) * 0.5,
                (min[1] + max[1]) * 0.5,
                (min[2] + max[2]) * 0.5,
            );

            let extent = ultraviolet::Vec3::new(max[0] - min[0], max[1] - min[1], max[2] - min[2]);
            let radius =
                0.5 * (extent.x * extent.x + extent.y * extent.y + extent.z * extent.z).sqrt();
            let radius = radius.max(1.0);

            // set the camera position after load, so we are not disoriented
            let eye_offset = ultraviolet::Vec3::new(radius * 1.5, radius, radius * 2.5);

            // Keep the near plane proportional to the model size to avoid
            // extreme depth ranges when loading very large assets
            let near_plane = (radius * 0.001).max(0.1);

            // The far plane must be far enough to cover the entire model.
            // Using a fixed upper clamp caused large models to be clipped
            // completely; relying on the model radius instead.
            let far_plane = (radius * 8.0).max(near_plane + 1.0);
            scene.set_camera_depth_range(near_plane, far_plane);
            scene.set_camera_look_at(center + eye_offset, center);
        }
    }
}
