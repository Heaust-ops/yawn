use std::cell::RefCell;
use wasm_bindgen::prelude::*;

thread_local! { static WEB_RUNTIME: RefCell<Option<renderer::app_setup::WebAppRuntime>> = const { RefCell::new(None) }; }

#[wasm_bindgen]
pub fn load_scene(url: String) {
    WEB_RUNTIME.with(|runtime| {
        if let Some(runtime) = runtime.borrow().as_ref() {
            let worker = runtime.worker();
            let message = js_sys::Object::new();
            let result = js_sys::Reflect::set(&message, &"type".into(), &"load-scene".into())
                .and_then(|_| js_sys::Reflect::set(&message, &"url".into(), &url.into()))
                .and_then(|_| worker.post_message(&message));
            if let Err(error) = result {
                log::error!("failed to request scene load: {:?}", error);
            }
        }
    });
}

#[wasm_bindgen]
pub fn worker_register_scene_payload(load_id: u32, bytes: &[u8]) {
    renderer::renderer::register_scene_payload(load_id, bytes);
}

#[wasm_bindgen]
pub fn worker_report_scene_error(load_id: u32, message: String) {
    renderer::renderer::report_scene_payload_error(load_id, message);
}

#[wasm_bindgen]
pub fn worker_maintain_renderer() {
    renderer::renderer::worker_maintain_renderer();
}

#[wasm_bindgen]
pub fn worker_report_pick(
    request_id: u32,
    status: String,
    slot: u32,
    generation: u32,
    snapshot_id: u32,
    publication_version: u32,
) {
    renderer::renderer::report_pick(
        request_id,
        status,
        slot,
        generation,
        snapshot_id,
        publication_version,
    );
}

use renderer::app_setup::WebApp;
use renderer::camera::Camera;
use renderer::renderer as gpu_renderer;
use renderer::renderer::scene::FrameMetadata;

pub struct EditorScene {
    uniform_buffers: [wgpu::Buffer; 2],
    bind_groups: [wgpu::BindGroup; 2],
    frame_metadata: FrameMetadata,
    cam: Camera,
}

impl renderer::renderer::scene::Scene for EditorScene {
    fn setup(
        renderer_context: &gpu_renderer::RendererContext,
        resources: &mut gpu_renderer::GpuResources,
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

        let scene = EditorScene {
            uniform_buffers: [uniform_resource.buffer, camera_resource.buffer],
            bind_groups: [uniform_resource.bind_group, camera_resource.bind_group],
            frame_metadata,
            cam: camera,
        };

        scene
    }

    fn frame_metadata_mut(&mut self) -> Option<&mut FrameMetadata> {
        Some(&mut self.frame_metadata)
    }

    fn camera_mut(&mut self) -> Option<&mut Camera> {
        Some(&mut self.cam)
    }

    fn uniform_buffers(&self) -> Option<&[wgpu::Buffer]> {
        Some(&self.uniform_buffers)
    }

    fn bind_groups(&self) -> &[wgpu::BindGroup] {
        &self.bind_groups
    }

    fn handle_mouse_click(&mut self, x: f32, y: f32) {
        self.frame_metadata.mouse_click = [x, y];
    }

    fn handle_zoom(&mut self, message: &renderer::message::WheelMessage) {
        self.cam.zoom(message);
    }

    fn handle_orbit(&mut self, delta_x: f32, delta_y: f32) {
        self.cam.orbit(delta_x, delta_y);
    }

    fn set_camera_depth_range(&mut self, near: f32, far: f32) {
        self.cam.set_depth_range(near, far);
    }

    fn set_camera_look_at(&mut self, eye: ultraviolet::Vec3, center: ultraviolet::Vec3) {
        self.cam.look_at(eye, center);
    }
}

#[cfg(target_arch = "wasm32")]
pub struct LevelEditor {
    #[allow(dead_code)]
    scene: EditorScene,
}

#[cfg(target_arch = "wasm32")]
impl WebApp for LevelEditor {
    type Scene = EditorScene;
}

/// Entrypoint for the level editor
#[wasm_bindgen]
pub fn main() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    wasm_logger::init(wasm_logger::Config::default());

    wasm_bindgen_futures::spawn_local(async {
        let runtime = match LevelEditor::setup_runtime() {
            Ok(runtime) => runtime,
            Err(error) => {
                log::error!("failed to start level editor runtime: {:?}", error);
                let global = js_sys::global();
                if let Ok(callback) = js_sys::Reflect::get(&global, &"rendererStartupFailed".into())
                {
                    if callback.is_function() {
                        if let Err(callback_error) = callback
                            .unchecked_into::<js_sys::Function>()
                            .call1(&global, &error)
                        {
                            log::error!(
                                "failed to report renderer startup failure: {callback_error:?}"
                            );
                        }
                    }
                }
                return;
            }
        };
        WEB_RUNTIME.with(|stored| *stored.borrow_mut() = Some(runtime));
    });
}

renderer::export_worker_entrypoint!();
