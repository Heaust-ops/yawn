use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::mpsc::Receiver};

use futures::channel::oneshot;
use log::info;
use ultraviolet::Vec4;
use wasm_bindgen::{prelude::Closure, JsCast, JsValue};
use wasm_bindgen_futures::spawn_local;
use web_sys::DedicatedWorkerGlobalScope;

use crate::{
    command_ring::CommandRing,
    gltf::{decode_gltf, install_imported, ModelBounds},
    message::{DrainEventError, MouseMessage, ResizeMessage, WindowEvent},
    render_data::{
        InstanceHandle, MeshHandle, PipelineKey, RenderData, RenderDataConfig, RenderFlags,
    },
    renderer::scene::Scene,
};

pub mod gpu_scene;
pub mod scene;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

struct ActiveCompiled {
    id: crate::render_graph::CompiledGraphId,
    graph: crate::render_graph::CompiledGraph,
    _textures: Vec<wgpu::Texture>,
    views: Vec<wgpu::TextureView>,
    class_bases: Vec<usize>,
}

struct PooledTransient {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

enum SwitchTarget {
    Immediate,
    Compiled(ActiveCompiled),
}

struct PendingSwitch {
    request: u32,
    target: SwitchTarget,
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

pub struct GpuResources {
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
    gpu_scene: gpu_scene::GpuSceneCache,
    pub(crate) command_ring: Option<&'static CommandRing>,
    pending_replies: Vec<JsValue>,
    gpu_error: std::sync::Arc<std::sync::atomic::AtomicBool>,
    framing_radius: f32,
    graph_registry: crate::render_graph::Registry,
    active_compiled: Option<ActiveCompiled>,
    pending_switch: Option<PendingSwitch>,
    transient_pool: HashMap<crate::render_graph::RuntimeTextureKey, Vec<PooledTransient>>,
    halted: bool,
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
                    if self.active_compiled.as_ref().is_some_and(|a| a.id == id) {
                        Err(crate::render_graph::GraphError::new(
                            "GRAPH_ACTIVE",
                            "compiled graph is active",
                        ))
                    } else if self.pending_switch.as_ref().is_some_and(
                        |p| matches!(&p.target, SwitchTarget::Compiled(a) if a.id == id),
                    ) {
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
                let outcome = if self.pending_switch.is_some() {
                    Err(crate::render_graph::GraphError::new(
                        "GRAPH_SWITCH_PENDING",
                        "a graph switch is pending",
                    ))
                } else if words[2] == 0 {
                    if words[3] != 0 || words[4] != 0 {
                        Err(crate::render_graph::GraphError::new(
                            "STALE_GRAPH_ID",
                            "immediate mode requires a zero id",
                        ))
                    } else {
                        self.pending_switch = Some(PendingSwitch {
                            request,
                            target: SwitchTarget::Immediate,
                        });
                        Ok(())
                    }
                } else if words[2] == 1 {
                    let id = crate::render_graph::CompiledGraphId {
                        slot: words[3],
                        generation: words[4],
                    };
                    self.prepare_compiled(id).map(|active| {
                        self.pending_switch = Some(PendingSwitch {
                            request,
                            target: SwitchTarget::Compiled(active),
                        })
                    })
                } else {
                    Err(crate::render_graph::GraphError::new(
                        "GRAPH_EXECUTION_UNSUPPORTED",
                        "unknown render mode",
                    ))
                };
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
                    let imported = decode_gltf(&bytes).map_err(|_| "GLB_INVALID")?;
                    let layout = gpu_scene::vertex_layouts();
                    let culled_pipeline = self.resources.get_or_create_pipeline(
                        &self.context.device,
                        "gltf_standard",
                        &layout,
                        include_str!("../gltf.wgsl"),
                        self.context.surface_config.format,
                    );
                    let double_sided_pipeline = self.resources.get_or_create_pipeline(
                        &self.context.device,
                        "gltf_standard_double_sided",
                        &layout,
                        include_str!("../gltf.wgsl"),
                        self.context.surface_config.format,
                    );
                    let installed = install_imported(
                        &mut self.render_data,
                        &imported,
                        [culled_pipeline, double_sided_pipeline],
                    )
                    .map_err(|_| "INSTALL_FAILED")?;
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

    fn prepare_compiled(
        &mut self,
        id: crate::render_graph::CompiledGraphId,
    ) -> Result<ActiveCompiled, crate::render_graph::GraphError> {
        let graph = self.graph_registry.get(id)?.clone();
        self.prepare_compiled_snapshot(id, graph)
    }

    fn prepare_compiled_snapshot(
        &mut self,
        id: crate::render_graph::CompiledGraphId,
        graph: crate::render_graph::CompiledGraph,
    ) -> Result<ActiveCompiled, crate::render_graph::GraphError> {
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
        Ok(ActiveCompiled {
            id,
            graph,
            _textures: textures,
            views,
            class_bases,
        })
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn new(canvas: web_sys::OffscreenCanvas, events_chan: Receiver<WindowEvent>) -> Self {
        let id = wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU,
            ..Default::default()
        };

        let instance = wgpu::Instance::new(&id);
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::OffscreenCanvas(canvas.clone()))
            .unwrap();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                ..Default::default()
            })
            .await
            .unwrap();

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

        let (device, queue) = adapter.request_device(&descriptor).await.unwrap();
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

        Self {
            canvas,
            events_chan,
            context,
            scene,
            resources,
            render_data,
            snapshot: crate::shared_snapshot::SharedSnapshot::new(),
            snapshot_init_sent: false,
            gpu_scene: Default::default(),
            command_ring: None,
            pending_replies: Vec::new(),
            gpu_error,
            framing_radius: 0.0,
            graph_registry: Default::default(),
            active_compiled: None,
            pending_switch: None,
            transient_pool: HashMap::new(),
            halted: false,
        }
    }

    fn render(&mut self, _time: f32) {
        if self.halted {
            return;
        }
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
        match self.snapshot.publish(&self.render_data) {
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
        self.scene.update(&self.context);
        if let Err(error) =
            self.gpu_scene
                .upload(&self.context.device, &self.context.queue, &self.render_data)
        {
            log::error!("GPU scene upload failed: {error}");
            self.post_fatal("GPU_UPLOAD_FAILED", &error);
            return;
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
        if let Some(active) = rendering_compiled {
            for pass in &active.graph.passes {
                let depth_resource = pass
                    .writes
                    .iter()
                    .find(|w| w.binding == "depth")
                    .unwrap()
                    .resource as usize;
                let allocation = active.graph.resources[depth_resource].allocation.unwrap();
                let depth_view = &active.views
                    [active.class_bases[allocation.class as usize] + allocation.slot as usize];
                let color = pass.writes.iter().find(|w| w.binding == "color").unwrap();
                let depth = pass.writes.iter().find(|w| w.binding == "depth").unwrap();
                let (color_load, color_store) = match &color.access {
                    crate::render_graph::WriteAccess::ColorAttachment { load, store, .. } => (
                        match load {
                            crate::render_graph::ColorLoad::Clear { value } => {
                                wgpu::LoadOp::Clear(wgpu::Color {
                                    r: value[0],
                                    g: value[1],
                                    b: value[2],
                                    a: value[3],
                                })
                            }
                            crate::render_graph::ColorLoad::Load => wgpu::LoadOp::Load,
                        },
                        if matches!(store, crate::render_graph::StoreOp::Store) {
                            wgpu::StoreOp::Store
                        } else {
                            wgpu::StoreOp::Discard
                        },
                    ),
                    _ => unreachable!(),
                };
                let (depth_load, depth_store) = match &depth.access {
                    crate::render_graph::WriteAccess::DepthAttachment { load, store } => (
                        match load {
                            crate::render_graph::DepthLoad::Clear { value } => {
                                wgpu::LoadOp::Clear(*value)
                            }
                            crate::render_graph::DepthLoad::Load => wgpu::LoadOp::Load,
                        },
                        if matches!(store, crate::render_graph::StoreOp::Store) {
                            wgpu::StoreOp::Store
                        } else {
                            wgpu::StoreOp::Discard
                        },
                    ),
                    _ => unreachable!(),
                };
                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some(&pass.id),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        depth_slice: None,
                        view: &texture_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: color_load,
                            store: color_store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: depth_load,
                            store: depth_store,
                        }),
                        stencil_ops: None,
                    }),
                    occlusion_query_set: None,
                    timestamp_writes: None,
                });
                self.encode_scene_forward(&mut render_pass);
            }
        } else {
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

            self.encode_scene_forward(&mut render_pass);
        }
        self.context.queue.submit(std::iter::once(encoder.finish()));
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
                        "compiledId":[active.id.slot, active.id.generation],
                        "graphId":active.graph.graph_id,
                        "revision":active.graph.revision
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
            .map(|a| js_sys::Array::of2(&a.id.slot.into(), &a.id.generation.into()).into())
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
                "activeCompiledGraph",
                active
                    .map(|a| a.graph.graph_id.as_str())
                    .unwrap_or("")
                    .into(),
            ),
            (
                "activeCompiledRevision",
                active.map(|a| a.graph.revision).unwrap_or(0).into(),
            ),
            (
                "graphPasses",
                active
                    .map(|a| a.graph.passes.len() as u32)
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
    }

    fn encode_scene_forward<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
        for (i, bind_group) in self.scene.bind_groups().iter().enumerate() {
            render_pass.set_bind_group(i as u32, bind_group, &[]);
        }
        if let (Some(p), Some(n), Some(u), Some(i), Some(inst)) = (
            &self.gpu_scene.positions.buffer,
            &self.gpu_scene.normals.buffer,
            &self.gpu_scene.uvs.buffer,
            &self.gpu_scene.indices.buffer,
            &self.gpu_scene.instances.buffer,
        ) {
            render_pass.set_vertex_buffer(0, p.slice(..));
            render_pass.set_vertex_buffer(1, n.slice(..));
            render_pass.set_vertex_buffer(2, u.slice(..));
            render_pass.set_vertex_buffer(3, inst.slice(..));
            render_pass.set_index_buffer(i.slice(..), wgpu::IndexFormat::Uint32);
            for draw in &self.gpu_scene.draws {
                render_pass.set_pipeline(self.resources.get_pipeline(draw.pipeline));
                render_pass.draw_indexed(
                    draw.indices.clone(),
                    draw.base_vertex,
                    draw.instances.clone(),
                );
            }
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
                r.scene.handle_zoom(msg.delta_y as f32);
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
            if let Some(old) = self.active_compiled.take() {
                let id = old.id;
                let graph = old.graph;
                // Keep immediate resources live and fall back for this frame if recreation fails.
                match self.prepare_compiled_snapshot(id, graph) {
                    Ok(active) => self.active_compiled = Some(active),
                    Err(error) => log::error!(
                        "compiled graph resize preparation failed: {}",
                        error.message
                    ),
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
        if (msg.buttons & 0x04) != 0 {
            let delta_x = (msg.movement_x * msg.scale_factor) as f32;
            let delta_y = (msg.movement_y * msg.scale_factor) as f32;
            self.scene.handle_orbit(delta_x, delta_y);
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
