#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;

use renderer::camera::Camera;
use renderer::render_data::RenderData;
use renderer::renderer as gpu_renderer;
use renderer::renderer::scene::FrameMetadata;

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
        _render_data: &mut RenderData,
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

        EditorScene {
            uniform_buffers: [uniform_resource.buffer, camera_resource.buffer],
            bind_groups: [uniform_resource.bind_group, camera_resource.bind_group],
            frame_metadata,
            cam: camera,
        }
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

/// Start the level editor inside its owning render worker.
#[wasm_bindgen]
pub fn worker_main(profile: bool) -> u32 {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    wasm_logger::init(wasm_logger::Config::default());
    renderer::app_setup::worker_entrypoint::<EditorScene>(profile)
}

/// Return this worker's shared WebAssembly memory to messaging clients.
#[wasm_bindgen]
pub fn worker_memory() -> JsValue {
    wasm_bindgen::memory()
}
