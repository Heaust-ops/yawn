use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;

#[path = "render_graph/compiler.rs"]
mod compiler;
#[path = "renderer/wgpu.rs"]
mod gpu;
#[path = "render_graph/gpu_resource.rs"]
mod gpu_resource;
#[path = "render_graph/render_graph.rs"]
mod graph;
#[path = "renderer/render.rs"]
mod render;
mod render_data;
#[path = "render_graph/store.rs"]
mod store;

use gpu::Wgpu;
use render::RenderLoop;
use render_data::RenderData;
use store::Store;

#[wasm_bindgen]
pub struct Core {
    data: Rc<RefCell<RenderData>>,
    gpu: Rc<RefCell<Option<Wgpu>>>,
    store: Rc<RefCell<Store>>,
    render: Rc<RenderLoop>,
}

#[wasm_bindgen]
impl Core {
    #[wasm_bindgen(constructor)]
    pub fn new(arena_bytes: u32) -> Result<Self, JsError> {
        Ok(Self {
            data: Rc::new(RefCell::new(
                RenderData::new(arena_bytes).map_err(JsError::new)?,
            )),
            gpu: Rc::new(RefCell::new(None)),
            store: Rc::new(RefCell::new(Store::default())),
            render: Rc::new(RenderLoop::new()),
        })
    }

    pub async fn initialize(&self, canvas: web_sys::OffscreenCanvas) -> Result<(), JsError> {
        let gpu = Wgpu::new(canvas)
            .await
            .map_err(|error| JsError::new(&error))?;
        *self.gpu.borrow_mut() = Some(gpu);
        self.render
            .start(self.gpu.clone(), self.store.clone(), self.data.clone());
        Ok(())
    }

    pub fn rows(&self) -> String {
        serde_json::to_string(&self.data.borrow().descriptors()).unwrap()
    }

    pub fn create_rows(
        &self,
        name: String,
        rows: u32,
        stride: u32,
        format: String,
    ) -> Result<String, JsError> {
        let rows = self
            .data
            .borrow_mut()
            .create_rows(name, rows, stride, format)
            .map_err(JsError::new)?;
        if let Some(gpu) = self.gpu.borrow().as_ref() {
            self.store
                .borrow_mut()
                .refresh_rows(&rows.name, gpu, &self.data.borrow())
                .map_err(|error| JsError::new(&error))?;
        }
        Ok(serde_json::to_string(&rows).unwrap())
    }

    pub fn delete_rows(&self, name: String) -> Result<(), JsError> {
        if self.store.borrow().uses_rows(&name) {
            return Err(JsError::new("ROWS_ACTIVE"));
        }
        self.data
            .borrow_mut()
            .delete_rows(&name)
            .map_err(JsError::new)
    }

    pub fn allocate_object(&self, name: &str) -> Result<String, JsError> {
        let (id, grew) = self
            .data
            .borrow_mut()
            .allocate_object(name)
            .map_err(JsError::new)?;
        let rows = self
            .data
            .borrow()
            .rows(name)
            .ok_or_else(|| JsError::new("ROWS_UNKNOWN"))?
            .clone();
        if grew {
            if let Some(gpu) = self.gpu.borrow().as_ref() {
                self.store
                    .borrow_mut()
                    .refresh_rows(name, gpu, &self.data.borrow())
                    .map_err(|error| JsError::new(&error))?;
            }
        }
        Ok(serde_json::json!({ "id": id, "rows": rows }).to_string())
    }

    pub fn delete_object(&self, name: &str, id: u32) -> Result<(), JsError> {
        self.data
            .borrow_mut()
            .delete_object(name, id)
            .map_err(JsError::new)
    }

    pub fn compile_graph(&self, source: &str) -> Result<String, JsError> {
        let graph = compiler::compile(source).map_err(JsError::new)?;
        Ok(self.store.borrow_mut().save(graph))
    }

    pub fn switch_loadout(&self, id: &str) -> Result<(), JsError> {
        let gpu = self.gpu.borrow();
        let gpu = gpu
            .as_ref()
            .ok_or_else(|| JsError::new("WEBGPU_UNINITIALIZED"))?;
        self.store
            .borrow_mut()
            .switch(id, gpu, &self.data.borrow())
            .map_err(|error| JsError::new(&error))
    }

    pub fn play(&self) {
        self.render.play();
    }

    pub fn pause(&self) {
        self.render.pause();
    }

    pub fn set_fps(&self, fps: u32) -> Result<(), JsError> {
        self.render.set_fps(fps).map_err(JsError::new)
    }
}
