use std::{cell::RefCell, rc::Rc, sync::mpsc::Receiver};

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

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
struct FullscreenUniforms {
    values: [[f32; 4]; 8],
}

fn pack_fullscreen_uniforms(
    key: &str,
    parameters: &crate::render_graph::NormalizedParameters,
) -> Option<FullscreenUniforms> {
    use crate::render_graph::NormalizedParameters;
    let first = match (key, parameters) {
        (
            "fullscreen_copy" | "frame_out",
            NormalizedParameters::FullscreenCopy | NormalizedParameters::FrameOut,
        ) => [0.; 4],
        ("tone_map", NormalizedParameters::ToneMap { exposure }) => [*exposure, 0., 0., 0.],
        ("bloom_extract", NormalizedParameters::BloomExtract { threshold, knee }) => {
            [*threshold, *knee, 0., 0.]
        }
        ("bloom_blur", NormalizedParameters::BloomBlur { direction, radius }) => {
            [direction[0], direction[1], *radius, 0.]
        }
        ("bloom_composite", NormalizedParameters::BloomComposite { intensity }) => {
            [*intensity, 0., 0., 0.]
        }
        ("luminance_edge", NormalizedParameters::LuminanceEdge { strength }) => {
            [*strength, 0., 0., 0.]
        }
        _ => return None,
    };
    let mut values = [[0.; 4]; 8];
    values[0] = first;
    Some(FullscreenUniforms { values })
}

fn resolve_fullscreen_entry(key: &str) -> Option<&'static str> {
    match key {
        "fullscreen_copy" | "frame_out" => Some("fs_copy"),
        "tone_map" => Some("fs_tone_map"),
        "bloom_extract" => Some("fs_bloom_extract"),
        "bloom_blur" => Some("fs_bloom_blur"),
        "bloom_composite" => Some("fs_bloom_composite"),
        "luminance_edge" => Some("fs_luminance_edge"),
        _ => None,
    }
}

#[cfg(test)]
mod fullscreen_tests {
    use super::*;
    use crate::render_graph::NormalizedParameters;

    #[test]
    fn fullscreen_uniform_abi_and_packer_are_fixed() {
        assert_eq!(std::mem::size_of::<FullscreenUniforms>(), 128);
        assert_eq!(std::mem::align_of::<FullscreenUniforms>(), 16);
        let packed = pack_fullscreen_uniforms(
            "bloom_blur",
            &NormalizedParameters::BloomBlur {
                direction: [0.0, 1.0],
                radius: 3.0,
            },
        )
        .unwrap();
        assert_eq!(packed.values[0], [0.0, 1.0, 3.0, 0.0]);
        assert!(packed.values[1..].iter().all(|value| *value == [0.0; 4]));
        assert_eq!(bytemuck::bytes_of(&packed).len(), 128);
        assert!(
            pack_fullscreen_uniforms("tone_map", &NormalizedParameters::FullscreenCopy).is_none()
        );
    }

    #[test]
    fn fullscreen_entries_are_explicit() {
        assert_eq!(resolve_fullscreen_entry("fullscreen_copy"), Some("fs_copy"));
        assert_eq!(resolve_fullscreen_entry("frame_out"), Some("fs_copy"));
        assert_eq!(resolve_fullscreen_entry("tone_map"), Some("fs_tone_map"));
        assert_eq!(
            resolve_fullscreen_entry("bloom_extract"),
            Some("fs_bloom_extract")
        );
        assert_eq!(
            resolve_fullscreen_entry("bloom_blur"),
            Some("fs_bloom_blur")
        );
        assert_eq!(
            resolve_fullscreen_entry("bloom_composite"),
            Some("fs_bloom_composite")
        );
        assert_eq!(
            resolve_fullscreen_entry("luminance_edge"),
            Some("fs_luminance_edge")
        );
        assert_eq!(resolve_fullscreen_entry("unknown"), None);
    }
}

struct GpuTextureSlot {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
}

enum PreparedExecution {
    FrustumCull,
    MeshQuery,
    PipelineRegistry,
    Pipeline {
        execution: usize,
        base: crate::render_data::PipelineKey,
        variant: wgpu::RenderPipeline,
    },
    Fullscreen {
        execution: usize,
        frame_out: bool,
        bind_group: wgpu::BindGroup,
        pipeline: wgpu::RenderPipeline,
        _uniform: wgpu::Buffer,
    },
}

struct ActiveCompiledGraph {
    id: crate::render_graph::CompiledGraphId,
    graph: crate::render_graph::CompiledGraph,
    runtime: crate::render_graph::RuntimePlan,
    textures: Vec<Vec<GpuTextureSlot>>,
    executions: Vec<PreparedExecution>,
    _fullscreen_layout: wgpu::BindGroupLayout,
}

#[derive(Clone, Copy)]
enum UploadGraph {
    Immediate,
    Compiled(crate::render_graph::MeshQueryRuntimeKey),
}

fn classify_upload_graph(graph: &ActiveCompiledGraph) -> UploadGraph {
    UploadGraph::Compiled(graph.runtime.allocations.query)
}

fn upload_query_for_render(
    pending: Option<UploadGraph>,
    active: Option<UploadGraph>,
) -> Option<crate::render_graph::MeshQueryRuntimeKey> {
    match pending.or(active) {
        Some(UploadGraph::Compiled(query)) => Some(query),
        Some(UploadGraph::Immediate) | None => None,
    }
}

fn resolve_culling_frustum(
    query: crate::render_graph::MeshQueryRuntimeKey,
    read: impl FnOnce() -> Option<Result<[[f32; 4]; 6], crate::camera::FrustumError>>,
) -> Result<Option<[[f32; 4]; 6]>, crate::render_graph::GraphError> {
    if matches!(
        query.frustum_culled,
        crate::render_graph::RuntimePredicate::Any | crate::render_graph::RuntimePredicate::Never
    ) {
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
    query: Option<crate::render_graph::MeshQueryRuntimeKey>,
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
        self.id
    }
    fn graph_id(&self) -> &str {
        &self.graph.graph_id
    }
    fn revision(&self) -> u32 {
        self.graph.revision
    }
    fn schema_version(&self) -> u32 {
        self.graph.schema_version
    }
    fn execution_count(&self) -> usize {
        self.graph.executions.len()
    }
    fn texture_slot_count(&self) -> usize {
        self.textures.iter().map(Vec::len).sum()
    }
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
            registry.get(id)?;
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
    fn valid_compile_graph(graph_id: &str, revision: u64) -> Vec<u8> {
        let mut graph = crate::render_graph::tests::full_cull_graph();
        graph["graphId"] = serde_json::json!(graph_id);
        graph["revision"] = serde_json::json!(revision);
        serde_json::to_vec(&graph).unwrap()
    }

    use super::*;

    fn query(visible: crate::render_graph::RuntimePredicate) -> UploadGraph {
        UploadGraph::Compiled(crate::render_graph::MeshQueryRuntimeKey {
            visible,
            frustum_culled: crate::render_graph::RuntimePredicate::Any,
        })
    }

    #[test]
    fn upload_selection_follows_the_graph_rendered_for_the_commit_frame() {
        use crate::render_graph::RuntimePredicate::{Any, RequiredFalse, RequiredTrue};
        let selected =
            |pending, active| upload_query_for_render(pending, active).map(|query| query.visible);
        assert_eq!(
            selected(Some(query(RequiredFalse)), Some(query(RequiredTrue))),
            Some(RequiredFalse)
        );
        assert_eq!(
            selected(Some(UploadGraph::Immediate), Some(query(RequiredTrue))),
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
        assert_eq!(selected(None, Some(UploadGraph::Immediate)), None);
        assert_eq!(selected(None, None), None);
        assert_eq!(selected(Some(query(Any)), None), Some(Any));
    }

    #[test]
    fn frustum_preflight_skips_any_and_distinguishes_missing_from_invalid() {
        use crate::render_graph::RuntimePredicate::{Any, RequiredFalse, RequiredTrue};
        let query = |frustum_culled| crate::render_graph::MeshQueryRuntimeKey {
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
    fn resolves_at_command_boundary_before_gpu_work() {
        let mut registry = crate::render_graph::Registry::default();
        let mut graph = crate::render_graph::tests::full_cull_graph();
        graph["graphId"] = serde_json::json!("switch");
        let bytes = serde_json::to_vec(&graph).unwrap();
        let (id, _) = registry.compile(&bytes).unwrap();
        let active = "existing_graph";
        let pending: Option<&str> = None;
        assert_eq!(
            resolve_switch_request(&registry, false, 1, id.slot, id.generation).unwrap(),
            ResolvedSwitchRequest::Compiled(id)
        );
        assert_eq!(active, "existing_graph");
        assert_eq!(pending, None);
        assert!(registry.contains(id));

        let pending_error = resolve_switch_request(&registry, true, 1, id.slot, id.generation)
            .expect_err("an existing pending request must win");
        assert_eq!(pending_error.code, "GRAPH_SWITCH_PENDING");
        assert_eq!(active, "existing_graph");
        assert_eq!(pending, None);

        let invalid_replacement =
            br#"{"schemaVersion":2,"graphId":"switch","revision":2,"nodes":[],"unexpected":true}"#;
        assert_eq!(
            registry.compile(invalid_replacement).unwrap_err().code,
            "GRAPH_JSON_INVALID"
        );
        let stored = registry.get(id).unwrap();
        assert_eq!(stored.revision, 1);

        registry.drop_graph(id).unwrap();
        assert_eq!(registry.get(id).unwrap_err().code, "STALE_GRAPH_ID");
    }

    #[test]
    fn resize_restart_snapshot_remains_bound_to_its_immutable_registry_revision() {
        let mut registry = crate::render_graph::Registry::default();
        let (id, _) = registry.compile(&valid_compile_graph("resize", 1)).unwrap();
        let revision_one = registry.get(id).unwrap().clone();
        let in_flight = InFlightPreparation {
            token: 1,
            id,
            purpose: PreparationPurpose::Resize,
            graph: revision_one,
        };
        let (revision_two_id, _) = registry.compile(&valid_compile_graph("resize", 2)).unwrap();
        let original = registry.get(id).unwrap();
        let revision_two = registry.get(revision_two_id).unwrap();
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
enum PreparationPurpose {
    Switch { request: u32 },
    Resize,
}

struct InFlightPreparation {
    token: u64,
    id: crate::render_graph::CompiledGraphId,
    purpose: PreparationPurpose,
    graph: crate::render_graph::CompiledGraph,
}

struct PreparationCompletion {
    token: u64,
    purpose: PreparationPurpose,
    candidate: Result<ActiveCompiledGraph, crate::render_graph::GraphError>,
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
    resources: PipelineLibrary,
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
    in_flight: Option<InFlightPreparation>,
    next_preparation_token: u64,
    preparation_completions: Rc<RefCell<Vec<PreparationCompletion>>>,
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
                    ) || self.in_flight.as_ref().is_some_and(|p| p.id == id)
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
                    self.pending_switch.is_some() || self.in_flight.is_some(),
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
                        let graph = self.graph_registry.get(id)?.clone();
                        self.begin_compiled_preparation(
                            id,
                            graph,
                            PreparationPurpose::Switch { request },
                        )
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
        resources: &mut PipelineLibrary,
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

    fn plan_compiled(
        &self,
        graph: &crate::render_graph::CompiledGraph,
    ) -> Result<crate::render_graph::RuntimePlan, crate::render_graph::GraphError> {
        crate::render_graph::prepare_runtime_plan(
            graph,
            crate::render_graph::RuntimeSurfaceContract {
                format: self.context.surface_config.format,
                width: self.context.surface_config.width,
                height: self.context.surface_config.height,
                usage: self.context.surface_config.usage,
                view_formats: self.context.surface_config.view_formats.clone(),
            },
            Some(&self.context.device.limits()),
        )
    }

    fn create_compiled_candidate(
        &mut self,
        id: crate::render_graph::CompiledGraphId,
        graph: crate::render_graph::CompiledGraph,
        runtime: crate::render_graph::RuntimePlan,
    ) -> Result<ActiveCompiledGraph, crate::render_graph::GraphError> {
        use crate::render_graph::*;
        let fail = |message| GraphError::new("GRAPH_RUNTIME_PLAN_INVALID", message);
        let resolved_pipelines = graph
            .executions
            .iter()
            .enumerate()
            .map(|(index, execution)| {
                let NormalizedParameters::Pipeline { pipeline, .. } = &execution.parameters else {
                    return Ok(None);
                };
                self.resources
                    .find_pipeline(pipeline)
                    .map(Some)
                    .ok_or_else(|| {
                        GraphError::at(
                            "GRAPH_EXECUTION_UNSUPPORTED",
                            format!("pipeline '{pipeline}' is not registered"),
                            format!("executions[{index}].parameters.pipeline"),
                        )
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut textures = Vec::with_capacity(runtime.allocations.classes.len());
        for class in &runtime.allocations.classes {
            let mut gpu_class = Vec::with_capacity(class.slots.len());
            for slot in &class.slots {
                let d = &slot.descriptor;
                let texture = self
                    .context
                    .device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some(" graph texture"),
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
                gpu_class.push(GpuTextureSlot {
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
                    label: Some(" fullscreen texture"),
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
                                min_binding_size: wgpu::BufferSize::new(128),
                            },
                            count: None,
                        },
                    ],
                });
        let pipeline_layout =
            self.context
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some(" fullscreen"),
                    bind_group_layouts: &[&fullscreen_layout],
                    push_constant_ranges: &[],
                });
        let shader = self
            .context
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(" fullscreen"),
                source: wgpu::ShaderSource::Wgsl(include_str!("fullscreen_copy.wgsl").into()),
            });
        let sampler = self
            .context
            .device
            .create_sampler(&wgpu::SamplerDescriptor {
                label: Some(" post linear clamp"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });
        let mut executions = Vec::new();
        for (index, execution) in graph.executions.iter().enumerate() {
            let contract = crate::render_graph::contract(&execution.executor.key)
                .ok_or_else(|| fail("executor contract missing"))?;
            match execution.executor.key.as_str() {
                "frustum_cull" => executions.push(PreparedExecution::FrustumCull),
                "mesh_query" => executions.push(PreparedExecution::MeshQuery),
                _ if execution.executor.key == "frame_out"
                    || contract.fullscreen_policy.is_some() =>
                {
                    let frame_out = execution.executor.key == "frame_out";
                    let (source, second) = if frame_out {
                        let ExecutionKind::FrameOut { color } = execution.kind else {
                            return Err(fail("frame_out kind mismatch"));
                        };
                        (color, color)
                    } else {
                        let sampled: Vec<_> = contract
                            .inputs
                            .iter()
                            .enumerate()
                            .filter(|(_, input)| {
                                input.role == crate::render_graph::InputRole::SampledTexture
                            })
                            .map(|(index, _)| {
                                execution.inputs.get(index).map(|input| input.resource)
                            })
                            .collect::<Option<_>>()
                            .ok_or_else(|| fail("fullscreen inputs mismatch"))?;
                        match (contract.fullscreen_policy, sampled.as_slice()) {
                            (
                                Some(crate::render_graph::FullscreenPolicy::BloomComposite),
                                [source, second],
                            ) => (*source, *second),
                            (Some(_), [source]) => (*source, *source),
                            _ => return Err(fail("fullscreen inputs mismatch")),
                        }
                    };
                    let values =
                        pack_fullscreen_uniforms(&execution.executor.key, &execution.parameters)
                            .ok_or_else(|| fail("executor parameters mismatch"))?;
                    use wgpu::util::DeviceExt;
                    let uniform =
                        self.context
                            .device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some(" post parameters"),
                                contents: bytemuck::bytes_of(&values),
                                usage: wgpu::BufferUsages::UNIFORM,
                            });
                    let target_format = if frame_out {
                        runtime.surface.format
                    } else {
                        let ExecutionKind::Render {
                            color_attachments, ..
                        } = &execution.kind
                        else {
                            return Err(fail("fullscreen execution is not render"));
                        };
                        let target = color_attachments
                            .first()
                            .ok_or_else(|| fail("fullscreen target missing"))?
                            .resource;
                        let a = runtime
                            .allocations
                            .resource_allocations
                            .get(target as usize)
                            .copied()
                            .flatten()
                            .ok_or_else(|| fail("fullscreen target allocation missing"))?;
                        runtime
                            .allocations
                            .classes
                            .get(a.class as usize)
                            .and_then(|class| class.slots.get(a.slot as usize))
                            .ok_or_else(|| fail("fullscreen target allocation is invalid"))?
                            .descriptor
                            .format
                    };
                    let entry = resolve_fullscreen_entry(&execution.executor.key)
                        .ok_or_else(|| fail("fullscreen executor mismatch"))?;
                    let pipeline = self.context.device.create_render_pipeline(
                        &wgpu::RenderPipelineDescriptor {
                            label: Some(" post pipeline"),
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
                                label: Some(" fullscreen source"),
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
                    executions.push(PreparedExecution::Fullscreen {
                        execution: index,
                        frame_out,
                        bind_group,
                        pipeline,
                        _uniform: uniform,
                    });
                }
                "pipeline_registry" => executions.push(PreparedExecution::PipelineRegistry),
                "pipeline" => {
                    let ExecutionKind::Render {
                        color_attachments,
                        depth_stencil,
                    } = &execution.kind
                    else {
                        return Err(fail("pipeline is not render"));
                    };
                    let color = color_attachments
                        .first()
                        .ok_or_else(|| fail("pipeline color missing"))?;
                    let color_format = {
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
                    let NormalizedParameters::Pipeline {
                        pipeline: _,
                        depth_compare,
                        depth_write_enabled,
                        ..
                    } = &execution.parameters
                    else {
                        return Err(fail("pipeline parameters mismatch"));
                    };
                    let base = resolved_pipelines
                        .get(index)
                        .copied()
                        .flatten()
                        .ok_or_else(|| fail("resolved pipeline missing"))?;
                    let compare = match depth_compare {
                        CompareFunction::Never => wgpu::CompareFunction::Never,
                        CompareFunction::Less => wgpu::CompareFunction::Less,
                        CompareFunction::LessEqual => wgpu::CompareFunction::LessEqual,
                        CompareFunction::Greater => wgpu::CompareFunction::Greater,
                        CompareFunction::GreaterEqual => wgpu::CompareFunction::GreaterEqual,
                        CompareFunction::Equal => wgpu::CompareFunction::Equal,
                        CompareFunction::NotEqual => wgpu::CompareFunction::NotEqual,
                        CompareFunction::Always => wgpu::CompareFunction::Always,
                    };
                    let variant = self
                        .resources
                        .create_target_variant(
                            &self.context.device,
                            base,
                            color_format,
                            depth_format,
                            compare,
                            *depth_write_enabled,
                        )
                        .map_err(|e| GraphError::new("GRAPH_RUNTIME_PLAN_INVALID", e))?;
                    executions.push(PreparedExecution::Pipeline {
                        execution: index,
                        base,
                        variant,
                    });
                }
                _ => return Err(fail("unsupported prepared execution")),
            }
        }
        Ok(ActiveCompiledGraph {
            id,
            graph,
            runtime,
            textures,
            executions,
            _fullscreen_layout: fullscreen_layout,
        })
    }

    fn begin_compiled_preparation(
        &mut self,
        id: crate::render_graph::CompiledGraphId,
        graph: crate::render_graph::CompiledGraph,
        purpose: PreparationPurpose,
    ) -> Result<(), crate::render_graph::GraphError> {
        let runtime = self.plan_compiled(&graph)?;
        // Candidate construction allocates GPU resources, so the live scene preflight
        // belongs here: this is the earliest boundary with both the runtime query and
        // scene access, and precedes GPU work and all pending/in-flight mutation.
        resolve_culling_frustum(runtime.allocations.query, || self.scene.frustum_planes())?;
        let restart_graph = graph.clone();
        self.next_preparation_token = self.next_preparation_token.wrapping_add(1).max(1);
        let token = self.next_preparation_token;
        self.context
            .device
            .push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        self.context
            .device
            .push_error_scope(wgpu::ErrorFilter::Validation);
        let candidate = self.create_compiled_candidate(id, graph, runtime);
        let validation = self.context.device.pop_error_scope();
        let out_of_memory = self.context.device.pop_error_scope();
        self.in_flight = Some(InFlightPreparation {
            token,
            id,
            purpose,
            graph: restart_graph,
        });
        let completions = self.preparation_completions.clone();
        spawn_local(async move {
            let validation_error = validation.await.map(|error| error.to_string());
            let out_of_memory_error = out_of_memory.await.map(|error| error.to_string());
            completions.borrow_mut().push(PreparationCompletion {
                token,
                purpose,
                candidate,
                validation_error,
                out_of_memory_error,
            });
        });
        Ok(())
    }

    fn drain_preparation_completions(&mut self) {
        let completions = std::mem::take(&mut *self.preparation_completions.borrow_mut());
        for completion in completions {
            let Some(in_flight) = self.in_flight.as_ref() else {
                continue;
            };
            if in_flight.token != completion.token {
                continue;
            }
            self.in_flight = None;
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
                (PreparationPurpose::Switch { request }, Ok(candidate)) => {
                    self.pending_switch = Some(PendingSwitch {
                        request,
                        target: SwitchTarget::Compiled(candidate),
                    });
                }
                (PreparationPurpose::Switch { request }, Err(error)) => {
                    self.reply(request, Err(error.into()));
                }
                (PreparationPurpose::Resize, Ok(candidate)) => {
                    self.active_compiled = Some(candidate);
                }
                (PreparationPurpose::Resize, Err(error)) => {
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

        let mut resources = PipelineLibrary::new();
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
            in_flight: None,
            next_preparation_token: 0,
            preparation_completions: Default::default(),
            halted: false,
            profiler,
        }
    }

    fn render(&mut self, _time: f32) {
        if self.halted {
            return;
        }
        self.drain_preparation_completions();
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
            Some(active) => format!(
                "graph:{}:{}:{}:{}",
                active.graph.graph_id, active.graph.revision, active.id.slot, active.id.generation
            ),
        });
        let encode_result = if let Some(active) = rendering_compiled {
            executors::encode_compiled(
                &mut encoder,
                &texture_view,
                active,
                &self.scene,
                &self.gpu_scene,
                &self.resources,
                &self.materials,
                profile_frame.as_mut(),
            )
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
                self.in_flight
                    .take()
                    .and_then(|preparation| match preparation.purpose {
                        PreparationPurpose::Switch { request } => {
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
                        PreparationPurpose::Resize => Some((preparation.id, preparation.graph)),
                    });
            let mut restarted = false;
            if let Some(old) = self.active_compiled.take() {
                let id = old.id();
                // Keep immediate resources live and fall back for this frame if recreation fails.
                restarted = true;
                self.begin_compiled_preparation(id, old.graph, PreparationPurpose::Resize)
                    .unwrap_or_else(|error| {
                        log::error!(
                            "compiled graph resize preparation failed: {}",
                            error.message
                        )
                    });
            }
            if !restarted {
                if let Some((id, graph)) = interrupted_resize {
                    if let Err(error) =
                        self.begin_compiled_preparation(id, graph, PreparationPurpose::Resize)
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
