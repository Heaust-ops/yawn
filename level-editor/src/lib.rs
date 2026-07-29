#![cfg(target_arch = "wasm32")]

use ultraviolet::Mat4;
use wasm_bindgen::prelude::*;

use renderer::app_setup::WebAppRuntime;
use renderer::camera::Camera;
use renderer::render_data::{InstanceType, MeshCreateInfo, RenderData};
use renderer::renderer as gpu_renderer;
use renderer::renderer::gpu_scene::vertex_layouts;
use renderer::renderer::scene::FrameMetadata;

/// Simple vertex format.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    pos: [f32; 3],
}

struct EditorScene {
    uniform_buffers: [wgpu::Buffer; 2],
    bind_groups: [wgpu::BindGroup; 2],
    frame_metadata: FrameMetadata,
    cam: Camera,
}

impl renderer::renderer::scene::Scene for EditorScene {
    fn setup(
        renderer_context: &gpu_renderer::RendererContext,
        resources: &mut gpu_renderer::PipelineLibrary,
        render_data: &mut RenderData,
    ) -> Self {
        let dimension = ultraviolet::Vec2::new(
            renderer_context.surface_config.width as f32,
            renderer_context.surface_config.height as f32,
        );

        let mut frame_metadata = FrameMetadata::new(dimension);
        let camera = Camera::new(dimension.x / dimension.y);

        frame_metadata.set_camera_position(camera.position());

        let uniform_resource = frame_metadata.create_uniform_resource(&renderer_context.device);
        let camera_resource = camera.create_uniform_resource(&renderer_context.device);

        let bind_group_layouts = [
            uniform_resource.bind_group_layout,
            camera_resource.bind_group_layout,
        ];

        resources.set_bind_group_layouts(&bind_group_layouts);

        let mut scene = EditorScene {
            uniform_buffers: [uniform_resource.buffer, camera_resource.buffer],
            bind_groups: [uniform_resource.bind_group, camera_resource.bind_group],
            frame_metadata,
            cam: camera,
        };

        scene.create_default_scene(
            &renderer_context.device,
            resources,
            render_data,
            renderer_context.surface_config.format,
        );

        scene
    }

    fn frame_metadata_mut(&mut self) -> Option<&mut FrameMetadata> {
        Some(&mut self.frame_metadata)
    }

    fn camera_mut(&mut self) -> Option<&mut Camera> {
        Some(&mut self.cam)
    }

    fn uniform_buffers(&self) -> Option<[&wgpu::Buffer; 2]> {
        Some([&self.uniform_buffers[0], &self.uniform_buffers[1]])
    }

    fn bind_groups(&self) -> &[wgpu::BindGroup] {
        &self.bind_groups
    }

    fn handle_mouse_click(&mut self, x: f32, y: f32) {
        self.frame_metadata.mouse_click = [x, y];
    }

    fn handle_zoom(&mut self, delta_y: f32) {
        self.cam.zoom(delta_y);
    }

    fn handle_orbit(&mut self, delta_x: f32, delta_y: f32) {
        self.cam.orbit(delta_x, delta_y);
    }

    fn handle_pan(&mut self, delta_x: f32, delta_y: f32, viewport_height: f32) {
        self.cam.pan(delta_x, delta_y, viewport_height);
    }

    fn set_camera_depth_range(&mut self, near: f32, far: f32) {
        self.cam.set_depth_range(near, far);
    }

    fn set_camera_look_at(&mut self, eye: ultraviolet::Vec3, center: ultraviolet::Vec3) {
        self.cam.look_at(eye, center);
    }
}

impl EditorScene {
    /// Ground plane vertex data.
    const VERTICES: &[Vertex] = &[
        // First triangle of quad
        Vertex {
            pos: [-5.0, 0.0, -5.0],
        },
        Vertex {
            pos: [5.0, 0.0, -5.0],
        },
        Vertex {
            pos: [-5.0, 0.0, 5.0],
        },
        // Second triangle of quad
        Vertex {
            pos: [5.0, 0.0, -5.0],
        },
        Vertex {
            pos: [5.0, 0.0, 5.0],
        },
        Vertex {
            pos: [-5.0, 0.0, 5.0],
        },
    ];
    // Wind the ground plane so the upward-facing side is front-facing (CCW from
    // above) to avoid being culled by the default back-face culling.
    const INDICES: &[u32] = &[0, 2, 1, 3, 5, 4];

    fn create_default_scene(
        &mut self,
        device: &wgpu::Device,
        resources: &mut gpu_renderer::PipelineLibrary,
        render_data: &mut RenderData,
        surface_format: wgpu::TextureFormat,
    ) {
        let positions: Vec<[f32; 3]> = Self::VERTICES.iter().map(|v| v.pos).collect();
        // Ground plane normals point upward (Y+)
        let normals: Vec<[f32; 3]> = vec![[0.0, 1.0, 0.0]; positions.len()];
        let tangents: Vec<[f32; 4]> = vec![[1.0, 0.0, 0.0, 1.0]; positions.len()];
        let uvs: &[[f32; 2]] = &[
            [0.0, 0.0],
            [1.0, 0.0],
            [0.0, 1.0],
            [1.0, 0.0],
            [1.0, 1.0],
            [0.0, 1.0],
        ];

        let vertex_layout = vertex_layouts();

        let pipeline_index = resources.get_or_create_pipeline(
            device,
            "ground_plane",
            &vertex_layout,
            include_str!("./program.wgsl"),
            surface_format,
        );

        let scale_factor = 100.0;
        let scale_matrix = Mat4::from_scale(scale_factor);

        let transform: [[f32; 4]; 4] = scale_matrix.into();
        render_data
            .create_mesh(MeshCreateInfo {
                positions: &positions,
                normals: &normals,
                tangents: &tangents,
                uvs,
                indices: Self::INDICES,
                pipeline: pipeline_index,
                material: renderer::render_data::MaterialKey::DEFAULT,
                default_instance_type: InstanceType {
                    words: [1 | 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
                },
                default_transform: transform,
            })
            .expect("ground plane geometry is valid");
    }
}

/// Entrypoint for the level editor
#[wasm_bindgen]
pub fn main(profile: bool) -> Result<RendererBridge, JsValue> {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    wasm_logger::init(wasm_logger::Config::default());

    let runtime = WebAppRuntime::new::<EditorScene>("main-worker", "#canvas0", profile)?;
    Ok(RendererBridge { runtime })
}

/// Opaque owner of the worker, event listeners, and pinned command ring.
#[wasm_bindgen]
pub struct RendererBridge {
    runtime: renderer::app_setup::WebAppRuntime,
}

#[wasm_bindgen]
impl RendererBridge {
    #[wasm_bindgen(getter)]
    pub fn worker(&self) -> web_sys::Worker {
        web_sys::Worker::clone(&*self.runtime.worker())
    }
    #[wasm_bindgen(getter, js_name = ringPtr)]
    pub fn ring_ptr(&self) -> u32 {
        self.runtime.ring_ptr()
    }
    #[wasm_bindgen(getter)]
    pub fn memory(&self) -> JsValue {
        wasm_bindgen::memory()
    }
}

renderer::export_worker_entrypoint!();
