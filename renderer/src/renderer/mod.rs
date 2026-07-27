use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::mpsc::Receiver};

use futures::channel::oneshot;
use log::info;
use ultraviolet::Vec4;
use wasm_bindgen::{prelude::Closure, JsCast, JsValue};
use wasm_bindgen_futures::spawn_local;
use web_sys::DedicatedWorkerGlobalScope;

use crate::{
    command_ring::CommandRing,
    gltf::{install_imported, ModelBounds},
    message::{camera_drag, CameraDrag, DrainEventError, MouseMessage, ResizeMessage, WindowEvent},
    render_data::{InstanceHandle, MeshHandle, RenderData, RenderDataConfig, RenderFlags},
    renderer::scene::Scene,
};

pub mod executors;
pub mod gpu_scene;
pub mod material;
pub mod pipeline_library;
pub mod profiler;
pub mod scene;
pub mod scene_frame;

pub use pipeline_library::PipelineLibrary;
pub type GpuResources = PipelineLibrary;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

struct ActiveCompiledV1 {
    id: crate::render_graph::CompiledGraphId,
    graph: crate::render_graph::CompiledGraph,
    _textures: Vec<wgpu::Texture>,
    views: Vec<wgpu::TextureView>,
    class_bases: Vec<usize>,
}

struct GpuTextureSlotV2 {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
}

enum PreparedExecutionV2 {
    FrustumCull,
    MeshQuery,
    LegacyForward {
        execution: usize,
        variants: Vec<(crate::render_data::PipelineKey, wgpu::RenderPipeline)>,
    },
    Fullscreen {
        execution: usize,
        bind_group: wgpu::BindGroup,
        pipeline: wgpu::RenderPipeline,
        _uniform: wgpu::Buffer,
    },
    Present,
}

struct ActiveCompiledV2 {
    id: crate::render_graph::CompiledGraphId,
    graph: crate::render_graph::CompiledGraphV2,
    runtime: crate::render_graph::RuntimePlanV2,
    textures: Vec<Vec<GpuTextureSlotV2>>,
    executions: Vec<PreparedExecutionV2>,
    _fullscreen_layout: wgpu::BindGroupLayout,
}

enum ActiveCompiledGraph {
    V1(ActiveCompiledV1),
    V2(ActiveCompiledV2),
}

#[derive(Clone, Copy)]
enum UploadGraph {
    Immediate,
    V1,
    V2(crate::render_graph::MeshQueryRuntimeKeyV2),
}

fn classify_upload_graph(graph: &ActiveCompiledGraph) -> UploadGraph {
    match graph {
        ActiveCompiledGraph::V1(_) => UploadGraph::V1,
        ActiveCompiledGraph::V2(active) => UploadGraph::V2(active.runtime.allocations.query),
    }
}

fn upload_query_for_render(
    pending: Option<UploadGraph>,
    active: Option<UploadGraph>,
) -> Option<crate::render_graph::MeshQueryRuntimeKeyV2> {
    match pending {
        Some(UploadGraph::V2(query)) => Some(query),
        Some(UploadGraph::V1 | UploadGraph::Immediate) => None,
        None => match active {
            Some(UploadGraph::V2(query)) => Some(query),
            _ => None,
        },
    }
}

fn resolve_culling_frustum(
    query: crate::render_graph::MeshQueryRuntimeKeyV2,
    read: impl FnOnce() -> Option<Result<[[f32; 4]; 6], crate::camera::FrustumError>>,
) -> Result<Option<[[f32; 4]; 6]>, crate::render_graph::GraphError> {
    if query.frustum_culled == crate::render_graph::TriStatePredicate::Any {
        return Ok(None);
    }
    match read() {
        Some(Ok(planes)) => Ok(Some(planes)),
        Some(Err(error)) => Err(crate::render_graph::GraphError::new(
            "GRAPH_EXECUTION_FAILED",
            format!("camera frustum is invalid: {error}"),
        )),
        None => Err(crate::render_graph::GraphError::new(
            "GRAPH_EXECUTION_FAILED",
            "culling graph requires a camera frustum, but the scene has no camera",
        )),
    }
}

fn update_validate_write_scene<S: scene::Scene>(
    scene: &mut S,
    queue: &wgpu::Queue,
    query: Option<crate::render_graph::MeshQueryRuntimeKeyV2>,
) -> Result<Option<[[f32; 4]; 6]>, crate::render_graph::GraphError> {
    scene.update_cpu();
    let planes = match query {
        Some(query) => resolve_culling_frustum(query, || scene.frustum_planes())?,
        None => None,
    };
    scene.write_uniforms(queue);
    Ok(planes)
}
impl ActiveCompiledGraph {
    fn id(&self) -> crate::render_graph::CompiledGraphId {
        match self {
            Self::V1(a) => a.id,
            Self::V2(a) => a.id,
        }
    }
    fn graph_id(&self) -> &str {
        match self {
            Self::V1(a) => &a.graph.graph_id,
            Self::V2(a) => &a.graph.graph_id,
        }
    }
    fn revision(&self) -> u32 {
        match self {
            Self::V1(a) => a.graph.revision,
            Self::V2(a) => a.graph.revision,
        }
    }
    fn schema_version(&self) -> u32 {
        match self {
            Self::V1(_) => 1,
            Self::V2(_) => 2,
        }
    }
    fn execution_count(&self) -> usize {
        match self {
            Self::V1(a) => a.graph.passes.len(),
            Self::V2(a) => a.graph.executions.len(),
        }
    }
    fn texture_slot_count(&self) -> usize {
        match self {
            Self::V1(a) => a.views.len(),
            Self::V2(a) => a.textures.iter().map(Vec::len).sum(),
        }
    }
}

struct PooledTransient {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

enum SwitchTarget {
    Immediate,
    Compiled(ActiveCompiledGraph),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResolvedSwitchRequest {
    Immediate,
    Compiled(crate::render_graph::CompiledGraphId),
}

fn resolve_switch_request(
    registry: &crate::render_graph::Registry,
    pending: bool,
    mode: u32,
    slot: u32,
    generation: u32,
) -> Result<ResolvedSwitchRequest, crate::render_graph::GraphError> {
    if pending {
        return Err(crate::render_graph::GraphError::new(
            "GRAPH_SWITCH_PENDING",
            "a graph switch is pending",
        ));
    }
    match mode {
        0 if slot == 0 && generation == 0 => Ok(ResolvedSwitchRequest::Immediate),
        0 => Err(crate::render_graph::GraphError::new(
            "STALE_GRAPH_ID",
            "immediate mode requires a zero id",
        )),
        1 => {
            let id = crate::render_graph::CompiledGraphId { slot, generation };
            // Resolve the registry entry here, before any GPU preparation or pending
            // state mutation. Registry::get is also the Phase 4 activation gate.
            registry.get_registered(id)?;
            Ok(ResolvedSwitchRequest::Compiled(id))
        }
        _ => Err(crate::render_graph::GraphError::new(
            "GRAPH_EXECUTION_UNSUPPORTED",
            "unknown render mode",
        )),
    }
}

#[cfg(test)]
mod switch_request_tests {
    use super::*;

    fn query(visible: crate::render_graph::TriStatePredicate) -> UploadGraph {
        UploadGraph::V2(crate::render_graph::MeshQueryRuntimeKeyV2 {
            visible,
            frustum_culled: crate::render_graph::TriStatePredicate::Any,
        })
    }

    #[test]
    fn upload_selection_follows_the_graph_rendered_for_the_commit_frame() {
        use crate::render_graph::TriStatePredicate::{Any, RequiredFalse, RequiredTrue};
        let selected =
            |pending, active| upload_query_for_render(pending, active).map(|query| query.visible);
        assert_eq!(
            selected(Some(query(RequiredFalse)), Some(query(RequiredTrue))),
            Some(RequiredFalse)
        );
        assert_eq!(
            selected(Some(UploadGraph::V1), Some(query(RequiredTrue))),
            None
        );
        assert_eq!(
            selected(Some(UploadGraph::Immediate), Some(query(RequiredTrue))),
            None
        );
        assert_eq!(
            selected(None, Some(query(RequiredTrue))),
            Some(RequiredTrue)
        );
        assert_eq!(selected(None, Some(UploadGraph::V1)), None);
        assert_eq!(selected(None, None), None);
        assert_eq!(selected(Some(query(Any)), None), Some(Any));
    }

    #[test]
    fn frustum_preflight_skips_any_and_distinguishes_missing_from_invalid() {
        use crate::render_graph::TriStatePredicate::{Any, RequiredFalse, RequiredTrue};
        let query = |frustum_culled| crate::render_graph::MeshQueryRuntimeKeyV2 {
            visible: RequiredTrue,
            frustum_culled,
        };
        let mut reads = 0;
        assert_eq!(
            resolve_culling_frustum(query(Any), || {
                reads += 1;
                None
            })
            .unwrap(),
            None
        );
        assert_eq!(
            reads, 0,
            "inactive frustum filtering must not read the camera"
        );
        let missing = resolve_culling_frustum(query(RequiredFalse), || None).unwrap_err();
        assert!(missing.message.contains("no camera"));
        let invalid = resolve_culling_frustum(query(RequiredFalse), || {
            Some(Err(crate::camera::FrustumError::Degenerate { plane: 2 }))
        })
        .unwrap_err();
        assert_eq!(invalid.code, "GRAPH_EXECUTION_FAILED");
        assert!(invalid.message.contains("invalid"));
    }

    #[test]
    fn v2_resolves_at_command_boundary_before_gpu_work() {
        let mut registry = crate::render_graph::Registry::default();
        let bytes = br#"{"schemaVersion":2,"graphId":"switch_v2","revision":1,"nodes":[]}"#;
        let (id, _) = registry.compile(bytes).unwrap();
        let active = "existing_v1";
        let pending: Option<&str> = None;
        assert_eq!(
            resolve_switch_request(&registry, false, 1, id.slot, id.generation).unwrap(),
            ResolvedSwitchRequest::Compiled(id)
        );
        assert_eq!(active, "existing_v1");
        assert_eq!(pending, None);
        assert!(registry.contains(id));

        let pending_error = resolve_switch_request(&registry, true, 1, id.slot, id.generation)
            .expect_err("an existing pending request must win");
        assert_eq!(pending_error.code, "GRAPH_SWITCH_PENDING");
        assert_eq!(active, "existing_v1");
        assert_eq!(pending, None);

        let invalid_replacement = br#"{"schemaVersion":2,"graphId":"switch_v2","revision":2,"nodes":[],"unexpected":true}"#;
        assert_eq!(
            registry.compile(invalid_replacement).unwrap_err().code,
            "GRAPH_JSON_INVALID"
        );
        let crate::render_graph::RegisteredGraph::V2(stored) = registry.get_registered(id).unwrap()
        else {
            panic!("the original V2 graph must remain registered")
        };
        assert_eq!(stored.revision, 1);

        registry.drop_graph(id).unwrap();
        assert_eq!(
            registry.get_registered(id).unwrap_err().code,
            "STALE_GRAPH_ID"
        );
    }

    #[test]
    fn resize_restart_snapshot_remains_bound_to_its_immutable_registry_revision() {
        let mut registry = crate::render_graph::Registry::default();
        let (id, _) = registry
            .compile(br#"{"schemaVersion":2,"graphId":"resize","revision":1,"nodes":[]}"#)
            .unwrap();
        let crate::render_graph::RegisteredGraph::V2(revision_one) =
            registry.get_registered(id).unwrap().clone()
        else {
            panic!("expected V2 graph")
        };
        let in_flight = InFlightV2Preparation {
            token: 1,
            id,
            purpose: V2PreparationPurpose::Resize,
            graph: revision_one,
        };
        let (revision_two_id, _) = registry
            .compile(br#"{"schemaVersion":2,"graphId":"resize","revision":2,"nodes":[]}"#)
            .unwrap();
        let crate::render_graph::RegisteredGraph::V2(original) =
            registry.get_registered(id).unwrap()
        else {
            panic!("expected V2 graph")
        };
        let crate::render_graph::RegisteredGraph::V2(revision_two) =
            registry.get_registered(revision_two_id).unwrap()
        else {
            panic!("expected V2 graph")
        };
        assert_eq!(in_flight.graph.revision, 1);
        assert_eq!(original.revision, 1);
        assert_eq!(revision_two.revision, 2);
        assert_ne!(id, revision_two_id);
    }
}

struct PendingSwitch {
    request: u32,
    target: SwitchTarget,
}

#[derive(Clone, Copy)]
enum V2PreparationPurpose {
    Switch { request: u32 },
    Resize,
}

struct InFlightV2Preparation {
    token: u64,
    id: crate::render_graph::CompiledGraphId,
    purpose: V2PreparationPurpose,
    graph: crate::render_graph::CompiledGraphV2,
}

struct V2PreparationCompletion {
    token: u64,
    purpose: V2PreparationPurpose,
    candidate: Result<ActiveCompiledV2, crate::render_graph::GraphError>,
    validation_error: Option<String>,
    out_of_memory_error: Option<String>,
}

struct CommandError {
    code: &'static str,
    details: JsValue,
}

impl From<&'static str> for CommandError {
    fn from(code: &'static str) -> Self {
        Self {
            code,
            details: JsValue::UNDEFINED,
        }
    }
}

impl From<crate::render_graph::GraphError> for CommandError {
    fn from(error: crate::render_graph::GraphError) -> Self {
        // GraphError details are JSON by construction. Parsing avoids panicking if that
        // invariant is ever accidentally broken at a command boundary.
        let details = js_sys::JSON::parse(&error.details.to_string()).unwrap_or(JsValue::NULL);
        Self {
            code: error.code,
            details,
        }
    }
}

fn render_data_error_code(error: &crate::render_data::RenderDataError) -> &'static str {
    use crate::render_data::RenderDataError::*;
    match error {
        InvalidMeshHandle | InvalidInstanceHandle | CannotDestroyDefaultInstance => "STALE_HANDLE",
        InvalidTransform => "INVALID_TRANSFORM",
        EmptyVertices
        | MismatchedVertexStreams
        | EmptyIndices
        | IndexOutOfBounds
        | NonFiniteGeometry
        | InputTooLarge => "INVALID_GEOMETRY",
        InvalidCapacityConfig { .. }
        | CapacityOverflow { .. }
        | CapacityExceeded { .. }
        | AllocationFailed { .. }
        | EmptyRange
        | RangeOverflow
        | RangeOutOfBounds
        | RangeOverlap => "RESOURCE_LIMIT",
        RevisionOverflow => "REVISION_OVERFLOW",
        StaleReplacementStage => "STALE_REPLACEMENT",
    }
}

/*pub struct GpuResources {
    // Core resources
    pipelines: Vec<wgpu::RenderPipeline>,

    // Layout management
    pipeline_layouts: Vec<wgpu::PipelineLayout>,
    bind_group_layouts: Vec<wgpu::BindGroupLayout>,

    // Simple name-based pipeline lookup
    pipeline_registry: HashMap<String, PipelineKey>,
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
    ) -> Result<PipelineKey, String> {
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
                cull_mode: if name == "gltf_standard_double_sided" {
                    None
                } else {
                    Some(wgpu::Face::Back)
                },
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
        let key = PipelineKey::new(index as u32);
        self.pipeline_registry.insert(name.to_string(), key);
        Ok(key)
    }

    pub fn find_pipeline(&self, name: &str) -> Option<PipelineKey> {
        self.pipeline_registry.get(name).copied()
    }

    pub fn get_or_create_pipeline(
        &mut self,
        device: &wgpu::Device,
        name: &str,
        vertex_layout: &[wgpu::VertexBufferLayout],
        shader_source: &str,
        surface_format: wgpu::TextureFormat,
    ) -> PipelineKey {
        if let Some(index) = self.find_pipeline(name) {
            return index;
        }

        self.create_pipeline(device, name, vertex_layout, shader_source, surface_format)
            .expect(&format!("Failed to create pipeline '{}'", name))
    }

    pub fn get_pipeline(&self, key: PipelineKey) -> &wgpu::RenderPipeline {
        &self.pipelines[key.get() as usize]
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
*/

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
    snapshot: crate::shared_snapshot::SharedSnapshot,
    snapshot_init_sent: bool,
    scene_frame: scene_frame::SceneFrameCache,
    gpu_scene: gpu_scene::GpuSceneCache,
    materials: material::MaterialResources,
    pub(crate) command_ring: Option<&'static CommandRing>,
    pending_replies: Vec<JsValue>,
    gpu_error: std::sync::Arc<std::sync::atomic::AtomicBool>,
    framing_radius: f32,
    graph_registry: crate::render_graph::Registry,
    active_compiled: Option<ActiveCompiledGraph>,
    pending_switch: Option<PendingSwitch>,
    in_flight_v2: Option<InFlightV2Preparation>,
    next_v2_preparation_token: u64,
    v2_preparation_completions: Rc<RefCell<Vec<V2PreparationCompletion>>>,
    transient_pool: HashMap<crate::render_graph::RuntimeTextureKey, Vec<PooledTransient>>,
    halted: bool,
    profiler: profiler::Profiler,
}

impl<T: Scene + 'static> Renderer<T> {
    fn reply(&mut self, request: u32, result: Result<JsValue, CommandError>) {
        let (ok, code, value, details) = match result {
            Ok(value) => (true, "OK", value, JsValue::UNDEFINED),
            Err(error) => (false, error.code, JsValue::UNDEFINED, error.details),
        };
        let reply = js_sys::Object::new();
        for (key, item) in [
            ("type", "reply".into()),
            ("request", request.into()),
            ("ok", ok.into()),
            ("code", code.into()),
            ("result", value),
            ("details", details),
        ] {
            let _ = js_sys::Reflect::set(&reply, &JsValue::from_str(key), &item);
        }
        self.pending_replies.push(reply.into());
    }

    fn drain_commands(&mut self) -> bool {
        let Some(ring) = self.command_ring else {
            return true;
        };
        let mut commands = Vec::new();
        if let Err(error) = ring.drain(|words| commands.push(words)) {
            log::error!("command ring closed: {error:?}");
            self.command_ring = None;
            self.post_fatal("RING_CORRUPT", &format!("{error:?}"));
            return false;
        }
        for words in commands {
            let (opcode, request) = (words[1], words[2]);
            let words = &words[1..];
            if opcode == 7 {
                let outcome = crate::take_payload(words[2])
                    .ok_or_else(|| crate::render_graph::GraphError {
                        code: "PAYLOAD_MISSING",
                        message: "staged payload is missing".into(),
                        details: serde_json::json!({"message":"staged payload is missing"}),
                    })
                    .and_then(|bytes| self.graph_registry.compile(&bytes));
                match outcome {
                    Ok((_id, summary)) => self.reply(
                        request,
                        Ok(js_sys::JSON::parse(&summary.to_string()).unwrap_or(JsValue::NULL)),
                    ),
                    Err(error) => self.reply(request, Err(error.into())),
                }
                continue;
            } else if opcode == 8 {
                let id = crate::render_graph::CompiledGraphId {
                    slot: words[2],
                    generation: words[3],
                };
                let outcome =
                    if self.active_compiled.as_ref().is_some_and(|a| a.id() == id) {
                        Err(crate::render_graph::GraphError::new(
                            "GRAPH_ACTIVE",
                            "compiled graph is active",
                        ))
                    } else if self.pending_switch.as_ref().is_some_and(
                        |p| matches!(&p.target, SwitchTarget::Compiled(a) if a.id() == id),
                    ) || self.in_flight_v2.as_ref().is_some_and(|p| p.id == id)
                    {
                        Err(crate::render_graph::GraphError::new(
                            "GRAPH_SWITCH_PENDING",
                            "compiled graph switch is pending",
                        ))
                    } else {
                        self.graph_registry.drop_graph(id)
                    };
                match outcome {
                    Ok(()) => self.reply(request, Ok(JsValue::UNDEFINED)),
                    Err(error) => self.reply(request, Err(error.into())),
                }
                continue;
            } else if opcode == 9 {
                let outcome = resolve_switch_request(
                    &self.graph_registry,
                    self.pending_switch.is_some() || self.in_flight_v2.is_some(),
                    words[2],
                    words[3],
                    words[4],
                )
                .and_then(|target| match target {
                    ResolvedSwitchRequest::Immediate => {
                        self.pending_switch = Some(PendingSwitch {
                            request,
                            target: SwitchTarget::Immediate,
                        });
                        Ok(())
                    }
                    ResolvedSwitchRequest::Compiled(id) => {
                        match self.graph_registry.get_registered(id)?.clone() {
                            crate::render_graph::RegisteredGraph::V1(graph) => {
                                self.prepare_compiled_snapshot(id, graph).map(|active| {
                                    self.pending_switch = Some(PendingSwitch {
                                        request,
                                        target: SwitchTarget::Compiled(ActiveCompiledGraph::V1(
                                            active,
                                        )),
                                    })
                                })
                            }
                            crate::render_graph::RegisteredGraph::V2(graph) => self
                                .begin_compiled_v2_preparation(
                                    id,
                                    graph,
                                    V2PreparationPurpose::Switch { request },
                                ),
                        }
                    }
                });
                if let Err(error) = outcome {
                    self.reply(request, Err(error.into()));
                }
                continue;
            }
            let outcome: Result<JsValue, &'static str> = (|| match opcode {
                1 => {
                    if words[3] > 1 {
                        return Err("INVALID_FRAMING");
                    }
                    let bytes = crate::take_payload(words[2]).ok_or("PAYLOAD_MISSING")?;
                    let imported =
                        crate::gltf::decode_gltf_owned(bytes).map_err(|_| "GLB_INVALID")?;
                    let pipelines = Self::ensure_gltf_pipelines(&mut self.resources, &self.context);
                    // Build a complete GPU candidate first. Neither the live scene nor
                    // its material epoch changes if image decode/resource creation fails.
                    let prepared_materials = self
                        .materials
                        .prepare(&self.context.device, &self.context.queue, &imported)
                        .map_err(|_| "MATERIAL_INVALID")?;
                    let installed = install_imported(&mut self.render_data, &imported, pipelines)
                        .map_err(|_| "INSTALL_FAILED")?;
                    // RenderData replacement and material publication are adjacent in
                    // this synchronous command, preventing a frame with mixed assets.
                    self.materials
                        .install(prepared_materials, self.render_data.revision());
                    if let Some(ModelBounds { min, max }) = installed.bounds {
                        let center = ultraviolet::Vec3::new(
                            (min[0] + max[0]) * 0.5,
                            (min[1] + max[1]) * 0.5,
                            (min[2] + max[2]) * 0.5,
                        );
                        let extent = ultraviolet::Vec3::new(
                            max[0] - min[0],
                            max[1] - min[1],
                            max[2] - min[2],
                        );
                        let radius = (0.5
                            * (extent.x * extent.x + extent.y * extent.y + extent.z * extent.z)
                                .sqrt())
                        .max(1.0);
                        self.framing_radius = radius;
                        log::info!(
                            "framing imported scene: center={center:?}, extent={extent:?}, radius={radius}"
                        );
                        self.scene.set_camera_depth_range(
                            (radius * 0.001).max(0.1),
                            (radius * 6.0).max(1.1),
                        );
                        if words[3] == 1 {
                            self.scene.set_camera_look_at(
                                center + ultraviolet::Vec3::new(0.0, radius * 0.05, 0.0),
                                center + ultraviolet::Vec3::new(radius, 0.0, 0.0),
                            );
                        } else {
                            self.scene.set_camera_look_at(
                                center
                                    + ultraviolet::Vec3::new(
                                        radius * 1.8,
                                        radius * 1.4,
                                        radius * 1.8,
                                    ),
                                center,
                            );
                        }
                    }
                    let result = js_sys::Object::new();
                    let meshes = js_sys::Array::new();
                    for h in installed.meshes {
                        meshes.push(&js_sys::Array::of2(
                            &h.slot().into(),
                            &h.generation().into(),
                        ));
                    }
                    js_sys::Reflect::set(&result, &"meshes".into(), &meshes).unwrap();
                    Ok(result.into())
                }
                2 => {
                    self.render_data
                        .set_mesh_flags(
                            MeshHandle::from_parts(words[2], words[3]),
                            RenderFlags::from_bits_retain(words[4]),
                        )
                        .map_err(|e| render_data_error_code(&e))?;
                    Ok(JsValue::UNDEFINED)
                }
                3 => {
                    let mesh = MeshHandle::from_parts(words[2], words[3]);
                    let mut m = [[0.; 4]; 4];
                    for i in 0..16 {
                        m[i / 4][i % 4] = f32::from_bits(words[4 + i]);
                    }
                    let h = self
                        .render_data
                        .create_instance(mesh, m, RenderFlags::from_bits_retain(words[20]))
                        .map_err(|e| render_data_error_code(&e))?;
                    Ok(js_sys::Array::of2(&h.slot().into(), &h.generation().into()).into())
                }
                4 => {
                    self.render_data
                        .set_instance_flags(
                            InstanceHandle::from_parts(words[2], words[3]),
                            RenderFlags::from_bits_retain(words[4]),
                        )
                        .map_err(|e| render_data_error_code(&e))?;
                    Ok(JsValue::UNDEFINED)
                }
                5 => {
                    let h = InstanceHandle::from_parts(words[2], words[3]);
                    let mut m = [[0.; 4]; 4];
                    for i in 0..16 {
                        m[i / 4][i % 4] = f32::from_bits(words[4 + i]);
                    }
                    self.render_data
                        .set_instance_transform(h, m)
                        .map_err(|e| render_data_error_code(&e))?;
                    Ok(JsValue::UNDEFINED)
                }
                6 => {
                    self.render_data
                        .destroy_instance(InstanceHandle::from_parts(words[2], words[3]))
                        .map_err(|e| render_data_error_code(&e))?;
                    Ok(JsValue::UNDEFINED)
                }
                _ => Err("UNKNOWN_OPCODE"),
            })();
            match outcome {
                Ok(value) => self.reply(request, Ok(value)),
                Err(code) => self.reply(request, Err(code.into())),
            }
        }
        true
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
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
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

    fn ensure_gltf_pipelines(
        resources: &mut GpuResources,
        context: &RendererContext,
    ) -> [crate::render_data::PipelineKey; 2] {
        let layout = gpu_scene::vertex_layouts();
        let culled = resources.get_or_create_pipeline(
            &context.device,
            "gltf_standard",
            &layout,
            include_str!("../gltf.wgsl"),
            context.surface_config.format,
        );
        let double_sided = resources.get_or_create_pipeline(
            &context.device,
            "gltf_standard_double_sided",
            &layout,
            include_str!("../gltf.wgsl"),
            context.surface_config.format,
        );
        [culled, double_sided]
    }

    fn prepare_compiled_snapshot(
        &mut self,
        id: crate::render_graph::CompiledGraphId,
        graph: crate::render_graph::CompiledGraph,
    ) -> Result<ActiveCompiledV1, crate::render_graph::GraphError> {
        crate::render_graph::validate_activatable(&graph)?;
        let surface = [
            self.context.surface_config.width,
            self.context.surface_config.height,
        ];
        let classes: Vec<_> = graph
            .allocation_classes
            .iter()
            .map(|class| (class.key.clone(), class.slot_count))
            .collect();
        let offsets = crate::render_graph::class_offsets(&classes, surface)?;
        let mut textures = Vec::new();
        let mut views = Vec::new();
        let mut class_bases = Vec::new();
        for (class, offset) in graph.allocation_classes.iter().zip(offsets) {
            class_bases.push(views.len());
            let key = crate::render_graph::runtime_texture_key(&class.key, surface)?;
            let required = offset.checked_add(class.slot_count).ok_or_else(|| {
                crate::render_graph::GraphError::new(
                    "GRAPH_RESOURCE_LIMIT",
                    "transient slot count overflow",
                )
            })? as usize;
            let bucket = self.transient_pool.entry(key.clone()).or_default();
            while bucket.len() < required {
                let usage = key
                    .usage
                    .iter()
                    .fold(wgpu::TextureUsages::empty(), |usage, item| {
                        usage
                            | match item {
                                crate::render_graph::TextureUsage::Sampled => {
                                    wgpu::TextureUsages::TEXTURE_BINDING
                                }
                                crate::render_graph::TextureUsage::Storage => {
                                    wgpu::TextureUsages::STORAGE_BINDING
                                }
                                crate::render_graph::TextureUsage::CopySrc => {
                                    wgpu::TextureUsages::COPY_SRC
                                }
                                crate::render_graph::TextureUsage::CopyDst => {
                                    wgpu::TextureUsages::COPY_DST
                                }
                                crate::render_graph::TextureUsage::ColorAttachment
                                | crate::render_graph::TextureUsage::DepthAttachment => {
                                    wgpu::TextureUsages::RENDER_ATTACHMENT
                                }
                            }
                    });
                let format = match key.format {
                    crate::render_graph::Format::Depth32Float => wgpu::TextureFormat::Depth32Float,
                    _ => {
                        return Err(crate::render_graph::GraphError::new(
                            "GRAPH_EXECUTION_UNSUPPORTED",
                            "unsupported transient texture format",
                        ))
                    }
                };
                let texture = self
                    .context
                    .device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some("render graph transient"),
                        size: wgpu::Extent3d {
                            width: key.extent.width,
                            height: key.extent.height,
                            depth_or_array_layers: key.extent.depth_or_array_layers,
                        },
                        mip_level_count: key.mip_level_count,
                        sample_count: key.sample_count,
                        dimension: wgpu::TextureDimension::D2,
                        format,
                        usage,
                        view_formats: &[],
                    });
                let view = texture.create_view(&Default::default());
                bucket.push(PooledTransient { texture, view });
            }
            for slot in offset as usize..required {
                textures.push(bucket[slot].texture.clone());
                views.push(bucket[slot].view.clone());
            }
        }
        Ok(ActiveCompiledV1 {
            id,
            graph,
            _textures: textures,
            views,
            class_bases,
        })
    }

    fn plan_compiled_v2(
        &self,
        graph: &crate::render_graph::CompiledGraphV2,
    ) -> Result<crate::render_graph::RuntimePlanV2, crate::render_graph::GraphError> {
        crate::render_graph::prepare_runtime_plan_v2(
            graph,
            crate::render_graph::RuntimeSurfaceContractV2 {
                format: self.context.surface_config.format,
                width: self.context.surface_config.width,
                height: self.context.surface_config.height,
                usage: self.context.surface_config.usage,
                view_formats: self.context.surface_config.view_formats.clone(),
            },
            Some(&self.context.device.limits()),
        )
    }

    fn create_compiled_v2_candidate(
        &mut self,
        id: crate::render_graph::CompiledGraphId,
        graph: crate::render_graph::CompiledGraphV2,
        runtime: crate::render_graph::RuntimePlanV2,
    ) -> Result<ActiveCompiledV2, crate::render_graph::GraphError> {
        use crate::render_graph::*;
        let fail = |message| GraphError::new("GRAPH_RUNTIME_PLAN_INVALID", message);
        let mut textures = Vec::with_capacity(runtime.allocations.classes.len());
        for class in &runtime.allocations.classes {
            let mut gpu_class = Vec::with_capacity(class.slots.len());
            for slot in &class.slots {
                let d = &slot.descriptor;
                let texture = self
                    .context
                    .device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some("V2 graph texture"),
                        size: wgpu::Extent3d {
                            width: d.extent.width,
                            height: d.extent.height,
                            depth_or_array_layers: d.extent.depth_or_array_layers,
                        },
                        mip_level_count: d.mip_level_count,
                        sample_count: d.sample_count,
                        dimension: d.dimension,
                        format: d.format,
                        usage: d.usage,
                        view_formats: &d.view_formats,
                    });
                let view = texture.create_view(&Default::default());
                gpu_class.push(GpuTextureSlotV2 {
                    _texture: texture,
                    view,
                });
            }
            textures.push(gpu_class);
        }
        let resolve = |resource: u32| -> Result<&wgpu::TextureView, GraphError> {
            let allocation = runtime
                .allocations
                .resource_allocations
                .get(resource as usize)
                .copied()
                .flatten()
                .ok_or_else(|| fail("resource has no GPU allocation"))?;
            textures
                .get(allocation.class as usize)
                .and_then(|c| c.get(allocation.slot as usize))
                .map(|s| &s.view)
                .ok_or_else(|| fail("resource allocation is out of bounds"))
        };
        let fullscreen_layout =
            self.context
                .device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("V2 fullscreen texture"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                    ],
                });
        let pipeline_layout =
            self.context
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("V2 fullscreen"),
                    bind_group_layouts: &[&fullscreen_layout],
                    push_constant_ranges: &[],
                });
        let shader = self
            .context
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("V2 fullscreen"),
                source: wgpu::ShaderSource::Wgsl(include_str!("fullscreen_copy.wgsl").into()),
            });
        let sampler = self
            .context
            .device
            .create_sampler(&wgpu::SamplerDescriptor {
                label: Some("V2 post linear clamp"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });
        let mut executions = Vec::new();
        for (index, execution) in graph.executions.iter().enumerate() {
            match execution.executor.key.as_str() {
                "frustum_cull" => executions.push(PreparedExecutionV2::FrustumCull),
                "mesh_query" => executions.push(PreparedExecutionV2::MeshQuery),
                "present" => executions.push(PreparedExecutionV2::Present),
                "fullscreen_copy" | "tone_map" | "bloom_extract" | "bloom_blur"
                | "bloom_composite" | "luminance_edge" => {
                    let sampled: Vec<_> = execution
                        .accesses
                        .iter()
                        .filter(|a| matches!(a.mode, AccessModeV2::SampledTexture))
                        .map(|a| a.resource)
                        .collect();
                    let source = *sampled
                        .first()
                        .ok_or_else(|| fail("fullscreen source missing"))?;
                    let second = *sampled.get(1).unwrap_or(&source);
                    let values: [f32; 8] = match execution.parameters {
                        NormalizedParametersV2::ToneMap { exposure } => {
                            [exposure, 0., 0., 0., 0., 0., 0., 0.]
                        }
                        NormalizedParametersV2::BloomExtract { threshold, knee } => {
                            [threshold, knee, 0., 0., 0., 0., 0., 0.]
                        }
                        NormalizedParametersV2::BloomBlur { direction, radius } => {
                            [direction[0], direction[1], radius, 0., 0., 0., 0., 0.]
                        }
                        NormalizedParametersV2::BloomComposite { intensity } => {
                            [intensity, 0., 0., 0., 0., 0., 0., 0.]
                        }
                        NormalizedParametersV2::LuminanceEdge { strength } => {
                            [strength, 0., 0., 0., 0., 0., 0., 0.]
                        }
                        _ => [0.; 8],
                    };
                    use wgpu::util::DeviceExt;
                    let uniform =
                        self.context
                            .device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("V2 post parameters"),
                                contents: bytemuck::cast_slice(&values),
                                usage: wgpu::BufferUsages::UNIFORM,
                            });
                    let ExecutionKindV2::Render {
                        color_attachments, ..
                    } = &execution.kind
                    else {
                        return Err(fail("fullscreen execution is not render"));
                    };
                    let target = color_attachments
                        .first()
                        .ok_or_else(|| fail("fullscreen target missing"))?
                        .resource;
                    let target_format = if graph.resources.get(target as usize).is_some_and(|r| matches!(r.plan, ResourcePlanV2::Texture { family, .. } if family == runtime.allocations.surface_family)) { runtime.surface.format } else { let a=runtime.allocations.resource_allocations[target as usize].ok_or_else(|| fail("fullscreen target allocation missing"))?; runtime.allocations.classes[a.class as usize].slots[a.slot as usize].descriptor.format };
                    let entry = match execution.executor.key.as_str() {
                        "fullscreen_copy" => "fs_copy",
                        "tone_map" => "fs_tone_map",
                        "bloom_extract" => "fs_bloom_extract",
                        "bloom_blur" => "fs_bloom_blur",
                        "bloom_composite" => "fs_bloom_composite",
                        "luminance_edge" => "fs_luminance_edge",
                        _ => unreachable!(),
                    };
                    let pipeline = self.context.device.create_render_pipeline(
                        &wgpu::RenderPipelineDescriptor {
                            label: Some("V2 post pipeline"),
                            layout: Some(&pipeline_layout),
                            vertex: wgpu::VertexState {
                                module: &shader,
                                entry_point: Some("vs_main"),
                                buffers: &[],
                                compilation_options: Default::default(),
                            },
                            primitive: Default::default(),
                            depth_stencil: None,
                            multisample: Default::default(),
                            fragment: Some(wgpu::FragmentState {
                                module: &shader,
                                entry_point: Some(entry),
                                targets: &[Some(wgpu::ColorTargetState {
                                    format: target_format,
                                    blend: None,
                                    write_mask: wgpu::ColorWrites::ALL,
                                })],
                                compilation_options: Default::default(),
                            }),
                            multiview: None,
                            cache: None,
                        },
                    );
                    let bind_group =
                        self.context
                            .device
                            .create_bind_group(&wgpu::BindGroupDescriptor {
                                label: Some("V2 fullscreen source"),
                                layout: &fullscreen_layout,
                                entries: &[
                                    wgpu::BindGroupEntry {
                                        binding: 0,
                                        resource: wgpu::BindingResource::TextureView(resolve(
                                            source,
                                        )?),
                                    },
                                    wgpu::BindGroupEntry {
                                        binding: 1,
                                        resource: wgpu::BindingResource::TextureView(resolve(
                                            second,
                                        )?),
                                    },
                                    wgpu::BindGroupEntry {
                                        binding: 2,
                                        resource: wgpu::BindingResource::Sampler(&sampler),
                                    },
                                    wgpu::BindGroupEntry {
                                        binding: 3,
                                        resource: uniform.as_entire_binding(),
                                    },
                                ],
                            });
                    executions.push(PreparedExecutionV2::Fullscreen {
                        execution: index,
                        bind_group,
                        pipeline,
                        _uniform: uniform,
                    });
                }
                "legacy_forward" => {
                    let ExecutionKindV2::Render {
                        color_attachments,
                        depth_stencil,
                    } = &execution.kind
                    else {
                        return Err(fail("legacy forward is not render"));
                    };
                    let color = color_attachments
                        .first()
                        .ok_or_else(|| fail("legacy color missing"))?;
                    let color_is_surface = graph
                        .resources
                        .get(color.resource as usize)
                        .is_some_and(|resource| {
                            matches!(
                                resource.plan,
                                ResourcePlanV2::SurfaceTarget { family }
                                    | ResourcePlanV2::Texture { family, .. }
                                    if family == runtime.allocations.surface_family
                            )
                        });
                    let color_format = if color_is_surface {
                        runtime.surface.format
                    } else {
                        let a = runtime
                            .allocations
                            .resource_allocations
                            .get(color.resource as usize)
                            .copied()
                            .flatten()
                            .ok_or_else(|| fail("color allocation missing"))?;
                        runtime
                            .allocations
                            .classes
                            .get(a.class as usize)
                            .and_then(|c| c.slots.get(a.slot as usize))
                            .map(|s| s.descriptor.format)
                            .ok_or_else(|| fail("color allocation invalid"))?
                    };
                    let depth_format = depth_stencil
                        .as_ref()
                        .map(|d| {
                            let a = runtime
                                .allocations
                                .resource_allocations
                                .get(d.resource as usize)
                                .copied()
                                .flatten()
                                .ok_or_else(|| fail("depth allocation missing"))?;
                            runtime
                                .allocations
                                .classes
                                .get(a.class as usize)
                                .and_then(|c| c.slots.get(a.slot as usize))
                                .map(|s| s.descriptor.format)
                                .ok_or_else(|| fail("depth allocation invalid"))
                        })
                        .transpose()?;
                    let config = execution
                        .inputs
                        .iter()
                        .filter_map(|i| graph.resources.get(i.resource as usize))
                        .find_map(|r| {
                            if let ResourcePlanV2::DepthStencilConfig { config } = r.plan {
                                Some(config)
                            } else {
                                None
                            }
                        })
                        .ok_or_else(|| fail("depth config missing"))?;
                    let compare = match config.depth_compare {
                        CompareFunctionV2::Never => wgpu::CompareFunction::Never,
                        CompareFunctionV2::Less => wgpu::CompareFunction::Less,
                        CompareFunctionV2::LessEqual => wgpu::CompareFunction::LessEqual,
                        CompareFunctionV2::Greater => wgpu::CompareFunction::Greater,
                        CompareFunctionV2::GreaterEqual => wgpu::CompareFunction::GreaterEqual,
                        CompareFunctionV2::Equal => wgpu::CompareFunction::Equal,
                        CompareFunctionV2::NotEqual => wgpu::CompareFunction::NotEqual,
                        CompareFunctionV2::Always => wgpu::CompareFunction::Always,
                    };
                    let mut variants = Vec::new();
                    let bases: Vec<_> = self.resources.pipeline_keys().collect();
                    for base in bases {
                        let variant = self
                            .resources
                            .create_target_variant(
                                &self.context.device,
                                base,
                                color_format,
                                depth_format,
                                compare,
                                config.depth_write_enabled,
                            )
                            .map_err(|e| GraphError::new("GRAPH_RUNTIME_PLAN_INVALID", e))?;
                        variants.push((base, variant));
                    }
                    executions.push(PreparedExecutionV2::LegacyForward {
                        execution: index,
                        variants,
                    });
                }
                _ => return Err(fail("unsupported prepared execution")),
            }
        }
        Ok(ActiveCompiledV2 {
            id,
            graph,
            runtime,
            textures,
            executions,
            _fullscreen_layout: fullscreen_layout,
        })
    }

    fn begin_compiled_v2_preparation(
        &mut self,
        id: crate::render_graph::CompiledGraphId,
        graph: crate::render_graph::CompiledGraphV2,
        purpose: V2PreparationPurpose,
    ) -> Result<(), crate::render_graph::GraphError> {
        let runtime = self.plan_compiled_v2(&graph)?;
        // Candidate construction allocates GPU resources, so the live scene preflight
        // belongs here: this is the earliest boundary with both the runtime query and
        // scene access, and precedes GPU work and all pending/in-flight mutation.
        resolve_culling_frustum(runtime.allocations.query, || self.scene.frustum_planes())?;
        let restart_graph = graph.clone();
        self.next_v2_preparation_token = self.next_v2_preparation_token.wrapping_add(1).max(1);
        let token = self.next_v2_preparation_token;
        self.context
            .device
            .push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        self.context
            .device
            .push_error_scope(wgpu::ErrorFilter::Validation);
        let candidate = self.create_compiled_v2_candidate(id, graph, runtime);
        let validation = self.context.device.pop_error_scope();
        let out_of_memory = self.context.device.pop_error_scope();
        self.in_flight_v2 = Some(InFlightV2Preparation {
            token,
            id,
            purpose,
            graph: restart_graph,
        });
        let completions = self.v2_preparation_completions.clone();
        spawn_local(async move {
            let validation_error = validation.await.map(|error| error.to_string());
            let out_of_memory_error = out_of_memory.await.map(|error| error.to_string());
            completions.borrow_mut().push(V2PreparationCompletion {
                token,
                purpose,
                candidate,
                validation_error,
                out_of_memory_error,
            });
        });
        Ok(())
    }

    fn drain_v2_preparation_completions(&mut self) {
        let completions = std::mem::take(&mut *self.v2_preparation_completions.borrow_mut());
        for completion in completions {
            let Some(in_flight) = self.in_flight_v2.as_ref() else {
                continue;
            };
            if in_flight.token != completion.token {
                continue;
            }
            self.in_flight_v2 = None;
            let result = if let Some(message) = completion.out_of_memory_error {
                Err(crate::render_graph::GraphError::new(
                    "GRAPH_RESOURCE_LIMIT",
                    message,
                ))
            } else if let Some(message) = completion.validation_error {
                Err(crate::render_graph::GraphError::new(
                    "GRAPH_RUNTIME_PLAN_INVALID",
                    message,
                ))
            } else {
                completion.candidate
            };
            match (completion.purpose, result) {
                (V2PreparationPurpose::Switch { request }, Ok(candidate)) => {
                    self.pending_switch = Some(PendingSwitch {
                        request,
                        target: SwitchTarget::Compiled(ActiveCompiledGraph::V2(candidate)),
                    });
                }
                (V2PreparationPurpose::Switch { request }, Err(error)) => {
                    self.reply(request, Err(error.into()));
                }
                (V2PreparationPurpose::Resize, Ok(candidate)) => {
                    self.active_compiled = Some(ActiveCompiledGraph::V2(candidate));
                }
                (V2PreparationPurpose::Resize, Err(error)) => {
                    log::error!(
                        "compiled graph resize preparation failed: {}",
                        error.message
                    );
                }
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn new(
        canvas: web_sys::OffscreenCanvas,
        events_chan: Receiver<WindowEvent>,
        profile: bool,
    ) -> Self {
        let id = wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU,
            ..Default::default()
        };

        let instance = wgpu::Instance::new(&id);
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::OffscreenCanvas(canvas.clone()))
            .unwrap();
        let mut adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                ..Default::default()
            })
            .await
            .unwrap();

        let optional_features = profiler::Profiler::requested_features(profile, adapter.features());
        let descriptor = wgpu::DeviceDescriptor {
            required_features: optional_features,
            required_limits: wgpu::Limits::default(),
            label: None,
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::default(),
        };

        let (device, queue) = match adapter.request_device(&descriptor).await {
            Ok(result) => result,
            Err(error) if !optional_features.is_empty() => {
                log::warn!("timestamp-enabled device request failed, retrying baseline: {error}");
                adapter = instance
                    .request_adapter(&wgpu::RequestAdapterOptions {
                        compatible_surface: Some(&surface),
                        force_fallback_adapter: false,
                        ..Default::default()
                    })
                    .await
                    .expect("surface-compatible adapter required for baseline device");
                let baseline = wgpu::DeviceDescriptor {
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    label: None,
                    memory_hints: wgpu::MemoryHints::default(),
                    trace: wgpu::Trace::default(),
                };
                adapter.request_device(&baseline).await.unwrap()
            }
            Err(error) => panic!("baseline WebGPU device request failed: {error}"),
        };
        info!("Adapter info: {:?}", adapter.get_info());
        info!("Adapter features: {:?}", adapter.features());
        info!("Adapter limits: {:?}", adapter.limits());
        let profiler = profiler::Profiler::new(profile, &device, &queue).await;
        let gpu_error = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let error_flag = gpu_error.clone();
        device.on_uncaptured_error(Box::new(move |error| {
            error_flag.store(true, std::sync::atomic::Ordering::Relaxed);
            log::error!("Uncaptured GPU error: {error}");
        }));

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_caps.formats[0],
            width: canvas.clone().width().max(1),
            height: canvas.clone().height().max(1),
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
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

        let mut render_data =
            RenderData::new(RenderDataConfig::default()).expect("valid render data config");
        let scene = T::setup(&context, &mut resources, &mut render_data);
        let materials = material::MaterialResources::new(&context.device, &context.queue);
        resources.set_material_bind_group_layout(&materials.layout);
        Self::ensure_gltf_pipelines(&mut resources, &context);

        Self {
            canvas,
            events_chan,
            context,
            scene,
            resources,
            render_data,
            snapshot: crate::shared_snapshot::SharedSnapshot::new(),
            snapshot_init_sent: false,
            scene_frame: Default::default(),
            gpu_scene: Default::default(),
            materials,
            command_ring: None,
            pending_replies: Vec::new(),
            gpu_error,
            framing_radius: 0.0,
            graph_registry: Default::default(),
            active_compiled: None,
            pending_switch: None,
            in_flight_v2: None,
            next_v2_preparation_token: 0,
            v2_preparation_completions: Default::default(),
            transient_pool: HashMap::new(),
            halted: false,
            profiler,
        }
    }

    fn render(&mut self, _time: f32) {
        if self.halted {
            return;
        }
        self.drain_v2_preparation_completions();
        if self
            .gpu_error
            .swap(false, std::sync::atomic::Ordering::AcqRel)
        {
            self.halted = true;
            self.post_fatal("GPU_VALIDATION_FAILED", "uncaptured WebGPU error");
            return;
        }
        if !self.drain_commands() {
            return;
        }
        let frame_plan = match self.scene_frame.get_or_build(&self.render_data) {
            Ok(plan) => plan,
            Err(error) => {
                log::error!("scene frame extraction failed: {error}");
                self.halted = true;
                self.post_fatal("SCENE_FRAME_FAILED", &error.to_string());
                return;
            }
        };
        let global = js_sys::global().unchecked_into::<DedicatedWorkerGlobalScope>();
        if !self.snapshot_init_sent {
            let message = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&message, &"type".into(), &"snapshot-init".into());
            let _ = js_sys::Reflect::set(
                &message,
                &"controlPtr".into(),
                &self.snapshot.control_ptr().into(),
            );
            let _ = js_sys::Reflect::set(&message, &"controlVersion".into(), &1.into());
            let _ = js_sys::Reflect::set(&message, &"schemaVersion".into(), &1.into());
            let _ = global.post_message(&message);
            self.snapshot_init_sent = true;
        }
        match self.snapshot.publish(frame_plan) {
            Ok(Some(epoch)) => {
                let message = js_sys::Object::new();
                let _ =
                    js_sys::Reflect::set(&message, &"type".into(), &"snapshot-published".into());
                let _ = js_sys::Reflect::set(&message, &"epoch".into(), &epoch.into());
                let _ = global.post_message(&message);
            }
            Ok(None) => {}
            Err(error) => log::error!("picking snapshot failed closed with error {error}"),
        }
        let pending = self.pending_switch.as_ref().map(|p| match &p.target {
            SwitchTarget::Immediate => UploadGraph::Immediate,
            SwitchTarget::Compiled(graph) => classify_upload_graph(graph),
        });
        let query = upload_query_for_render(
            pending,
            self.active_compiled.as_ref().map(classify_upload_graph),
        );
        // Resolve again immediately before every active frame. Do this before scene
        // upload so an invalid camera cannot mutate GPU state or produce a frame.
        let planes = match update_validate_write_scene(&mut self.scene, &self.context.queue, query)
        {
            Ok(planes) => planes,
            Err(error) => {
                if let Some(pending) = self.pending_switch.take() {
                    self.reply(pending.request, Err(error.into()));
                } else {
                    self.post_fatal("GRAPH_EXECUTION_FAILED", &error.message);
                }
                return;
            }
        };
        let upload = if let Some(query) = query {
            self.gpu_scene.upload_with_query(
                &self.context.device,
                &self.context.queue,
                frame_plan,
                query,
            )
        } else {
            self.gpu_scene
                .upload(&self.context.device, &self.context.queue, frame_plan)
        };
        if let Err(error) = upload {
            log::error!("GPU scene upload failed: {error}");
            self.post_fatal("GPU_UPLOAD_FAILED", &error);
            return;
        }
        if let Some(query) = query {
            self.gpu_scene
                .write_culling_params(&self.context.queue, planes, query);
        }

        let surface_texture = match self.context.surface.get_current_texture() {
            Ok(value) => value,
            Err(error) => {
                self.post_fatal("SURFACE_FRAME_FAILED", &error.to_string());
                return;
            }
        };
        let texture_view = surface_texture.texture.create_view(&Default::default());
        let mut encoder =
            self.context
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Render command encoder"),
                });

        let rendering_compiled = match self.pending_switch.as_ref().map(|p| &p.target) {
            Some(SwitchTarget::Compiled(active)) => Some(active),
            Some(SwitchTarget::Immediate) => None,
            None => self.active_compiled.as_ref(),
        };
        let mut profile_frame = self.profiler.begin(|| match rendering_compiled {
            None => "immediate".to_owned(),
            Some(ActiveCompiledGraph::V1(active)) => format!(
                "v1:{}:{}:{}:{}",
                active.graph.graph_id, active.graph.revision, active.id.slot, active.id.generation
            ),
            Some(ActiveCompiledGraph::V2(active)) => format!(
                "v2:{}:{}:{}:{}",
                active.graph.graph_id, active.graph.revision, active.id.slot, active.id.generation
            ),
        });
        let encode_result = if let Some(active) = rendering_compiled {
            match active {
                ActiveCompiledGraph::V1(active) => {
                    executors::encode_compiled_v1(
                        &mut encoder,
                        &texture_view,
                        active,
                        &self.scene,
                        &self.gpu_scene,
                        &self.resources,
                        &self.materials,
                        profile_frame.as_mut(),
                    );
                    Ok(())
                }
                ActiveCompiledGraph::V2(active) => executors::encode_compiled_v2(
                    &mut encoder,
                    &texture_view,
                    active,
                    &self.scene,
                    &self.gpu_scene,
                    &self.resources,
                    &self.materials,
                    profile_frame.as_mut(),
                ),
            }
        } else {
            executors::encode_immediate(
                &mut encoder,
                &texture_view,
                &self.context.depth_view,
                &self.scene,
                &self.gpu_scene,
                &self.resources,
                &self.materials,
                profile_frame.as_mut(),
            );
            Ok(())
        };
        if let Err(error) = encode_result {
            if let Some(frame) = profile_frame.take() {
                self.profiler.cancel(frame);
            }
            if let Some(pending) = self.pending_switch.take() {
                self.reply(
                    pending.request,
                    Err(
                        crate::render_graph::GraphError::new("GRAPH_EXECUTION_FAILED", error)
                            .into(),
                    ),
                );
            }
            return;
        }
        let profile_map = profile_frame.and_then(|frame| self.profiler.finish(&mut encoder, frame));
        self.context.queue.submit(std::iter::once(encoder.finish()));
        if let Some(request) = profile_map {
            self.profiler.map(request);
        }
        surface_texture.present();
        if let Some(pending) = self.pending_switch.take() {
            let result = match pending.target {
                SwitchTarget::Immediate => {
                    self.active_compiled = None;
                    js_sys::JSON::parse(r#"{"mode":"immediate"}"#).unwrap_or(JsValue::NULL)
                }
                SwitchTarget::Compiled(active) => {
                    let summary = serde_json::json!({
                        "mode":"compiled",
                        "compiledId":[active.id().slot, active.id().generation],
                        "graphId":active.graph_id(),
                        "revision":active.revision(),
                        "schemaVersion":active.schema_version()
                    });
                    self.active_compiled = Some(active);
                    js_sys::JSON::parse(&summary.to_string()).unwrap_or(JsValue::NULL)
                }
            };
            self.reply(pending.request, Ok(result));
        }
        let global = js_sys::global().unchecked_into::<DedicatedWorkerGlobalScope>();
        for reply in self.pending_replies.drain(..) {
            let _ = global.post_message(&reply);
        }
        let telemetry = js_sys::Object::new();
        let index_count: u32 = self
            .gpu_scene
            .draws
            .iter()
            .map(|d| d.indices.end - d.indices.start)
            .sum();
        let instance_count: u32 = self
            .gpu_scene
            .draws
            .iter()
            .map(|d| d.instances.end - d.instances.start)
            .sum();
        let active = self.active_compiled.as_ref();
        let active_id = active
            .map(|a| js_sys::Array::of2(&a.id().slot.into(), &a.id().generation.into()).into())
            .unwrap_or(JsValue::NULL);
        for (key, value) in [
            ("type", "telemetry".into()),
            ("revision", (self.render_data.revision() as f64).into()),
            ("draws", (self.gpu_scene.draws.len() as u32).into()),
            ("instances", instance_count.into()),
            ("indices", index_count.into()),
            ("width", self.context.surface_config.width.into()),
            ("height", self.context.surface_config.height.into()),
            ("framingRadius", self.framing_radius.into()),
            (
                "renderMode",
                if active.is_some() {
                    "compiled".into()
                } else {
                    "immediate".into()
                },
            ),
            ("activeCompiledId", active_id),
            (
                "activeCompiledSchemaVersion",
                active.map(|a| a.schema_version()).unwrap_or(0).into(),
            ),
            (
                "activeCompiledGraph",
                active.map(|a| a.graph_id()).unwrap_or("").into(),
            ),
            (
                "activeCompiledRevision",
                active.map(|a| a.revision()).unwrap_or(0).into(),
            ),
            (
                "graphPasses",
                active
                    .map(|a| a.execution_count() as u32)
                    .unwrap_or(0)
                    .into(),
            ),
            (
                "graphExecutions",
                active
                    .map(|a| a.execution_count() as u32)
                    .unwrap_or(0)
                    .into(),
            ),
            (
                "graphTextureSlots",
                active
                    .map(|a| a.texture_slot_count() as u32)
                    .unwrap_or(0)
                    .into(),
            ),
            (
                "transientPoolTextures",
                (self.transient_pool.values().map(Vec::len).sum::<usize>() as u32).into(),
            ),
            (
                "gpuError",
                self.gpu_error
                    .load(std::sync::atomic::Ordering::Relaxed)
                    .into(),
            ),
        ] {
            let _ = js_sys::Reflect::set(&telemetry, &key.into(), &value);
        }
        let _ = global.post_message(&telemetry);
        if let Some(snapshot) = self.profiler.snapshot_json(js_sys::Date::now()) {
            let _ = global.post_message(&snapshot);
        }
    }

    fn post_fatal(&mut self, code: &str, message: &str) {
        self.pending_replies.clear();
        let value = js_sys::Object::new();
        for (key, item) in [
            ("type", JsValue::from_str("fatal")),
            ("code", JsValue::from_str(code)),
            ("message", JsValue::from_str(message)),
        ] {
            let _ = js_sys::Reflect::set(&value, &key.into(), &item);
        }
        let global = js_sys::global().unchecked_into::<DedicatedWorkerGlobalScope>();
        let _ = global.post_message(&value);
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
            tx.send(result).unwrap();
        });

        // Poll the device to process the mapping

        rx.await.unwrap().unwrap();
        let depth_value = {
            let data = slice.get_mapped_range();
            let row_pitch = padded_row_bytes as usize;
            let byte_offset = y as usize * row_pitch + x as usize * pixel_size as usize;
            let mut depth_bytes = [0u8; 4];
            depth_bytes.copy_from_slice(&data[byte_offset..byte_offset + 4]);
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
                r.scene.handle_zoom(msg.delta_y_pixels);
            }
            WindowEvent::Keyboard(_) => {}
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
                if let Ok(mut r) = renderer.try_borrow_mut() {
                    r.render(time);
                }
            }

            Self::run_render_loop(renderer.clone());
        });

        let global = js_sys::global().unchecked_into::<DedicatedWorkerGlobalScope>();

        global
            .request_animation_frame(render_frame.as_ref().unchecked_ref())
            .unwrap();

        render_frame.forget();
    }

    fn resize(&mut self, msg: ResizeMessage) {
        let new_width = ((msg.width * msg.scale_factor) as u32).max(1);
        let new_height = ((msg.height * msg.scale_factor) as u32).max(1);
        if new_width != self.context.surface_config.width
            || new_height != self.context.surface_config.height
        {
            self.context.surface_config.width = new_width;
            self.context.surface_config.height = new_height;
            self.context
                .surface
                .configure(&self.context.device, &self.context.surface_config);
            self.recreate_depth_texture();
            // The executable subset uses surface-relative transients exclusively.
            // Dropping old buckets prevents stale-size reuse and bounds resize growth.
            self.transient_pool.clear();
            if let Some(pending) = self.pending_switch.take() {
                self.reply(
                    pending.request,
                    Err(crate::render_graph::GraphError::new(
                        "GRAPH_SWITCH_INVALIDATED",
                        "graph switch invalidated by resize",
                    )
                    .into()),
                );
            }
            let interrupted_resize =
                self.in_flight_v2
                    .take()
                    .and_then(|preparation| match preparation.purpose {
                        V2PreparationPurpose::Switch { request } => {
                            self.reply(
                                request,
                                Err(crate::render_graph::GraphError::new(
                                    "GRAPH_SWITCH_INVALIDATED",
                                    "graph switch invalidated by resize",
                                )
                                .into()),
                            );
                            None
                        }
                        V2PreparationPurpose::Resize => Some((preparation.id, preparation.graph)),
                    });
            let mut restarted_v2 = false;
            if let Some(old) = self.active_compiled.take() {
                let id = old.id();
                // Keep immediate resources live and fall back for this frame if recreation fails.
                match old {
                    ActiveCompiledGraph::V1(a) => self
                        .prepare_compiled_snapshot(id, a.graph)
                        .map(ActiveCompiledGraph::V1)
                        .map(|active| self.active_compiled = Some(active)),
                    ActiveCompiledGraph::V2(a) => {
                        restarted_v2 = true;
                        self.begin_compiled_v2_preparation(
                            id,
                            a.graph,
                            V2PreparationPurpose::Resize,
                        )
                    }
                }
                .unwrap_or_else(|error| {
                    log::error!(
                        "compiled graph resize preparation failed: {}",
                        error.message
                    )
                });
            }
            if !restarted_v2 {
                if let Some((id, graph)) = interrupted_resize {
                    if let Err(error) =
                        self.begin_compiled_v2_preparation(id, graph, V2PreparationPurpose::Resize)
                    {
                        log::error!(
                            "compiled graph resize preparation failed: {}",
                            error.message
                        );
                    }
                }
            }

            self.scene.resize(
                new_width as f64,
                new_height as f64,
                msg.scale_factor,
                &self.context.queue,
            );

            info!(
                "Resized: ({}, {}), scale: {}",
                new_width, new_height, msg.scale_factor
            );
        }
    }

    pub fn mouse_move(&mut self, msg: MouseMessage) {
        let delta_x = msg.movement_x as f32;
        let delta_y = msg.movement_y as f32;
        match camera_drag(msg.buttons) {
            Some(CameraDrag::Orbit) => self.scene.handle_orbit(delta_x, delta_y),
            Some(CameraDrag::Pan) => {
                self.scene
                    .handle_pan(delta_x, delta_y, msg.viewport_height as f32);
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod error_code_tests {
    use super::*;
    use crate::render_data::RenderDataError;
    #[test]
    fn render_data_errors_have_exact_stable_codes() {
        assert_eq!(
            render_data_error_code(&RenderDataError::InvalidMeshHandle),
            "STALE_HANDLE"
        );
        assert_eq!(
            render_data_error_code(&RenderDataError::InvalidTransform),
            "INVALID_TRANSFORM"
        );
        assert_eq!(
            render_data_error_code(&RenderDataError::StaleReplacementStage),
            "STALE_REPLACEMENT"
        );
    }
}
